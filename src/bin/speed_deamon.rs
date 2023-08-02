use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpStream, TcpListener}
};

struct SrtMsg {
    len: u8,
    content: Vec<u8>,
}

impl SrtMsg {
    async fn read(stream: &mut TcpStream) -> Self {
        let len = stream.read_u8().await.unwrap();
        let mut content: Vec<u8> = Vec::with_capacity(len as usize);
        for _ in 0..len {
            let el = stream.read_u8().await.unwrap();
            content.push(el);
        }
        return Self { len, content };
    }
}

const ERROR_CODE: u8 = 0x10;
struct ServerErrorMsg {
    msg: SrtMsg,
}

const PLATE_CODE: u8 = 0x20;
struct PlateMsg {
    timestamp: u32,
    plate: SrtMsg,
}

const TICKET_CODE: u8 = 0x21;
struct TicketMsg {
    road: u16,
    mile1: u16,
    timestamp1: u32,
    mile2: u16,
    timestamp2: u32,
    speed: u16, // 100x mph
    plate: SrtMsg,
}

const WANT_HEART_BEAT_CODE: u8 = 0x40;
struct WantHeartbeatMsg {
    interval: u32,
}

const HEART_BEAT_CODE: u8 = 0x41;
struct HeartbeatMsg {}

const I_AM_CAMERA_CODE: u8 = 0x80;
struct IAmCameraMsg {
    road: u16,
    mile: u16,
    limit: u16,
}

const I_AM_DISPATCHER_CODE: u8 = 0x81;
struct IAmDispatcherMsg {
    numroads: u8,
    roads: Vec<u16>,
}

impl IAmDispatcherMsg {
    async fn read(stream: &mut TcpStream) -> Self {
        let numroads = stream.read_u8().await.unwrap();
        let mut roads: Vec<u16> = Vec::with_capacity(numroads as usize);
        for _ in 0..numroads {
            let road = stream.read_u16().await.unwrap();
            roads.push(road);
        }
        return Self { numroads, roads };
    }
}

#[tokio::main]
async fn main() {
    let listner = TcpListener::bind("0.0.0.0:7878").await.unwrap();
    loop {
        match listner.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(async move {
                    handle_stream(stream).await
                });
            }
            Err(e) => println!("Failed accepting connection: {:?}", e)
        };
    }
}


async fn handle_stream(mut stream: TcpStream) {
    match stream.read_u8().await.unwrap() {
        I_AM_DISPATCHER_CODE => handle_dispatcher_conn(stream).await,
        I_AM_CAMERA_CODE => handle_camera_conn(stream).await,
        _ => stream.write_all(&[ERROR_CODE, 0x00]).await.unwrap(),
    };

}

async fn handle_dispatcher_conn(mut stream: TcpStream) {
    println!("Processing dispatcher connection...");
    let dispatcher_data = IAmDispatcherMsg::read(&mut stream).await;
    // TODO:
    // save dispatcher data to shared state
    // receive and write tickets
}

async fn handle_camera_conn(mut stream: TcpStream) {
    println!("Processing camera connection...")
    // TODO:
    // send tickets to shared state
}
