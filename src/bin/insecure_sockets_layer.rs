use ::threadpool::ThreadPool;

use std::io::prelude::*;
use std::io::BufReader;
use std::net::{TcpListener, TcpStream};

#[derive(Debug)]
enum Op {
    Rev,
    Xorn(u8),
    Xorpos,
    Addn(u8),
    Addpos,
}

const X_REVESEBITS: u8 = 0x01;
fn revesebits(bytes: &mut [u8]) {
    for b in bytes {
        *b = b.reverse_bits();
    }
}

const X_XORN: u8 = 0x02;
fn xorn(bytes: &mut [u8], n: u8) {
    for b in bytes {
        *b ^= n;
    }
}

const X_XORPOS: u8 = 0x03;
fn xorpos(bytes: &mut [u8], offset: usize) {
    for (i, b) in bytes.iter_mut().enumerate() {
        *b ^= (i + offset) as u8;
    }
}

const X_ADDN: u8 = 0x04;
fn addn(bytes: &mut [u8], n: u8) {
    for b in bytes {
        *b = b.wrapping_add(n);
    }
}

fn subn(bytes: &mut [u8], n: u8) {
    for b in bytes {
        *b = b.wrapping_sub(n);
    }
}

const X_ADDPOS: u8 = 0x05;
fn addpos(bytes: &mut [u8], offset: usize) {
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = b.wrapping_add((i + offset) as u8);
    }
}

fn subpos(bytes: &mut [u8], offset: usize) {
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = b.wrapping_sub((i + offset) as u8);
    }
}

fn main() {
    let pool = ThreadPool::new(10);
    let listner = TcpListener::bind("0.0.0.0:7878").unwrap();
    while let Ok((stream, _)) = listner.accept() {
        pool.execute(|| {
            handle_connection(stream);
        });
    }
}

fn handle_connection(mut stream: TcpStream) {
    println!("New connection!");
    let mut in_cnt: usize = 0;
    let mut out_cnt: usize = 0;

    let mut br = BufReader::new(stream.try_clone().unwrap());

    let mut spec: Vec<u8> = vec![];
    let amt = br.read_until(0x00, &mut spec).unwrap();
    let ops: Vec<Op> = parse_ops(&spec, amt);
    println!("Got ops: {:?}", ops);

    let mut data_line = String::new();
    let mut buf = vec![];
    loop {
        match stream.read(&mut buf) {
            Err(e) => {
                println!("Got error: {e}");
                break;
            }
            Ok(0) => {
                println!("Got EOF, {:?}", buf);
                break;
            }
            Ok(n) => {
                decode(&ops, &mut buf[..n], in_cnt);
                in_cnt += n;

                let msg = std::str::from_utf8(&buf).unwrap();
                println!("Got chunk: '{msg}'");

                if let Some((l, r)) = msg.split_once('\n') {
                    let out_msg = get_most_copies(&(data_line + l));
                    println!("Responding with: '{out_msg}'");

                    let mut resp = out_msg.as_bytes().to_owned();
                    encode(&ops, &mut resp, out_cnt);
                    out_cnt += resp.len();

                    println!("Encoded response: {:#04X?}", resp);
                    stream.write_all(&resp).unwrap();
                    stream.flush().unwrap();

                    println!("Wrote response");

                    data_line = r.into();
                } else {
                    data_line += msg;
                }
            }
        };
    }

    // for line in br.lines() {
    //     let mut data = line.unwrap().as_bytes().to_owned();
    //     println!("Got data: {:?}", data);
    //     let orig = data.clone();

    //     decode(&ops, &mut data, in_cnt);

    //     // stoppig, if cipher did non modify data
    //     println!("{:?}\n{:?}\n{}", orig, data, orig == data);
    //     if orig == data {
    //         break;
    //     }

    //     in_cnt += data.len() as u32;

    //     let msg = std::str::from_utf8(&data).unwrap();
    //     println!("Data after cipher: {msg}");

    //     let out_msg =  get_most_copies(msg);
    //     println!("Responding with: '{msg}'");

    //     let mut resp = out_msg.as_bytes().to_owned();
    //     encode(&ops, &mut resp, out_cnt);
    //     out_cnt += resp.len() as u32;

    //     println!("Encoded response: {:#04X?}", resp);
    //     stream.write_all(&resp).unwrap();
    //     stream.flush().unwrap();

    // }
    println!("Connection closed");
}

fn get_most_copies(msg: &str) -> String {
    let (res, _) = msg
        .split(',')
        .map(|s| (s, s.split_once('x').unwrap().0.parse::<u32>().unwrap()))
        .max_by_key(|(_, x)| *x)
        .unwrap();

    return res.into();
}

fn parse_ops(spec: &[u8], end: usize) -> Vec<Op> {
    let mut ops = vec![];
    let mut i = 0;
    while i <= end {
        let op = spec[i];
        match op {
            X_REVESEBITS => ops.push(Op::Rev),
            X_XORN => {
                i += 1;
                let n = spec[i];
                ops.push(Op::Xorn(n));
            }
            X_XORPOS => ops.push(Op::Xorpos),
            X_ADDN => {
                i += 1;
                let n = spec[i];
                ops.push(Op::Addn(n));
            }
            X_ADDPOS => ops.push(Op::Addpos),
            0x00 => break,
            _ => (),
        }
        i += 1;
    }
    return ops;
}

fn decode(ops: &Vec<Op>, data: &mut [u8], offset: usize) {
    for op in ops.iter().rev() {
        match *op {
            Op::Rev => revesebits(data),
            Op::Xorn(n) => xorn(data, n),
            Op::Xorpos => xorpos(data, offset),
            Op::Addn(n) => subn(data, n),
            Op::Addpos => subpos(data, offset),
        }
    }
}

fn encode(ops: &Vec<Op>, data: &mut [u8], offset: usize) {
    for op in ops.iter() {
        match *op {
            Op::Rev => revesebits(data),
            Op::Xorn(n) => xorn(data, n),
            Op::Xorpos => xorpos(data, offset),
            Op::Addn(n) => addn(data, n),
            Op::Addpos => addpos(data, offset),
        }
    }
}
