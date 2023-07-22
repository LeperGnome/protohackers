use ::threadpool::ThreadPool;

use std::{
    io::prelude::*,
    net::{TcpListener, TcpStream},
};

fn main() {
    let listner = TcpListener::bind("0.0.0.0:7878").unwrap();
    let pool = ThreadPool::new(8);
    for stream in listner.incoming() {
        pool.execute(|| handle_connection(stream.unwrap()));
    }
}

fn handle_connection(mut stream: TcpStream) {
    let mut buf = [0; 1024];
    loop {
        let read = stream.read(&mut buf);
        println!("got {:?}", &buf);
        _ = match read {
            Ok(n) if n == 0 => break,
            Ok(n) => stream.write(&buf[0..n]).unwrap(),
            Err(e) => { println!("{:?}", e); return; }
        }
    }
}
