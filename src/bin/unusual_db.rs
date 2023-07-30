use std::collections::HashMap;
use std::net::UdpSocket;

fn main() {
    let info = "UnusualDP ver. 0.0.1";
    println!("{info}");

    let mut db: HashMap<String, String> = HashMap::from([("version".into(), info.into())]);

    loop {
        let socket = UdpSocket::bind("0.0.0.0:7878").unwrap();
        let mut buf = [0; 1000];
        let (amt, src) = socket.recv_from(&mut buf).unwrap();
        let buf = &mut buf[..amt];

        let msg = std::str::from_utf8(buf).unwrap();
        match msg.split_once("=") {
            // settings values
            Some((key, val)) if key != "version" => {
                println!("SET: {key} = {val}");
                db.insert(key.into(), val.into());
            }
            Some(_) => (),

            // gettings values
            None => {
                println!("GET: {msg}");
                if let Some((key, val)) = db.get_key_value(msg) {
                    socket
                        .send_to(format!("{key}={val}").as_bytes(), src)
                        .unwrap();
                }
            }
        }
    }
}
