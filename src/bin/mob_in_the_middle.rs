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
}

async fn proxy(from: ReadHalf<TcpStream>, mut to: WriteHalf<TcpStream>, dbg_prefix: &str) {
    let mut from = BufReader::new(from);
    let mut buf = String::new();
    loop {
        if let Ok(read) = from.read_line(&mut buf).await {
            if read == 0 {
                break;
            }
            println!("{dbg_prefix} {buf}");
            if let Ok(_) = to.write(&buf.as_bytes()).await {
                if let Ok(_) = to.flush().await {
                    {
                        continue;
                    }
                }
            }
        }
        break;
    }
}
