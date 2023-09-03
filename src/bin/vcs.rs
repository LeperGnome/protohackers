use tokio::{
    io::{split, AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};

use std::io::prelude::*;

enum Command {
    Help,
    Get {
        file: String,
        version: String,
    },
    Put {
        file: String,
        len: usize,
        data: String,
    },
    List {
        dir: String,
    },
}
impl Command {
    async fn read<R: AsyncBufReadExt + Unpin>(r: &mut R) -> Result<Self, String> {
        let mut l = String::new();
        match r.read_line(&mut l).await {
            Ok(0) | Err(_) => return Err(()),
            _ => (),
        };

        let parts = l.split(' ').collect::<Vec<&str>>();

        match parts.get(0) {
            Some(m) => match *m {
                "GET" if parts.len() == 3 => Ok(Self::Get {
                    file: parts.get(1).unwrap().to_string(),
                    version: parts.get(2).unwrap().to_string(),
                }),
                _ => Err(format!("ERR illegal method: {m}")),
            },
            None => Err("ERR illegal method:".into()),
        }
    }
}

#[tokio::main]
async fn main() {
    let listner = TcpListener::bind("0.0.0.0:7878").await.unwrap();
    while let Ok((stream, _)) = listner.accept().await {
        tokio::spawn(async move {
            handle_stream(stream).await;
        });
    }
}

async fn handle_stream(stream: TcpStream) {
    let (mut r, mut w) = split(stream);
    w.write_all("READY\n".as_bytes()).await.unwrap();
    w.flush().await.unwrap();
}

fn experiment() {
    let stream = TcpStream::connect("vcs.protohackers.com:30307").unwrap();

    let mut writer = BufWriter::new(stream.try_clone().unwrap());
    let mut reader = BufReader::new(stream);

    read_line_and_print(&mut reader);

    send(&mut writer, "PUT /aello/more.txt 10\n123\n567\n90");
    read_line_and_print(&mut reader);
    read_line_and_print(&mut reader);

    send(&mut writer, "PUT /hello/more.txt 10\n123\n567\n90");
    read_line_and_print(&mut reader);
    read_line_and_print(&mut reader);

    send(&mut writer, "LIST /\n");
    read_line_and_print(&mut reader);
    read_line_and_print(&mut reader);
    read_line_and_print(&mut reader);
    read_line_and_print(&mut reader);

    send(&mut writer, "PUT /some.txt 1\n1");
    read_line_and_print(&mut reader);
    read_line_and_print(&mut reader);

    send(&mut writer, "GET /some.txt r1\n");
    read_line_and_print(&mut reader);
    read_line_and_print(&mut reader);

    send(&mut writer, "PUT /some.txt 3\n123");
    read_line_and_print(&mut reader);
    read_line_and_print(&mut reader);

    send(&mut writer, "GET /some.txt r2\n");
    read_line_and_print(&mut reader);
    read_line_and_print(&mut reader);

    send(&mut writer, "LIST /\n");
    read_line_and_print(&mut reader);
    read_line_and_print(&mut reader);
    read_line_and_print(&mut reader);
    read_line_and_print(&mut reader);
    read_line_and_print(&mut reader);
}

fn read_line_and_print(r: &mut BufReader<TcpStream>) {
    let mut buff = String::new();
    r.read_line(&mut buff).unwrap();
    print!("--> {buff}");
}

fn send(w: &mut BufWriter<TcpStream>, msg: &str) {
    println!("<-- {msg}");
    w.write_all(msg.as_bytes()).unwrap();
    w.flush().unwrap();
}
