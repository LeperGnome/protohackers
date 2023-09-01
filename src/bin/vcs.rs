use std::{
    net::TcpStream, 
    io::{BufReader, Write, BufRead, BufWriter}
};

fn main() {
    let stream = TcpStream::connect("vcs.protohackers.com:30307").unwrap();

    let mut writer = BufWriter::new(stream.try_clone().unwrap());
    let mut reader = BufReader::new(stream);

    read_line_and_print(&mut reader);
    send(&mut writer, "PUT SOME.TXT 3\n123\n");
    read_line_and_print(&mut reader);
    send(&mut writer, "GET some\n");
}

fn read_line_and_print(r: &mut BufReader<TcpStream>) {
    let mut buff = String::new();
    r.read_line(&mut buff).unwrap();
    print!("--> {buff}");
}

fn send(w: &mut BufWriter<TcpStream>, msg: &str) {
    print!("<-- {msg}");
    w.write_all(msg.as_bytes()).unwrap();
    w.flush().unwrap();
}
