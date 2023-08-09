use std::collections::HashMap;
use std::sync::Arc;
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
            content.push(0);
        }
        stream.read_exact(&mut content).await.unwrap();
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
    plate: SrtMsg,
    timestamp: u32,
}
impl PlateMsg {
    async fn read<R: AsyncRead + Unpin + Send>(stream: &mut R) -> Self {
        let plate = SrtMsg::read(stream).await;
        let timestamp = stream.read_u32().await.unwrap();
        return Self { timestamp, plate };
    }
}

const TICKET_CODE: u8 = 0x21;
#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
struct Snapshot {
    from: IAmCameraMsg,
    plate: PlateMsg,
}

// {
//     "NSDF": {
//          123 : [
//              (t, m), ...
//          ]
//     }
// }

struct State {
    snapshots: Vec<Snapshot>,
    idle_tickets: Vec<TicketMsg>,
    dispatchrs: HashMap<u16, Vec<mpsc::Sender<TicketMsg>>>,
}
impl State {
    fn new() -> Self {
        Self {
            snapshots: vec![],
            idle_tickets: vec![],
            dispatchrs: HashMap::new(),
        }
    }
}

#[derive(Debug)]
struct DispatcherRegistration {
    info: IAmDispatcherMsg,
    tx: mpsc::Sender<TicketMsg>,
}

async fn process_snapshots(mut rx: mpsc::Receiver<Snapshot>, state: Arc<AMutex<State>>) {
    while let Some(new_snap) = rx.recv().await {
        println!("Got snapshot: {:?}", new_snap);
        let mut st = state.lock().await;
        let cur_idx: usize;
        if let Some(idx) = st
            .snapshots
            .iter()
            .position(|x| x.plate.timestamp > new_snap.plate.timestamp)
        {
            cur_idx = idx.saturating_sub(1);
            println!("Found at index {idx}, Try insert at {cur_idx}");
            st.snapshots.insert(cur_idx, new_snap.clone());
        } else {
            st.snapshots.push(new_snap.clone());
            cur_idx = st.snapshots.len() - 1;
        }

        let mut tickets = vec![];

        st.snapshots
            .iter()
            .enumerate()
            .filter(|(i, x)| {
                x.plate.plate.content == new_snap.plate.plate.content
                    && x.from.road == new_snap.from.road
                    && cur_idx.abs_diff(*i) == 1
            })
            .for_each(|(_, x)| {
                let road = x.from.road;
                let plate = new_snap.plate.clone();
                let timestamp1 = new_snap.plate.timestamp.min(x.plate.timestamp);
                let timestamp2 = new_snap.plate.timestamp.max(x.plate.timestamp);
                let mile1 = new_snap.from.mile.min(x.from.mile);
                let mile2 = new_snap.from.mile.max(x.from.mile);
                let speed = (((mile2 - mile1) as f32) * 10000_f32) / (((timestamp2 - timestamp1) as f32) / 36_f32); // TODO rounding
                let ticket = TicketMsg {
                        road,
                        mile1,
                        mile2,
                        timestamp1,
                        timestamp2,
                        speed: speed as u16,
                        plate: plate.plate,
                    };
                println!("possible ticket: {:?}", &ticket);

                if ticket.speed > x.from.limit.saturating_mul(100) {
                    tickets.push(ticket);
                }
            });

        for ticket in tickets {
            if let Some(ds) = st.dispatchrs.get(&ticket.road) {
                let mut broken = vec![];
                for (i, d) in ds.iter().enumerate() {
                    println!("Try sending ticket...");
                    if let Err(_) = d.send(ticket.clone()).await {
                        eprintln!("Failed sending ticket...");
                        broken.push(i);
                    } else {
                        break;
                    }
                }
            } else {
                st.idle_tickets.push(ticket);
            }
        }
    }
}

