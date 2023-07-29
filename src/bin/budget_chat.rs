use std::sync::Arc;

use tokio::{
    io::{split, AsyncBufReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf},
    net::{TcpListener, TcpStream},
    spawn,
    sync::{mpsc, Mutex},
};

struct Chat {
    users: Vec<User>,
}

impl Chat {
    async fn add_user(&mut self, user: User) {
        let msg = format!(
            "* The room contains: {:?}\n",
            self.users
                .iter()
                .map(|u| u.name.clone())
                .collect::<Vec<String>>()
        );
        send_and_log(&user.tx, &msg).await;

        let msg = format!("* {} has entered the room\n", user.name);
        for u in self.users.iter() {
            send_and_log(&u.tx, &msg).await;
        }
        self.users.push(user);
    }
    async fn send_message(&self, from: &str, message: &str) {
        let msg = format!("[{}] {}", from, message);
        for u in self.users.iter() {
            if u.name == from {
                continue;
            }
            send_and_log(&u.tx, &msg).await;
        }
    }

    async fn remove_user(&mut self, name: &str) {
        if let Some(pos) = self.users.iter().position(|u| u.name == name) {
            self.users.remove(pos);
        }
        for u in self.users.iter() {
            let msg = format!("* {} has left the room\n", name);
            send_and_log(&u.tx, &msg).await;
        }
    }
}

struct User {
    name: String,
    tx: mpsc::Sender<String>,
}

type AChat = Arc<Mutex<Chat>>;

#[tokio::main]
async fn main() {
    let listner = TcpListener::bind("0.0.0.0:7878").await.unwrap();
    let chat = Arc::new(Mutex::new(Chat { users: Vec::new() }));

    loop {
        let (conn, _) = listner.accept().await.unwrap();
        let chat = chat.clone();
        spawn(async move {
            process_conn(conn, chat).await;
        });
    }
}

async fn process_conn(conn: TcpStream, chat: AChat) {
    let (read, mut writer) = split(conn);
    let mut buf_reader = BufReader::new(read);

    let name: String;
    match ask_name(&mut buf_reader, &mut writer).await {
        Ok(n) => name = n,
        Err(_) => return,
    };

    let (tx, mut rx) = mpsc::channel(32);
    chat.lock()
        .await
        .add_user(User {
            name: name.clone(),
            tx,
        })
        .await;

    // writes messages from other users and sys messages
    let incoming_task = spawn(async move {
        while let Some(msg) = rx.recv().await {
            writer.write_all(msg.as_bytes()).await.unwrap();
            writer.flush().await.unwrap();
        }
    });

    // reads messages from user and sends to chat
    let mut message = String::new();
    loop {
        match buf_reader.read_line(&mut message).await {
            Ok(0) | Err(_) => {
                chat.lock().await.remove_user(&name).await;
                incoming_task.abort();
                break;
            }
            Ok(_) => {
                chat.lock().await.send_message(&name, &message).await;
                message.clear();
            }
        }
    }
}

async fn ask_name(
    reader: &mut BufReader<ReadHalf<TcpStream>>,
    writer: &mut WriteHalf<TcpStream>,
) -> Result<String, ()> {
    writer
        .write_all("Please enter your name:\n".as_bytes())
        .await
        .unwrap();

    let mut name: String = "".into();
    match reader.read_line(&mut name).await {
        Ok(0) | Err(_) => return Err(()),
        Ok(_) => (),
    };
    name = name.trim().into();

    for c in name.chars() {
        if !c.is_alphanumeric() {
            return Err(());
        }
    }

    return Ok(name);
}

async fn send_and_log(tx: &mpsc::Sender<String>, msg: &str) {
    print!("{msg}");
    tx.send(msg.into()).await.unwrap();
}
