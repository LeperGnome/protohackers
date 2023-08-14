use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::{
    io::{split, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpListener,
    sync::{mpsc, Mutex as AMutex},
};

const SEC_IN_DAYS: f64 = 86400.0;

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
impl TicketMsg {
    fn from_snapshots(s1: &Snapshot, s2: &Snapshot) -> Self {
        let road = s1.from.road;
        let plate = s2.plate.clone();
        let timestamp1 = s1.plate.timestamp;
        let timestamp2 = s2.plate.timestamp;
        let mile1 = s1.from.mile;
        let mile2 = s2.from.mile;
        let speed = (((mile2.abs_diff(mile1)) as f32) * 10000_f32)
            / (((timestamp2 - timestamp1) as f32) / 36_f32);
        Self {
            road,
            mile1,
            mile2,
            timestamp1,
            timestamp2,
            speed: speed as u16,
            plate: plate.plate,
        }
    }
    async fn drain<W: AsyncWrite + Unpin + Send>(&self, w: Arc<AMutex<W>>) {
        let mut wl = w.lock().await;
        wl.write_u8(TICKET_CODE).await.unwrap();
        wl.write_u8(self.plate.len).await.unwrap();
        wl.write_all(&self.plate.content).await.unwrap();
        wl.write_u16(self.road).await.unwrap();
        wl.write_u16(self.mile1).await.unwrap();
        wl.write_u32(self.timestamp1).await.unwrap();
        wl.write_u16(self.mile2).await.unwrap();
        wl.write_u32(self.timestamp2).await.unwrap();
        wl.write_u16(self.speed).await.unwrap();
    }
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
        return Self { roads };
    }
}

#[derive(Debug, Clone)]
struct Snapshot {
    from: IAmCameraMsg,
    plate: PlateMsg,
}

#[derive(Debug)]
struct State {
    snapshots: Vec<Snapshot>,
    issued_tickets: Vec<(Vec<u8>, u32)>,
    idle_tickets: Vec<(TicketMsg, bool)>, // bool marks weather ticket was sent
    dispatchrs: HashMap<u16, Vec<mpsc::Sender<TicketMsg>>>,
}
impl State {
    fn new() -> Self {
        Self {
            snapshots: vec![],
            issued_tickets: vec![],
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
        let mut st = state.lock().await;
        let cur_idx: usize;
        if let Some(idx) = st.snapshots.iter().position(|x| {
            x.plate.timestamp > new_snap.plate.timestamp
                && x.plate.plate.content == new_snap.plate.plate.content
                && x.from.road == x.from.road
        }) {
            cur_idx = idx.saturating_sub(1);
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
                    && *i != cur_idx
            })
            .for_each(|(_, x)| {
                let (s1, s2): (&Snapshot, &Snapshot);
                if new_snap.plate.timestamp > x.plate.timestamp {
                    (s1, s2) = (x, &new_snap);
                } else {
                    (s2, s1) = (x, &new_snap);
                }

                let ticket = TicketMsg::from_snapshots(s1, s2);
                if ticket.speed > x.from.limit.saturating_mul(100) {
                    tickets.push(ticket.clone());
                }
            });

        for ticket in tickets {
            let from_day = ((ticket.timestamp1 as f64) / SEC_IN_DAYS).floor() as u32;
            let to_day = ((ticket.timestamp2 as f64) / SEC_IN_DAYS).floor() as u32;
            if (from_day..=to_day).any(|d| {
                st.issued_tickets
                    .contains(&(ticket.plate.content.clone(), d))
            }) {
                continue;
            } else {
                for day in from_day..=to_day {
                    st.issued_tickets.push((ticket.plate.content.clone(), day));
                }
            }
            if let Some(ds) = st.dispatchrs.get(&ticket.road) {
                let mut broken = vec![];
                'dis: for (i, d) in ds.iter().enumerate() {
                    if let Ok(_) = d.send(ticket.clone()).await {
                        break 'dis;
                    } else {
                        broken.push(i);
                    }
                }
            } else {
                st.idle_tickets.push((ticket.clone(), false));
            }
        }
    }
}

