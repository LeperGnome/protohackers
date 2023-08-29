use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{
    io::{split, AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::{mpsc, oneshot},
};

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

struct Request {
    data: RequestData,
    cid: usize,
    response_tx: oneshot::Sender<Response>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum ResponseStatus {
    Ok,
    NoJob,
    Error,
}

struct JobCenter {
    request_rx: mpsc::Receiver<Request>,
}
impl JobCenter {
    async fn process_requests(&mut self) {
        todo!();
    }
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

    let (request_tx, request_rx) = mpsc::channel::<Request>(1024);

    tokio::spawn(async move {
        let mut job_centre = JobCenter { request_rx };
        job_centre.process_requests().await;
    });

    while let Ok((stream, _)) = listner.accept().await {
        let request_tx = request_tx.clone();
        tokio::spawn(async move {
            handle_connection(stream, cid, request_tx).await;
        });
        cid += 1;
    }
}

async fn handle_connection(stream: TcpStream, cid: usize, request_tx: mpsc::Sender<Request>) {
    let (reader, mut writer) = split(stream);
    let mut br = BufReader::new(reader);
    let mut line = String::new();

    loop {
        match br.read_line(&mut line).await {
            Ok(0) | Err(_) => {
                // TODO:
                // abort all jobs
                // close connection
                todo!();
            }
            Ok(_) => {
                if let Ok(request_data) = serde_json::from_str::<RequestData>(&line) {
                    let (response_tx, response_rx) = oneshot::channel::<Response>();
                    let request = Request {
                        data: request_data,
                        cid,
                        response_tx,
                    };
                    request_tx.send(request).await.unwrap(); // TODO
                    let response = response_rx.await.unwrap();
                    writer
                        .write_all(serde_json::to_string(&response).unwrap().as_bytes())
                        .await
                        .unwrap();
                    writer.flush().await.unwrap();
                } else {
                    // TODO:
                    // send error response
                    todo!();
                }
            }
        };
        line.clear();
    }
}
