use std::{
    net::SocketAddr,
    sync::{Arc,Mutex}
};

use tokio::{
    io::{split, AsyncBufReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf},
    net::{TcpListener, TcpSocket, TcpStream},
};

#[tokio::main]
async fn main() {
    println!("Server started!");
    let listner = TcpListener::bind("0.0.0.0:7878").await.unwrap();
    let state: Arc<Mutex<Vec<SocketAddr>>> = Arc::new(Mutex::new(Vec::new()));

    loop {
        let state = state.clone();  // for debugging only
        let (stream, _) = listner.accept().await.unwrap();
        tokio::spawn(async move {
            process_stream(stream, state).await;
        });
    }
}

async fn process_stream(stream: TcpStream, state: Arc<Mutex<Vec<SocketAddr>>>) {
    println!("New connection...");
    let peer_addr = stream.peer_addr().unwrap();

    {
        let mut v = state.lock().unwrap();
        v.push(peer_addr);
        println!("Current state: {:?}", &v);
    }

    let out_stream = TcpSocket::new_v4()
        .unwrap()
        .connect("206.189.113.124:16963".parse().unwrap())
        .await
        .unwrap();

    println!("Connected to real chat, proxying...");

    let (in_read, in_write) = split(stream);
    let (out_read, out_write) = split(out_stream);

    let user_to_server = tokio::spawn(async move {
        proxy(in_read, out_write, "-->").await;
    });

    let server_to_user = tokio::spawn(async move {
        proxy(out_read, in_write, "<--").await;
    });

    tokio::select! {
        _ = user_to_server => {}
        _ = server_to_user => {}
    };

    {
        let mut v = state.lock().unwrap();
        let idx = v.iter().position(|x| x == &peer_addr).unwrap();
        v.remove(idx);
        println!("Disconnected");
        println!("Current state: {:?}", &v);
    }
}

async fn proxy(from: ReadHalf<TcpStream>, mut to: WriteHalf<TcpStream>, dbg_prefix: &str) {
    let mut from = BufReader::new(from);
    let mut buf = String::new();
    loop {
        if let Ok(read) = from.read_line(&mut buf).await {
            println!("%% {dbg_prefix} {read} : '{buf}'");
            if read == 0 {
                break;
            }
            buf = buf.trim().into();
            let modified_msg = rewrite_address(&buf);
            if let Ok(_) = to.write(&format!("{}\n", modified_msg).as_bytes()).await {
                if let Ok(_) = to.flush().await {
                    buf.clear();
                    continue;
                }
            }
        }
        break;
    }
}

fn rewrite_address(msg: &str) -> String {
    let addresses: Vec<String> = msg
        .split(' ')
        .filter(|x| {
            return x.starts_with("7")
                && x.len() >= 26
                && x.len() <= 35
                && x.chars().fold(true, |acc, c| acc && c.is_alphanumeric());
        })
        .map(|x| x.to_string())
        .collect();
    let mut res = msg.to_string();
    for addr in addresses {
        res = res.replace(&addr, "7YWHMfk9JZe0LM0g1ZauHuiSxhI");
    }
    return res;
}
