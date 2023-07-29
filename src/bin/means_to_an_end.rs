use ::threadpool::ThreadPool;

use std::io::prelude::*;
use std::net::{TcpListener, TcpStream};

fn main() {
    let listner = TcpListener::bind("0.0.0.0:7878").unwrap();
    let pool = ThreadPool::new(5);
    for stream in listner.incoming() {
        pool.execute(|| handle_connection(stream.unwrap()));
    }
}

fn handle_connection(mut stream: TcpStream) {
    let mut prices: Vec<[i32; 2]> = vec![];
    loop {
        let mut message: [u8; 9] = [0; 9];
        match stream.read_exact(&mut message) {
            Ok(_) => (),
            Err(e) => println!("Failed reading message, got error: {:?}", e),
        };

        let message_type = message[0] as char;
        let num1 = i32::from_be_bytes(<[u8; 4]>::try_from(&message[1..5]).unwrap());
        let num2 = i32::from_be_bytes(<[u8; 4]>::try_from(&message[5..]).unwrap());

        match message_type {
            'I' => {
                match insert_data(&mut prices, num1, num2) {
                    Ok(_) => (),
                    Err(_) => break,
                };
            }
            'Q' => {
                let res = query_data(&prices, num1, num2);
                stream.write_all(&res.to_be_bytes()).unwrap();
            }
            _ => {
                println!("unknown command, closing connection");
                break;
            }
        }
        // println!("Current prices: {:?}", &prices);
    }
}

fn insert_data(prices: &mut Vec<[i32; 2]>, date: i32, ammount: i32) -> Result<(), ()> {
    if prices.iter().position(|&el| el[0] == date).is_some() {
        println!("Timestamp already exists, aborting");
        return Err(());
    };

    prices.push([date, ammount]);

    Ok(())
}

fn query_data(prices: &Vec<[i32; 2]>, date_from: i32, date_to: i32) -> i32 {
    if date_from > date_to {
        return 0;
    }

    let mut count = 0_i64;
    let sum: i64 = prices
        .iter()
        .filter(|x| x[0] >= date_from && x[0] <= date_to)
        .map(|x| {
            count += 1;
            return x[1] as i64;
        })
        .sum();

    if count == 0 {
        return 0;
    }

    return (sum / count) as i32;
}
