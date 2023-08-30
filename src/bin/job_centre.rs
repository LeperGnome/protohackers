use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{
    io::{split, AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::{mpsc, oneshot},
};

use std::collections::HashMap;

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
    fn with_id(self, id: usize) -> Self {
        return Self {
            status: self.status,
            id: Some(id),
            job: self.job,
            pri: self.pri,
            queue: self.queue,
        };
    }
}

struct JobInfo {
    id: usize,
    data: JobData,
    pri: usize,
    locked_by: Option<usize>,
    queue: String,
}

// NOTE: pessimized, but seems ok, considering expected load
struct AwaitingClient {
    cid: usize,
    queues: Vec<String>,
    response_tx: oneshot::Sender<Response>,
}

struct JobCenter {
    next_id: usize,
    jobs: HashMap<String, Vec<JobInfo>>,
    awaiting_clients: Vec<AwaitingClient>,
    request_rx: mpsc::Receiver<Request>,
}
impl JobCenter {
    fn new(request_rx: mpsc::Receiver<Request>) -> Self {
        return Self {
            next_id: 0,
            jobs: HashMap::new(),
            awaiting_clients: Vec::new(),
            request_rx,
        };
    }

    async fn process_requests(&mut self) {
        while let Some(request) = self.request_rx.recv().await {
            println!("JobCenter got request: {:?}", request);
            self._process_request(request).await;
        }
    }

    async fn _process_request(&mut self, request: Request) {
        match request.data {
            RequestData::Get { queues, wait } => match self._get(&queues, wait, request.cid) {
                Some(r) => request.response_tx.send(r).unwrap(),
                None => self.awaiting_clients.push(AwaitingClient {
                    cid: request.cid,
                    queues,
                    response_tx: request.response_tx,
                }),
            },
            RequestData::Put { queue, job, pri } => {
                let response = self._put(queue, job, pri);
                request.response_tx.send(response).unwrap();
                self.next_id += 1;
            }
            RequestData::Abort { id } => {
                todo!()
            }
            RequestData::Delete { id } => {
                todo!()
            }
        };
    }

    fn _put(&mut self, queue: String, job: JobData, pri: usize) -> Response {
        let mut locked_by = None;

        // check in awaiting_clients first
        if let Some(i) = self
            .awaiting_clients
            .iter()
            .position(|cli| cli.queues.contains(&queue))
        {
            let awaiting_client = self.awaiting_clients.remove(i);
            let response = Response {
                status: ResponseStatus::Ok,
                id: Some(self.next_id),
                job: Some(job.clone()),
                pri: Some(pri),
                queue: Some(queue.clone()),
            };
            awaiting_client.response_tx.send(response).unwrap();
            locked_by = Some(awaiting_client.cid);
        }

        let new_job_info = JobInfo {
            id: self.next_id,
            data: job,
            pri,
            queue,
            locked_by,
        };

        let jobs = self.jobs.entry(new_job_info.queue.clone()).or_default();

        let insertion_idx = jobs
            .binary_search_by_key(&new_job_info.pri, |j| j.pri)
            .unwrap_or_else(|e| e);

        jobs.insert(insertion_idx, new_job_info);
        return Response::new_with_status(ResponseStatus::Ok).with_id(self.next_id);
    }

    fn _get(&mut self, queues: &Vec<String>, wait: bool, cid: usize) -> Option<Response> {
        let best_index_in_queue = self._get_best_index_queue(queues);
        let response: Option<Response>;
        if let Some((i, q)) = best_index_in_queue {
            // job found -> responding and setting lock
            let best_job = self.jobs.get_mut(&q).unwrap().get_mut(i).unwrap(); // Hacky, but I just
                                                                               // found it above
            response = Some(Response {
                status: ResponseStatus::Ok,
                id: Some(best_job.id),
                job: Some(best_job.data.clone()),
                pri: Some(best_job.pri),
                queue: Some(best_job.queue.clone()),
            });
            best_job.locked_by = Some(cid);
        } else {
            // no job found
            if !wait {
                response = Some(Response::new_with_status(ResponseStatus::NoJob));
            } else {
                response = None;
            }
        }
        return response;
    }

    fn _get_best_index_queue(&self, queues: &Vec<String>) -> Option<(usize, String)> {
        // NOTE: using this method to find location of a best job.
        // All of that is used because I could not figure out how to use `get_many_mut()` on
        // HashMap properly

        let mut best_index_in_queue: Option<(usize, String)> = None;
        let mut best_pri = 0_usize;

        for q_name in queues {
            if let Some(q) = self.jobs.get(q_name) {
                // find best job in each queue
                if let Some((i, j)) = q
                    .iter()
                    .filter(|j| matches!(j.locked_by, None))
                    .enumerate()
                    .max_by_key(|(_, j)| j.pri)
                {
                    // find overall best
                    if j.pri > best_pri {
                        best_index_in_queue = Some((i, q_name.clone()));
                        best_pri = j.pri;
                    }
                }
            }
        }
        return best_index_in_queue;
    }
}

#[tokio::main]
async fn main() {
    let listner = TcpListener::bind("0.0.0.0:7878").await.unwrap();
    let mut cid = 0;

    let (request_tx, request_rx) = mpsc::channel::<Request>(1024);

    tokio::spawn(async move {
        let mut job_centre = JobCenter::new(request_rx);
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
                    let request = Request {
                        data: request_data,
                        cid,
                        response_tx,
                    };
                    request_tx.send(request).await.unwrap();
                    let response = response_rx.await.unwrap();

                    if response.job.is_some() {
                        // NOTE: hacky, but should work
                        acquired_ids.push(response.id.unwrap());
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
