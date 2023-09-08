use tokio::{
    io::{split, AsyncBufReadExt, AsyncWriteExt, AsyncReadExt, BufReader},
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
            Ok(0) | Err(_) => return Err("client disconnected".into()),
            _ => (),
        };

        let parts = l.split(' ').collect::<Vec<&str>>();

        match parts.get(0) {
            Some(m) => match m.to_uppercase().as_str() {
                "GET" if parts.len() == 3 => Ok(Self::Get {
                    file: parts[1].to_string(),
                    version: parts[2].to_string(),
                }),
                "PUT" if parts.len() == 3 => {
                    let len = parts[2].parse::<usize>().unwrap_or(0);
                    let mut data_buff: Vec<u8> = vec![0; len];
                    r.read_exact(&mut data_buff).await;

                    Ok(Self::Put{
                        file: parts[1].to_string(),
                        len,
                        data: std::str::from_utf8(&data_buff).unwrap().to_string(),
                    })
                },
                "LIST" if parts.len() == 2 Ok()
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
