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
                num_is_prime = miller_rabin(num);
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


// NOTE: I took prime checking algorithm from "primal" rust library
// https://github.com/huonw/primal/blob/master/primal-check/src/is_prime.rs


fn mod_mul_(a: u64, b: u64, m: u64) -> u64 {
    (u128::from(a) * u128::from(b) % u128::from(m)) as u64
}

fn mod_mul(a: u64, b: u64, m: u64) -> u64 {
    match a.checked_mul(b) {
        Some(r) => if r >= m { r % m } else { r },
        None => mod_mul_(a, b, m),
    }
}

fn mod_sqr(a: u64, m: u64) -> u64 {
    if a < (1 << 32) {
        let r = a * a;
        if r >= m {
            r % m
        } else {
            r
        }
    } else {
        mod_mul_(a, a, m)
    }
}

fn mod_exp(mut x: u64, mut d: u64, n: u64) -> u64 {
    let mut ret: u64 = 1;
    while d != 0 {
        if d % 2 == 1 {
            ret = mod_mul(ret, x, n)
        }
        d /= 2;
        x = mod_sqr(x, n);
    }
    ret
}

pub fn miller_rabin(n: u64) -> bool {
    const HINT: &[u64] = &[2];

    // we have a strict upper bound, so we can just use the witness
    // table of Pomerance, Selfridge & Wagstaff and Jeaschke to be as
    // efficient as possible, without having to fall back to
    // randomness. Additional limits from Feitsma and Galway complete
    // the entire range of `u64`. See also:
    // https://en.wikipedia.org/wiki/Miller%E2%80%93Rabin_primality_test#Testing_against_small_sets_of_bases
    const WITNESSES: &[(u64, &[u64])] = &[
        (2_046, HINT),
        (1_373_652, &[2, 3]),
        (9_080_190, &[31, 73]),
        (25_326_000, &[2, 3, 5]),
        (4_759_123_140, &[2, 7, 61]),
        (1_112_004_669_632, &[2, 13, 23, 1662803]),
        (2_152_302_898_746, &[2, 3, 5, 7, 11]),
        (3_474_749_660_382, &[2, 3, 5, 7, 11, 13]),
        (341_550_071_728_320, &[2, 3, 5, 7, 11, 13, 17]),
        (3_825_123_056_546_413_050, &[2, 3, 5, 7, 11, 13, 17, 19, 23]),
        (std::u64::MAX, &[2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37]),
    ];

    if n % 2 == 0 { return n == 2 }
    if n == 1 { return false }

    let mut d = n - 1;
    let mut s = 0;
    while d % 2 == 0 { d /= 2; s += 1 }

    let witnesses =
        WITNESSES.iter().find(|&&(hi, _)| hi >= n)
            .map(|&(_, wtnss)| wtnss).unwrap();
    'next_witness: for &a in witnesses.iter() {
        let mut power = mod_exp(a, d, n);
        assert!(power < n);
        if power == 1 || power == n - 1 { continue 'next_witness }

        for _r in 0..s {
            power = mod_sqr(power, n);
            assert!(power < n);
            if power == 1 { return false }
            if power == n - 1 {
                continue 'next_witness
            }
        }
        return false
    }

    true
}
