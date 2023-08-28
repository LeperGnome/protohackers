use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::net::{TcpListener, TcpStream};

// NOTE: might not be super accurate, since job can't actually be a list
type JobData = Value;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[serde(tag = "request")]
enum RequestData {
    Get {
        queues: Vec<String>,
        wait: bool,
    },
    Put {
        queue: String,
        job: JobData,
        pri: usize,
    },
    Delete {
        id: usize,
    },
    Abort {
        id: usize,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum ResponseStatus {
    Ok,
    NoJob,
    Error,
}

// NOTE: not exactly correct, but good enough
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Response {
    status: ResponseStatus,
    id: Option<usize>,
    job: Option<JobData>,
    pri: Option<usize>,
    queue: Option<String>,
}

#[tokio::main]
async fn main() {
    let listner = TcpListener::bind("0.0.0.0:7878").await.unwrap();
    let mut cid = 0;

    while let Ok((stream, _)) = listner.accept().await {
        tokio::spawn(async move {
            handle_connection(stream, cid).await;
        });
        cid += 1;
    }
}

async fn handle_connection(stream: TcpStream, client_idx: usize) {}