async fn process_dispatchers(
    mut rx: mpsc::Receiver<DispatcherRegistration>,
    state: Arc<AMutex<State>>,
) {
    while let Some(d) = rx.recv().await {
        println!("<dis> New dispatcher: {:?}", d.info);
        let mut st = state.lock().await;

        for road in d.info.roads.iter() {
            st.dispatchrs
                .entry(*road)
                .or_insert(Vec::new())
                .push(d.tx.clone());
        }

        for t in st.idle_tickets.iter_mut() {
            if !t.1 && d.info.roads.contains(&t.0.road) {
                d.tx.send(t.0.clone()).await.unwrap();
                t.1 = true;
            }
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
                tokio::spawn(async move {
                    let (mut r, w) = split(stream);
                    handle_stream(&mut r, Arc::new(AMutex::new(w)), snap_tx, disp_tx).await
                });
            }
            Err(e) => println!("Failed accepting connection: {:?}", e),
        };
    }
}

async fn handle_stream<R, W>(
    r: &mut R,
    w: Arc<AMutex<W>>,
    snapshot_tx: mpsc::Sender<Snapshot>,
    dispatcher_tx: mpsc::Sender<DispatcherRegistration>,
) where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send + 'static,
{
    match r.read_u8().await {
        Ok(b) if b == WANT_HEART_BEAT_CODE => {
            let hb = WantHeartbeatMsg::read(r).await;
            spawn_heartbeat(w.clone(), hb.interval as f32);
            match r.read_u8().await {
                Ok(b) if b == I_AM_DISPATCHER_CODE => {
                    handle_dispatcher_conn(r, w, dispatcher_tx).await
                }
                Ok(b) if b == I_AM_CAMERA_CODE => handle_camera_conn(r, w, snapshot_tx).await,
                Err(_) => return,
                _ => w.lock().await.write_all(&[ERROR_CODE, 0x00]).await.unwrap(),
            }
        }
        Ok(b) if b == I_AM_DISPATCHER_CODE => handle_dispatcher_conn(r, w, dispatcher_tx).await,
        Ok(b) if b == I_AM_CAMERA_CODE => handle_camera_conn(r, w, snapshot_tx).await,
        Err(_) => return,
        _ => w.lock().await.write_all(&[ERROR_CODE, 0x00]).await.unwrap(),
    };
}

fn spawn_heartbeat<W>(w: Arc<AMutex<W>>, interval: f32)
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        do_heartbeat(w, interval).await;
        println!("-- Client disconnected, stopping heartbeat");
    });
}

async fn do_heartbeat<W>(w: Arc<AMutex<W>>, interval: f32)
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    if interval <= 0_f32 {
        return;
    }
    while let Ok(_) = w.lock().await.write_all(&[HEART_BEAT_CODE]).await {
        println!("-- Sent heartbeat");
        tokio::time::sleep(Duration::from_secs_f32(interval / 10_f32)).await;
    }
}

async fn handle_dispatcher_conn<R, W>(
    r: &mut R,
    w: Arc<AMutex<W>>,
    dispatcher_tx: mpsc::Sender<DispatcherRegistration>,
) where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send + 'static,
{
    println!("<dis> Processing dispatcher connection...");

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

    let w2 = w.clone();
    let ticket_listner = tokio::spawn(async move {
        // gettings tickets to send to client
        while let Some(t) = ticket_rx.recv().await {
            t.drain(w2.clone()).await;
        }
    });

    match r.read_u8().await {
        Ok(b) if b == WANT_HEART_BEAT_CODE => {
            let hb = WantHeartbeatMsg::read(r).await;
            do_heartbeat(w, hb.interval as f32).await;
        }
        Err(_) => {
            println!("<dis> err reading from dispatcher");
            ticket_listner.abort();
        }
        Ok(b) => {
            println!("<dis> unknown command {:?}", b);
            w.lock().await.write_all(&[ERROR_CODE, 0x00]).await.unwrap();
            ticket_listner.abort();
        }
    }
}

async fn handle_camera_conn<R, W>(r: &mut R, w: Arc<AMutex<W>>, snapshot_tx: mpsc::Sender<Snapshot>)
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send + 'static,
{
    println!("[cam] Processing camera connection...");

    let camera_info = IAmCameraMsg::read(r).await;
    println!("[cam] Got camera: {:?}", &camera_info);
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
            Ok(b) if b == WANT_HEART_BEAT_CODE => {
                let hb = WantHeartbeatMsg::read(r).await;
                spawn_heartbeat(w.clone(), hb.interval as f32);
            }
            Err(_) => {
                break;
            }
            b @ _ => {
                println!("[cam] unknown command {:?}", b);
                w.lock().await.write_all(&[ERROR_CODE, 0x00]).await.unwrap();
                break;
            }
        }
    }
    println!("[cam] Closing camera connection");
}