async fn process_dispatchers(
    mut rx: mpsc::Receiver<DispatcherRegistration>,
    state: Arc<AMutex<State>>,
) {
    while let Some(d) = rx.recv().await {
        println!("New dispatcher: {:?}", d.info);
        for road in d.info.roads {
            state
                .lock()
                .await
                .dispatchrs
                .entry(road)
                .or_insert(Vec::new())
                .push(d.tx.clone());
        }
    }
}

#[tokio::main]
async fn main() {
    let state = Arc::new(AMutex::new(State::new()));
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
    match r.read_u8().await {
        Ok(b) if b == WANT_HEART_BEAT_CODE => {
            let hb = WantHeartbeatMsg::read(&mut r).await;
            spawn_heartbeat(wa.clone(), hb.interval as f32);
            match r.read_u8().await {
                Ok(b) if b == I_AM_DISPATCHER_CODE => {
                    handle_dispatcher_conn(&mut r, wa, dispatcher_tx).await
                }
                Ok(b) if b == I_AM_CAMERA_CODE => handle_camera_conn(&mut r, wa, snapshot_tx).await,
                Err(_) => return,
                _ => wa
                    .lock()
                    .await
                    .write_all(&[ERROR_CODE, 0x00])
                    .await
                    .unwrap(),
            }
        }
        Ok(b) if b == I_AM_DISPATCHER_CODE => {
            handle_dispatcher_conn(&mut r, wa, dispatcher_tx).await
        }
        Ok(b) if b == I_AM_CAMERA_CODE => handle_camera_conn(&mut r, wa, snapshot_tx).await,
        Err(_) => return,
        _ => wa
            .lock()
            .await
            .write_all(&[ERROR_CODE, 0x00])
            .await
            .unwrap(),
    };
}

fn spawn_heartbeat<W>(w: Arc<AMutex<W>>, interval: f32)
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    if interval > 0_f32 {
        tokio::spawn(async move {
            while let Ok(_) = w.lock().await.write_all(&[HEART_BEAT_CODE]).await {
                println!("Sent heartbeat");
                tokio::time::sleep(Duration::from_secs_f32(interval / 10_f32)).await;
            }
            println!("Client disconnected, stopping heartbeat");
        });
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

    // gettings tickets to send to client
    while let Some(t) = ticket_rx.recv().await {
        println!("Got ticket: {:?}", t);
        let mut wl = w.lock().await;
        wl.write_u8(TICKET_CODE).await.unwrap();
        wl.write_u8(t.plate.len).await.unwrap();
        wl.write_all(&t.plate.content).await.unwrap();
        wl.write_u16(t.road).await.unwrap();
        wl.write_u16(t.mile1).await.unwrap();
        wl.write_u32(t.timestamp1).await.unwrap();
        wl.write_u16(t.mile2).await.unwrap();
        wl.write_u32(t.timestamp2).await.unwrap();
        wl.write_u16(t.speed).await.unwrap();
    }
    // TODO: handle heartbeat at any point?
    // TODO: handle disconnect
}

async fn handle_camera_conn<R, W>(r: &mut R, w: Arc<AMutex<W>>, snapshot_tx: mpsc::Sender<Snapshot>)
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    println!("Processing camera connection...");

    let camera_info = IAmCameraMsg::read(r).await;
    println!("Got camera: {:?}", &camera_info);
    loop {
        match r.read_u8().await {
            Ok(b) if b == PLATE_CODE => {
                let plate_msg = PlateMsg::read(r).await;
                snapshot_tx
                    .send(Snapshot {
                        plate: plate_msg,
                        from: camera_info.clone(),
                    })
                    .await
                    .unwrap();
            }
            // TODO: handle heartbeat at any point?
            Ok(b) if b == WANT_HEART_BEAT_CODE => (),
            Err(e) => {
                eprintln!("got error from camera: {e}");
                break;
            }
            _ => w.lock().await.write_all(&[ERROR_CODE, 0x00]).await.unwrap(),
        }
    }
    println!("Closing camera connection");
}
