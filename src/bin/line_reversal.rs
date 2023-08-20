use std::{
    collections::HashMap,
    net::{SocketAddr, ToSocketAddrs},
    sync::Arc,
};
use tokio::{
    net::UdpSocket,
    sync::mpsc,
    time::{timeout, Duration, Instant},
};

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
            Some((pos, data)) => {
                if data.chars().filter(|c| *c == '/').count()
                    > data.chars().filter(|c| *c == '\\').count()
                {
                    return Err(());
                }
                Ok(Self {
                    pos: pos.parse::<usize>().or(Err(()))?,
                    data: unescape_data(data),
                })
            }
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
    s.replace(r"\", r"\\").replace(r"/", r"\/")
}

fn unescape_data(s: &str) -> String {
    s.replace(r"\/", r"/").replace(r"\\", r"\")
}

#[derive(Debug, Clone)]
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

        let mut tmp = msg.splitn(3, '/');
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

struct LRCPListner {
    sock: Arc<UdpSocket>,
    session_senders: HashMap<usize, mpsc::Sender<Message>>,
    buf: [u8; 1024],
}

impl LRCPListner {
    async fn bind<S: ToSocketAddrs>(addrs: S) -> Result<LRCPListner, String> {
        for addr in addrs.to_socket_addrs().or(Err("Failed parsing address"))? {
            if let Ok(sock) = UdpSocket::bind(addr).await {
                return Ok(LRCPListner {
                    sock: Arc::new(sock),
                    session_senders: HashMap::new(),
                    buf: [0; 1024],
                });
            }
        }
        Err("Could not bind to addr".into())
    }

    async fn listen(self) -> mpsc::Receiver<LRCPSession> {
        let (tx, rx) = mpsc::channel::<LRCPSession>(1024);
        tokio::spawn(async move {
            self.poll(tx).await;
        });
        return rx;
    }

    async fn poll(mut self, listen_tx: mpsc::Sender<LRCPSession>) {
        let (sock_writer_tx, mut sock_writer_rx) = mpsc::channel::<(Message, SocketAddr)>(100);

        let w_sock = self.sock.clone();

        // corutine that writes responses from all sessions to socket
        _ = tokio::spawn(async move {
            while let Some((msg, addr)) = sock_writer_rx.recv().await {
                if let Err(e) = w_sock.send_to(msg.to_string().as_bytes(), addr).await {
                    eprintln!("Error sending message {:?} to {}, error: {}", msg, addr, e);
                }
            }
        });

        loop {
            let (amt, addr) = self.sock.recv_from(&mut self.buf).await.unwrap();
            if let Ok(msg) = Message::from_buf(&self.buf[..amt]) {
                let session_id = msg.session_id;

                // registers new session
                if matches!(msg.data, MessageData::Connect)
                    && self.session_senders.get(&msg.session_id).is_none()
                {
                    self.register_session(addr, session_id, &listen_tx, sock_writer_tx.clone())
                        .await;
                }
                // transmit message to existing session
                if let Err(_) = self.transmit_to_session(msg).await {
                    self.sock
                        .send_to(
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
                    self.session_senders.remove(&session_id);
                }
            } else {
                eprintln!(
                    "error parsing message: '{}'",
                    std::str::from_utf8(&self.buf[..amt]).unwrap()
                );
            }
        }
    }

    async fn register_session(
        &mut self,
        addr: SocketAddr,
        session_id: usize,
        listen_tx: &mpsc::Sender<LRCPSession>,
        writer: LRCPSender,
    ) {
        let (session_tx, session_rx) = mpsc::channel::<Message>(100);
        listen_tx
            .send(LRCPSession::new(session_id, addr, writer, session_rx))
            .await
            .unwrap();
        self.session_senders.insert(session_id, session_tx);
    }

    async fn transmit_to_session(&self, msg: Message) -> Result<(), ()> {
        if let Some(sessoin_sender) = self.session_senders.get(&msg.session_id) {
            sessoin_sender.send(msg).await.or(Err(()))?;
            return Ok(());
        }
        return Err(());
    }
}

type LRCPSender = mpsc::Sender<(Message, SocketAddr)>;
type LRCPReceiver = mpsc::Receiver<Message>;

struct LRCPSession {
    id: usize,
    peer_addr: SocketAddr,
    writer: LRCPSender,
    reader: LRCPReceiver,

    max_client_pos: usize,
    max_client_ack: usize,
    max_server_pos: usize,
    last_sent_ack_n: usize,

    sent_bin: Vec<u8>,
    recv_bin: Vec<u8>,
    consumed: usize,
}
impl LRCPSession {
    fn new(id: usize, peer_addr: SocketAddr, writer: LRCPSender, reader: LRCPReceiver) -> Self {
        LRCPSession {
            id,
            peer_addr,
            reader,
            writer,
            max_server_pos: 0,
            max_client_ack: 0,
            max_client_pos: 0,
            last_sent_ack_n: 0,
            sent_bin: vec![],
            recv_bin: vec![],
            consumed: 0,
        }
    }
    async fn read(&mut self) -> Result<usize, String> {
        let mut req_len = None;
        while matches!(req_len, None) {
            if let Some(msg) = self.reader.recv().await {
                println!(
                    "[{} : {}] --> '{:?}'",
                    self.peer_addr, msg.session_id, msg.data
                );
                let resp_msg_data = match msg.data {
                    MessageData::Connect => Some(MessageData::Ack(0)),
                    MessageData::Data(p) => {
                        if p.pos == self.max_client_pos {
                            self.max_client_pos += p.data.len();
                            self.last_sent_ack_n = self.max_client_pos;
                            self.recv_bin.append(&mut p.data.as_bytes().to_vec());
                            req_len = Some(p.data.len());
                        }
                        Some(MessageData::Ack(self.last_sent_ack_n))
                    }
                    MessageData::Ack(n) if n <= self.max_client_ack => None,
                    MessageData::Ack(n) if n == self.max_server_pos => None,
                    MessageData::Ack(n) if n > self.max_server_pos => Some(MessageData::Close),
                    MessageData::Ack(n) if n < self.max_server_pos => {
                        Some(MessageData::Data(Payload {
                            pos: n,
                            data: std::str::from_utf8(&self.sent_bin[n..])
                                .unwrap()
                                .to_string(),
                        }))
                    }
                    MessageData::Ack(_) => None,
                    MessageData::Close => Some(MessageData::Close),
                };

                if let Some(data) = resp_msg_data {
                    println!("[{} : {}] <-- '{:?}'", self.peer_addr, msg.session_id, data);
                    self.writer
                        .send((
                            Message {
                                session_id: msg.session_id,
                                data,
                            },
                            self.peer_addr,
                        ))
                        .await
                        .unwrap();
                }
            }
        }
        Ok(req_len.unwrap())
    }

    async fn read_until(&mut self, c: char) -> Result<String, String> {
        let c = c as u8;
        loop {
            match self.get_form_buf(c).await {
                Some(data) => return Ok(data),
                None => self.read().await?,
            };
        }
    }

    async fn get_form_buf(&mut self, c: u8) -> Option<String> {
        let lo = self.consumed;
        for hi in lo..self.recv_bin.len() {
            if self.recv_bin[hi] == c {
                self.consumed = hi + 1;
                let msg = std::str::from_utf8(&self.recv_bin[lo..hi])
                    .unwrap()
                    .to_string();
                return Some(msg);
            }
        }
        return None;
    }

    async fn write(&mut self, message: &str) -> Result<(), String> {
        const RETRY_TIMES: u8 = 20;
        const ACK_WAIT: u64 = 3;

        let data = format!("{}\n", message);
        let msg_o = Message {
            session_id: self.id,
            data: MessageData::Data(Payload {
                pos: self.max_server_pos,
                data: data.clone(),
            }),
        };
        for i in 0..RETRY_TIMES {
            println!(
                "[{} : {}] <-- (#{i}) '{:?}'",
                self.peer_addr, msg_o.session_id, msg_o.data
            );
            self.writer
                .send((msg_o.clone(), self.peer_addr))
                .await
                .or(Err("Failed sending message =(".to_string()))?;

            let new_server_pos = self.max_server_pos + data.len();

            let ddline = Instant::now() + Duration::from_secs(ACK_WAIT);

            'att: loop {
                if let Ok(omsg) = timeout(Duration::from_secs(ACK_WAIT), self.reader.recv()).await {
                    if let Some(msg) = omsg {
                        match msg.data {
                            MessageData::Ack(n) if n == new_server_pos => {
                                self.sent_bin.append(&mut data.as_bytes().to_vec());
                                self.max_server_pos = new_server_pos;
                                return Ok(());
                            }
                            _ => (),
                        }
                    }
                } else {
                    break 'att;
                }

                if Instant::now() > ddline {
                    break 'att;
                }
            }
        }
        Err("Max retries exceeded".into())
    }
}

#[tokio::main]
async fn main() {
    let listner = LRCPListner::bind("0.0.0.0:7878").await.unwrap();
    let mut session_rx = listner.listen().await;

    while let Some(session) = session_rx.recv().await {
        tokio::spawn(async move {
            handle_session(session).await;
        });
    }
}

async fn handle_session(mut session: LRCPSession) {
    println!("[{} : {}] connected", session.peer_addr, session.id);

    while let Ok(msg) = session.read_until('\n').await {
        println!(
            "[{} : {}] got line: '{}'",
            session.peer_addr, session.id, msg
        );

        let rev_msg = msg.chars().rev().collect::<String>();
        if let Err(e) = session.write(&rev_msg).await {
            println!("[{} : {}] err on write: {e}", session.peer_addr, session.id);
            break;
        };
    }

    println!("[{} : {}] closed", session.peer_addr, session.id);
}
