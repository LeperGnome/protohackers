use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::{
    io::{split, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{mpsc, Mutex as AMutex},
};

#[derive(Debug, Clone)]
struct SrtMsg {
    len: u8,
    content: Vec<u8>,
}

impl SrtMsg {
    async fn read<R: AsyncRead + Unpin + Send>(stream: &mut R) -> Self {
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
#[derive(Debug, Clone)]
struct PlateMsg {
    timestamp: u32,
    plate: SrtMsg,
}

impl PlateMsg {
    async fn read<R: AsyncRead + Unpin + Send>(stream: &mut R) -> Self {
        let timestamp = stream.read_u32().await.unwrap();
        let plate = SrtMsg::read(stream).await;
        return Self { timestamp, plate };
    }
}

const TICKET_CODE: u8 = 0x21;
#[derive(Debug)]
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
impl WantHeartbeatMsg {
    async fn read<R: AsyncRead + Unpin + Send>(stream: &mut R) -> Self {
        let interval = stream.read_u32().await.unwrap();
        return Self { interval };
    }
}

const HEART_BEAT_CODE: u8 = 0x41;
struct HeartbeatMsg {}

const I_AM_CAMERA_CODE: u8 = 0x80;
#[derive(Debug, Clone, Copy)]
struct IAmCameraMsg {
    road: u16,
    mile: u16,
    limit: u16,
}
impl IAmCameraMsg {
    async fn read<R: AsyncRead + Unpin + Send>(stream: &mut R) -> Self {
        let road = stream.read_u16().await.unwrap();
        let mile = stream.read_u16().await.unwrap();
        let limit = stream.read_u16().await.unwrap();
        return Self { road, mile, limit };
    }
}

const I_AM_DISPATCHER_CODE: u8 = 0x81;
#[derive(Debug)]
struct IAmDispatcherMsg {
    numroads: u8,
    roads: Vec<u16>,
}

impl IAmDispatcherMsg {
    async fn read<R: AsyncRead + Unpin + Send>(stream: &mut R) -> Self {
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
struct Snapshot {
    from: IAmCameraMsg,
    plate: PlateMsg,
}

struct State {
    snapshots: Vec<Snapshot>,
    dispatchrs: HashMap<u16, Vec<mpsc::Sender<TicketMsg>>>,
}
impl State {
    fn new() -> Self {
        Self {
            snapshots: vec![],
            dispatchrs: HashMap::new(),
        }
    }
}

#[derive(Debug)]
struct DispatcherRegistration {
    info: IAmDispatcherMsg,
    tx: mpsc::Sender<TicketMsg>,
}

async fn process_snapshots(mut rx: mpsc::Receiver<Snapshot>, state: Arc<Mutex<State>>) {
    while let Some(s) = rx.recv().await {
        println!("Got snapshot: {:?}", s);
        // TODO organise data in some way and store
    }
}

async fn process_dispatchers(
    mut rx: mpsc::Receiver<DispatcherRegistration>,
    state: Arc<Mutex<State>>,
) {
    while let Some(d) = rx.recv().await {
        println!("New dispatcher: {:?}", d.info);
        // TODO organise data in some way and store
    }
}

#[tokio::main]
async fn main() {
    let state = Arc::new(Mutex::new(State::new()));
    let state2 = state.clone();
    let (snap_tx, snap_rx) = mpsc::channel::<Snapshot>(1024);
    let (disp_tx, disp_rx) = mpsc::channel::<DispatcherRegistration>(256);

    tokio::spawn(async move {
        process_snapshots(snap_rx, state).await;
    });

    tokio::spawn(async move {
        process_dispatchers(disp_rx, state2).await;
    });

    let listner = TcpListener::bind("0.0.0.0:7878").await.unwrap();
    loop {
        let snap_tx = snap_tx.clone();
        let disp_tx = disp_tx.clone();
        match listner.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(async move { handle_stream(stream, snap_tx, disp_tx).await });
            }
            Err(e) => println!("Failed accepting connection: {:?}", e),
        };
    }
}

async fn handle_stream(
    stream: TcpStream,
    snapshot_tx: mpsc::Sender<Snapshot>,
    dispatcher_tx: mpsc::Sender<DispatcherRegistration>,
) {
    let (mut r, w) = split(stream);
    let wa = Arc::new(AMutex::new(w));
    match r.read_u8().await.unwrap() {
        WANT_HEART_BEAT_CODE => {
            let heartbeat = WantHeartbeatMsg::read(&mut r).await;
            let wa2 = wa.clone();
            tokio::spawn(async move {
                spawn_heartbeat(wa2, heartbeat.interval as f32).await;
            });
            match r.read_u8().await.unwrap() {
                I_AM_DISPATCHER_CODE => handle_dispatcher_conn(&mut r, wa, dispatcher_tx).await,
                I_AM_CAMERA_CODE => handle_camera_conn(&mut r, wa, snapshot_tx).await,
                _ => wa.lock().await.write_all(&[ERROR_CODE, 0x00]).await.unwrap(),
            }
        }
        I_AM_DISPATCHER_CODE => handle_dispatcher_conn(&mut r, wa, dispatcher_tx).await,
        I_AM_CAMERA_CODE => handle_camera_conn(&mut r, wa, snapshot_tx).await,
        _ => wa.lock().await.write_all(&[ERROR_CODE, 0x00]).await.unwrap(),
    };
}

async fn spawn_heartbeat<W>(w: Arc<AMutex<W>>, interval: f32)
where
    W: AsyncWrite + Unpin + Send,
{
    while let Ok(_) = w.lock().await.write_all(&[HEART_BEAT_CODE]).await {
        tokio::time::sleep(Duration::from_secs_f32(interval / 10_f32));
    }
}

async fn handle_dispatcher_conn<R, W>(
    r: &mut R,
    w: Arc<AMutex<W>>,
    dispatcher_tx: mpsc::Sender<DispatcherRegistration>,
) where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    println!("Processing dispatcher connection...");

    let dispatcher_data = IAmDispatcherMsg::read(r).await;
    let (ticket_tx, mut ticket_rx) = mpsc::channel::<TicketMsg>(16);

    // registration
    dispatcher_tx
        .send(DispatcherRegistration {
            info: dispatcher_data,
            tx: ticket_tx,
        })
        .await
        .unwrap();

    // gettings tickets ready to send to client
    while let Some(t) = ticket_rx.recv().await {
        println!("Got ticket: {:?}", t);
    }

    // TODO: handle heartbeat at any point?
}

async fn handle_camera_conn<R, W>(r: &mut R, w: Arc<AMutex<W>>, snapshot_tx: mpsc::Sender<Snapshot>)
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    println!("Processing camera connection...");

    let camera_info = IAmCameraMsg::read(r).await;
    loop {
        match r.read_u8().await.unwrap() {
            PLATE_CODE => {
                let plate_msg = PlateMsg::read(r).await;
                // TODO
                snapshot_tx
                    .send(Snapshot {
                        plate: plate_msg,
                        from: camera_info.clone(),
                    })
                    .await
                    .unwrap();
            }
            // TODO: handle heartbeat at any point?
            WANT_HEART_BEAT_CODE => (),
            _ => w.lock().await.write_all(&[ERROR_CODE, 0x00]).await.unwrap(),
        }
    }
}
