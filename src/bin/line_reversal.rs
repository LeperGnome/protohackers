use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio::{net::UdpSocket, sync::mpsc};

#[derive(Debug)]
enum Message {
    Connect,
    Data(Payload),
    Ack(u32),
    Close,
}
impl Message {
    fn from_buf(buf: &[u8]) -> Result<(u32, Message), ()> {
        let msg = std::str::from_utf8(&buf)
            .or(Err(()))?
            .strip_prefix("/")
            .ok_or(())?
            .trim_end() // TODO: use for debug
            .strip_suffix("/")
            .ok_or(())?;

        let mut tmp = msg.splitn(3, '/'); //.skip(1);
        let cmd = tmp.next();
        let session_id = tmp.next().ok_or(())?.parse::<u32>().or(Err(()))?;

        match cmd {
            Some(cmd) if cmd == "connect" => Ok((session_id, Message::Connect)),
            Some(cmd) if cmd == "data" => Ok((
                session_id,
                Message::Data(tmp.next().ok_or(())?.parse::<Payload>()?),
            )),
            Some(cmd) if cmd == "ack" => Ok((
                session_id,
                Message::Ack(tmp.next().ok_or(())?.parse::<u32>().or(Err(()))?),
            )),
            Some(cmd) if cmd == "close" => Ok((session_id, Message::Close)),
            Some(_) | None => Err(()),
        }
    }
    fn as_bytes(&self) -> &[u8] {
        todo!();
    }
}

#[derive(Debug)]
struct Payload {
    pos: u32,
    data: String,
}
impl std::str::FromStr for Payload {
    type Err = ();

    fn from_str(s: &str) -> Result<Payload, Self::Err> {
        match s.split_once("/") {
            Some((pos, data)) => Ok(Self {
                pos: pos.parse::<u32>().or(Err(()))?,
                data: data.to_string(),
            }),
            None => Err(()),
        }
    }
}

struct Session {
    id: u32,
    peer_addr: SocketAddr,
    tx: mpsc::Sender<Message>,
}

async fn handle_session(
    id: u32,
    peer_addr: SocketAddr,
    mut writer: mpsc::Sender<(Message, SocketAddr)>,
    mut reader: mpsc::Receiver<Message>,
) {
    while let Some(msg) = reader.recv().await {
        println!("[{id} : {peer_addr}] got message: '{:?}'", msg);
    }
}

#[tokio::main]
async fn main() {
    let sock = UdpSocket::bind("0.0.0.0:7878").await.unwrap();
    let sock = Arc::new(sock);

    let mut sessions: HashMap<u32, Session> = HashMap::new();
    let mut buf = [0; 1024];

    let (writer_tx, mut writer_rx) = mpsc::channel::<(Message, SocketAddr)>(100);

    let w_sock = sock.clone();

    _ = tokio::spawn(async move {
        while let Some((msg, addr)) = writer_rx.recv().await {
            if let Err(e) = w_sock.send_to(msg.as_bytes(), addr).await {
                eprintln!("Error sending message {:?} to {}, error: {}", msg, addr, e);
            }
        }
    });

    loop {
        let writer_tx = writer_tx.clone();
        let (amt, addr) = sock.recv_from(&mut buf).await.unwrap();
        if let Ok((session_id, msg)) = Message::from_buf(&buf[..amt]) {
            match msg {
                Message::Connect => {
                    let (session_tx, session_rx) = mpsc::channel::<Message>(100);
                    tokio::spawn(async move {
                        handle_session(session_id, addr, writer_tx.clone(), session_rx).await;
                    });
                    sessions
                        .entry(session_id)
                        .or_insert(Session {
                            peer_addr: addr,
                            id: session_id,
                            tx: session_tx,
                        })
                        .tx
                        .send(Message::Connect)
                        .await
                        .unwrap();
                }
                m @ _ => {
                    if let Some(session) = sessions.get(&session_id) {
                        session.tx.send(m).await.unwrap();
                    } else {
                        sock.send_to(format!("/close/{}/", session_id).as_bytes(), addr)
                            .await
                            .unwrap();
                    }
                }
            };
        } else {
            eprintln!("error parsing message");
        }
    }
}
