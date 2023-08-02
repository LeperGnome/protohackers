use tokio::{
    io::{AsyncReadExt},
    net::{TcpStream, TcpListener}
};

struct SrtMsg {
    len: u8,
    data: [u8],
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
    roads: [u16],
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
        _ => todo!(),
    };

}

async fn handle_dispatcher_conn(mut stream: TcpStream) {
    println!("Processing dispatcher connection...")
}

async fn handle_camera_conn(mut stream: TcpStream) {
    println!("Processing camera connection...")
}
