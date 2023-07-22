use std::{
    thread,
    sync::{mpsc, Arc, Mutex},
    thread::JoinHandle,
    io::{prelude::*, BufReader},
    net::{TcpListener, TcpStream},
};

type Job = Box<dyn FnOnce() + Send + 'static>;

struct ThreadPool {
    workers: Vec<Worker>,
    sender: mpsc::Sender<Job>,
}

impl ThreadPool {
    /// Create a new ThreadPool.
    ///
    /// The size is the number of threads in the pool.
    ///
    /// # Panics
    ///
    /// The `new` function will panic if the size is zero.
    fn new(size: usize) -> Self {
        assert!(size > 0);

        let (sender, receiver) = mpsc::channel();
        let receiver = Arc::new(Mutex::new(receiver));

        let mut workers = Vec::with_capacity(size);
        for id in 0..size {
            workers.push(Worker::new(id, Arc::clone(&receiver)));
        }

        Self { workers, sender }
    }

    fn execute<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static
    {
        let job = Box::new(f);
        self.sender.send(job).unwrap();
    }
}

struct Worker {
    id: usize,
    handle: JoinHandle<()>,
}

impl Worker {
    fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<Job>>>) -> Self {
        let handle = thread::spawn(move || loop {
            let job = receiver.lock().unwrap().recv().unwrap();
            println!("Worker {id} got a job, executing...");
            job();
        });
        Self { id, handle }
    }
}

fn main() {
    let listner = TcpListener::bind("0.0.0.0:7878").unwrap();
    let pool = ThreadPool::new(8);
    for stream in listner.incoming() {
        pool.execute(|| handle_connection(stream.unwrap()));
    }
}

fn handle_connection(mut stream: TcpStream) {
    let mut buf = [0; 1024];
    loop {
        let read = stream.read(&mut buf);
        println!("got {:?}", &buf);
        _ = match read {
            Ok(n) if n == 0 => break,
            Ok(n) => stream.write(&buf[0..n]).unwrap(),
            Err(e) => { println!("{:?}", e); return; }
        }
    }
}
