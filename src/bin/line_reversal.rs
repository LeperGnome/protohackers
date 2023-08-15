use std::{net::SocketAddr, sync::Arc};
use tokio::net::UdpSocket;

#[derive(Debug)]
enum Message {
    Connect,
    Data(Payload),
    Ack(u32),
    Close,
}
impl Message {
    fn from_buf(buf: &[u8]) -> Result<(u32, Message), ()> {
        let msg = std::str::from_utf8(&buf).or(Err(()))?;

        let mut tmp = msg.splitn(4, '/').skip(1);
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
}

#[derive(Debug)]
struct Payload {
    pos: u32,
    data: String,
}
impl std::str::FromStr for Payload {
    type Err = ();

    fn from_str(s: &str) -> Result<Payload, Self::Err> {
        let s = s.strip_suffix("/").unwrap_or(s);
        match s.split_once("/") {
            Some((pos, data)) => Ok(Self {
                pos: pos.parse::<u32>().unwrap(),
                data: data.to_string(),
            }),
            None => Err(()),
        }
    }
}

struct Session {
    id: u32,
    peer_addr: SocketAddr,
    socket: Arc<UdpSocket>,
}

#[tokio::main]
async fn main() {
    let sock = UdpSocket::bind("0.0.0.0:7878").await.unwrap();
    let sock = Arc::new(sock);

    let mut sessions: Vec<Session> = vec![];
    let mut buf = [0; 1024];

    loop {
        let (amt, addr) = sock.recv_from(&mut buf).await.unwrap();
        if let Ok((session_id, msg)) = Message::from_buf(&buf[..amt]) {
            match msg {
                Message::Connect => {
                    sessions.push(Session {
                        peer_addr: addr,
                        id: session_id,
                        socket: sock.clone(),
                    });
                    sock.send_to(format!("/ack/{}/0/", session_id).as_bytes(), addr)
                        .await
                        .unwrap();
                }
                _ => todo!(),
            };
        } else {
            eprintln!("error parsing message");
        }
    }
}
