use tokio::{
    io::{split, AsyncBufReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf},
    net::{TcpListener, TcpSocket, TcpStream},
};

#[tokio::main]
async fn main() {
    println!("Server started!");
    let listner = TcpListener::bind("0.0.0.0:7878").await.unwrap();

    loop {
        let (stream, _) = listner.accept().await.unwrap();
        tokio::spawn(async move {
            process_stream(stream).await;
        });
    }
}

async fn process_stream(stream: TcpStream) {
    println!("New connection...");

    let out_stream = TcpSocket::new_v4()
        .unwrap()
        .connect("206.189.113.124:16963".parse().unwrap())
        .await
        .unwrap();

    println!("Connected to real chat, proxying...");

    let (in_read, in_write) = split(stream);
    let (out_read, out_write) = split(out_stream);

    let mut user_to_server = tokio::spawn(async move {
        proxy(in_read, out_write, "-->").await;
    });

    let mut server_to_user = tokio::spawn(async move {
        proxy(out_read, in_write, "<--").await;
    });

    tokio::select! {
        _ = &mut user_to_server => { server_to_user.abort(); }
        _ = &mut server_to_user => { user_to_server.abort(); }
    };
    println!("Disconnected");
}

async fn proxy(from: ReadHalf<TcpStream>, mut to: WriteHalf<TcpStream>, dbg_prefix: &str) {
    let mut from = BufReader::new(from);
    let mut buf = String::new();
    loop {
        match from.read_line(&mut buf).await {
            Ok(n) if n == 0 => {
                println!("Client disconnected via EOF");
                break;
            }
            Ok(n) => {
                println!("{dbg_prefix} {n} : '{buf}'");
                let modified_msg = rewrite_address(&buf);

                if let Err(_) = to.write(&modified_msg.as_bytes()).await {
                    break;
                }

                if let Err(_) = to.flush().await {
                    break;
                }
                buf.clear();
            }
            Err(e) => {
                eprintln!("Client disconnected abruptly! err: {:?}", e);
                break;
            }
        }
    }
}

fn rewrite_address(msg: &str) -> String {
    let addresses: Vec<String> = msg
        .split(' ')
        .filter(|w| {
            let x = w.trim();
            return x.starts_with("7")
                && x.len() >= 26
                && x.len() <= 35
                && x.chars().fold(true, |acc, c| acc && c.is_alphanumeric());
        })
        .map(|w| w.trim().to_string())
        .collect();
    let mut res = msg.to_string();
    for addr in addresses {
        res = res.replace(&addr, "7YWHMfk9JZe0LM0g1ZauHuiSxhI");
    }
    return res;
}
