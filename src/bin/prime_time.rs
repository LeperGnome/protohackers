use ::threadpool::ThreadPool;
use serde::{Serialize, Deserialize};

use std::net::{TcpListener, TcpStream};
use std::io::{prelude::*, BufReader, BufWriter};


#[derive(Serialize, Deserialize, Debug)]
enum Method {
    isPrime,
}

#[derive(Serialize, Deserialize, Debug)]
struct Request {
    method: Method,
    number: serde_json::Number,
}


#[derive(Serialize, Deserialize, Debug)]
struct Response {
    method: Method,
    prime: bool,
}

fn main() {
    let listner = TcpListener::bind("0.0.0.0:7878").unwrap();
    let pool = ThreadPool::new(8);
    for stream in listner.incoming() {
        pool.execute(|| handle_connection(stream.unwrap()));
    }
}

fn handle_connection(stream: TcpStream) {
    let reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = BufWriter::new(stream);
    for line in reader.lines() {
        if let Ok(request) = serde_json::from_str::<Request>(&line.unwrap()) {
            println!("{:?}", request);

            let num_is_prime: bool;
            if let Some(num) = request.number.as_u64() {
                num_is_prime = is_prime(num);
            } else { 
                num_is_prime = false;
            }

            let response = Response{method: Method::isPrime, prime: num_is_prime};

            writer.write_all(format!("{}\n",serde_json::to_string(&response).unwrap()).as_bytes()).unwrap();
            writer.flush().unwrap();
        } else { 
            println!("Request is not valid");
            writer.write_all(b"{}").unwrap();
            break;
        }
    }
    println!("Closing stream...");
}

fn is_prime(n: u64) -> bool {
    if n <= 1 {
        return false;
    }
    for a in 2..n {
        if n % a == 0 {
            return false;
        }
    }
    true
}
