use std::{
    net::TcpStream,
    io::{BufReader, BufWriter, prelude::*}
};


fn main() {
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
