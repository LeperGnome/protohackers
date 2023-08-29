use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{
    io::{split, AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::{mpsc, oneshot},
};

// NOTE: might not be super accurate, since job can't actually be a list
type JobData = Value;

#[derive(Serialize, Deserialize, Debug)]
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

#[derive(Debug)]
struct Request {
    data: RequestData,
    cid: usize,
    response_tx: oneshot::Sender<Response>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
enum ResponseStatus {
    Ok,
    NoJob,
    Error,
}

// NOTE: not exactly correct, but good enough
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
struct Response {
    status: ResponseStatus,
    id: Option<usize>,
    job: Option<JobData>,
    pri: Option<usize>,
    queue: Option<String>,
}
impl Response {
    fn new_with_status(status: ResponseStatus) -> Self {
        return Self {
            status,
            id: None,
            job: None,
            pri: None,
            queue: None,
        };
    }
}

struct JobCenter {
    request_rx: mpsc::Receiver<Request>,
}
impl JobCenter {
    async fn process_requests(&mut self) {
        while let Some(request) = self.request_rx.recv().await {
            println!("JobCenter got request: {:?}", request);
            match request.data {
                RequestData::Get { queues, wait } => {
                    todo!()
                }
                RequestData::Put { queue, job, pri } => {
                    todo!()
                }
                RequestData::Abort { id } => {
                    todo!()
                }
                RequestData::Delete { id } => {
                    todo!()
                }
            };
        }
    }
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
    let mut acquired_ids: Vec<usize> = vec![];

    loop {
        match br.read_line(&mut line).await {
            Ok(0) | Err(_) => {
                // EOF or some error: should abort all acquired jobs and close the connection
                for id in acquired_ids {
                    let (response_tx, _) = oneshot::channel::<Response>();
                    request_tx
                        .send(Request {
                            data: RequestData::Abort { id },
                            cid,
                            response_tx,
                        })
                        .await
                        .unwrap();
                }
                break;
            }
            Ok(_) => {
                if let Ok(request_data) = serde_json::from_str::<RequestData>(&line) {
                    let (response_tx, response_rx) = oneshot::channel::<Response>();
                    let is_get = matches!(request_data, RequestData::Get { .. });
                    let request = Request {
                        data: request_data,
                        cid,
                        response_tx,
                    };
                    request_tx.send(request).await.unwrap();
                    let response = response_rx.await.unwrap();
                    if is_get {
                        acquired_ids.push(response.id.unwrap()); // NOTE: hacky, but should work
                    }
                    send_json(response, &mut writer).await;
                } else {
                    // Client sent invalid message, responding with error, keeping connection
                    send_json(
                        Response::new_with_status(ResponseStatus::Error),
                        &mut writer,
                    )
                    .await;
                }
            }
        };
        line.clear();
    }
}

async fn send_json<T, W>(data: T, writer: &mut W)
where
    T: Serialize,
    W: AsyncWriteExt + Send + Unpin + 'static,
{
    writer
        .write_all(serde_json::to_string(&data).unwrap().as_bytes())
        .await
        .unwrap();
    writer.flush().await.unwrap();
}
