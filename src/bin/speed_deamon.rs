use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc,
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

impl PlateMsg {
    async fn read(stream: &mut TcpStream) -> Self {
        let timestamp = stream.read_u32().await.unwrap();
        let plate = SrtMsg::read(stream).await;
        return Self { timestamp, plate };
    }
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
impl IAmCameraMsg {
    async fn read(stream: &mut TcpStream) -> Self {
        let road = stream.read_u16().await.unwrap();
        let mile = stream.read_u16().await.unwrap();
        let limit = stream.read_u16().await.unwrap();
        return Self { road, mile, limit };
    }
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

#[derive(Debug)]
struct Snapshot {}

struct SpeedDaemon {
    snapshots: Vec<Snapshot>,
    snapshots_rx: mpsc::Receiver<Snapshot>,
}

impl SpeedDaemon {
    fn new(rx: mpsc::Receiver<Snapshot>) -> Self {
        return Self {
            snapshots: vec![],
            snapshots_rx: rx,
        };
    }

    async fn process(&mut self) {
        while let Some(_) = self.snapshots_rx.recv().await {
            println!("Got snapshot!");
            // TODO organise data in some way and store
        }
    }
}

#[tokio::main]
async fn main() {
    let (tx, rx) = mpsc::channel::<Snapshot>(1024);
    tokio::spawn(async move {
        let mut daemon = SpeedDaemon::new(rx);
        daemon.process().await;
    });

    let listner = TcpListener::bind("0.0.0.0:7878").await.unwrap();
    loop {
        let tx = tx.clone();
        match listner.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(async move { handle_stream(stream, tx).await });
            }
            Err(e) => println!("Failed accepting connection: {:?}", e),
        };
    }
}

async fn handle_stream(mut stream: TcpStream, snapshot_tx: mpsc::Sender<Snapshot>) {
    match stream.read_u8().await.unwrap() {
        I_AM_DISPATCHER_CODE => handle_dispatcher_conn(stream).await,
        I_AM_CAMERA_CODE => handle_camera_conn(stream, snapshot_tx).await,
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

async fn handle_camera_conn(mut stream: TcpStream, snapshot_tx: mpsc::Sender<Snapshot>) {
    println!("Processing camera connection...");

    let camera_info = IAmCameraMsg::read(&mut stream).await;
    loop {
        match stream.read_u8().await.unwrap() {
            PLATE_CODE => {
                let plate_msg = PlateMsg::read(&mut stream).await;
                // TODO
                snapshot_tx.send(Snapshot {}).await.unwrap();
            }
            WANT_HEART_BEAT_CODE => (),
            _ => stream.write_all(&[ERROR_CODE, 0x00]).await.unwrap(),
        }
    }
}
