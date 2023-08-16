use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio::{net::UdpSocket, sync::mpsc};

#[derive(Debug, Clone)]
enum MessageData {
    Connect,
    Data(Payload),
    Ack(usize),
    Close,
}

#[derive(Debug, Clone)]
struct Payload {
    pos: usize,
    data: String,
}
impl std::str::FromStr for Payload {
    type Err = ();

    fn from_str(s: &str) -> Result<Payload, Self::Err> {
        match s.split_once("/") {
            Some((pos, data)) => Ok(Self {
                pos: pos.parse::<usize>().or(Err(()))?,
                data: unescape_data(data),
            }),
            None => Err(()),
        }
    }
}
impl Payload {
    fn to_string(&self) -> String {
        return format!("{}/{}", self.pos, escape_data(&self.data));
    }
}

fn escape_data(s: &str) -> String {
    s.replace(r"/", r"\/").replace(r"\", r"\\")
}

fn unescape_data(s: &str) -> String {
    s.replace(r"\/", r"/").replace(r"\\", r"\")
}

#[derive(Debug)]
struct Message {
    session_id: usize,
    data: MessageData,
}

impl Message {
    fn from_buf(buf: &[u8]) -> Result<Message, ()> {
        let msg = std::str::from_utf8(&buf)
            .or(Err(()))?
            .strip_prefix("/")
            .ok_or(())?
            .trim_end() // TODO: used for debugging
            .strip_suffix("/")
            .ok_or(())?;

        let mut tmp = msg.splitn(3, '/'); //.skip(1);
        let cmd = tmp.next();
        let session_id = tmp.next().ok_or(())?.parse::<usize>().or(Err(()))?;

        match cmd {
            Some(cmd) if cmd == "connect" => Ok(Message {
                session_id,
                data: MessageData::Connect,
            }),
            Some(cmd) if cmd == "data" => Ok(Message {
                session_id,
                data: MessageData::Data(tmp.next().ok_or(())?.parse::<Payload>()?),
            }),
            Some(cmd) if cmd == "ack" => Ok(Message {
                session_id,
                data: MessageData::Ack(tmp.next().ok_or(())?.parse::<usize>().or(Err(()))?),
            }),
            Some(cmd) if cmd == "close" => Ok(Message {
                session_id,
                data: MessageData::Close,
            }),
            Some(_) | None => Err(()),
        }
    }
    fn to_string(&self) -> String {
        match &self.data {
            MessageData::Connect => format!("/connect/{}/", self.session_id),
            MessageData::Data(p) => format!("/data/{}/{}/", self.session_id, p.to_string()),
            MessageData::Ack(l) => format!("/ack/{}/{}/", self.session_id, l),
            MessageData::Close => format!("/close/{}/", self.session_id),
        }
    }
}

struct Session {
    tx: mpsc::Sender<Message>,
}

#[tokio::main]
async fn main() {
    let sock = UdpSocket::bind("0.0.0.0:7878").await.unwrap();
    let sock = Arc::new(sock);

    let mut sessions: HashMap<usize, Session> = HashMap::new();
    let mut buf = [0; 1024];

    let (writer_tx, mut writer_rx) = mpsc::channel::<(Message, SocketAddr)>(100);

    let w_sock = sock.clone();

    _ = tokio::spawn(async move {
        while let Some((msg, addr)) = writer_rx.recv().await {
            if let Err(e) = w_sock.send_to(msg.to_string().as_bytes(), addr).await {
                eprintln!("Error sending message {:?} to {}, error: {}", msg, addr, e);
            }
        }
    });

    loop {
        let (amt, addr) = sock.recv_from(&mut buf).await.unwrap();
        if let Ok(msg) = Message::from_buf(&buf[..amt]) {
            let session_id = msg.session_id;

            // registers session
            if matches!(msg.data, MessageData::Connect) && sessions.get(&msg.session_id).is_none() {
                register_session(&mut sessions, session_id, addr, writer_tx.clone()).await;
            }
            // transmit message to appropriate session
            if let Err(_) = transmit_to_session(&sessions, msg).await {
                sock.send_to(
                    Message {
                        session_id,
                        data: MessageData::Close,
                    }
                    .to_string()
                    .as_bytes(),
                    addr,
                )
                .await
                .unwrap();
                // TODO remove session?
            }
        } else {
            eprintln!("error parsing message");
        }
    }
}

async fn register_session(
    sessions: &mut HashMap<usize, Session>,
    session_id: usize,
    addr: SocketAddr,
    writer: mpsc::Sender<(Message, SocketAddr)>,
) {
    let (session_tx, session_rx) = mpsc::channel::<Message>(100);
    tokio::spawn(async move {
        handle_session(addr, writer, session_rx).await;
    });
    sessions.insert(session_id, Session { tx: session_tx });
}

async fn transmit_to_session(sessions: &HashMap<usize, Session>, msg: Message) -> Result<(), ()> {
    if let Some(session) = sessions.get(&msg.session_id) {
        session.tx.send(msg).await.or(Err(()))?;
        return Ok(());
    }
    return Err(());
}

async fn handle_session(
    peer_addr: SocketAddr,
    writer: mpsc::Sender<(Message, SocketAddr)>,
    mut reader: mpsc::Receiver<Message>,
) {
    let mut max_client_pos: usize = 0;
    let mut max_client_ack: usize = 0;
    let mut max_server_pos: usize = 0;

    let mut sent_bin: Vec<u8> = vec![];
    let mut recv_bin: Vec<u8> = vec![];

    let mut last_sent_ack = MessageData::Ack(0);

    while let Some(msg) = reader.recv().await {
        println!(
            "[{} : {peer_addr}] got message: '{:?}'",
            msg.session_id, msg.data
        );
        let resp_msg_data = match msg.data {
            MessageData::Connect => MessageData::Ack(0),
            MessageData::Data(p) => {
                if p.pos == max_client_pos {
                    max_client_pos += p.data.len();
                    last_sent_ack = MessageData::Ack(max_client_pos);
                    recv_bin.append(&mut p.data.as_bytes().to_vec());
                }
                last_sent_ack.clone()
            }
            MessageData::Ack(n) if n <= max_client_ack => continue,
            MessageData::Ack(n) if n == max_server_pos => continue,
            MessageData::Ack(n) if n > max_server_pos => MessageData::Close,
            MessageData::Ack(n) if n < max_server_pos => MessageData::Data(Payload {
                pos: n,
                data: std::str::from_utf8(&sent_bin[n..]).unwrap().to_string(),
            }),
            MessageData::Ack(_) => unreachable!(),
            MessageData::Close => MessageData::Close,
        };

        writer
            .send((
                Message {
                    session_id: msg.session_id,
                    data: resp_msg_data,
                },
                peer_addr,
            ))
            .await
            .unwrap();
    }
}
