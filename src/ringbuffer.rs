use chrono::{DateTime, Utc};
use log::{Level, LevelFilter, Metadata, Record};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};

#[derive(Debug)]
pub struct RingBuffer {
    buffer: RwLock<VecDeque<String>>,
    capacity: usize,
}

impl RingBuffer {
    pub fn new(capacity: usize) -> Arc<Self> {
        Arc::new(Self {
            buffer: RwLock::new(VecDeque::with_capacity(capacity)),
            capacity,
        })
    }

    pub async fn push(&self, value: String) {
        let mut buffer = self.buffer.write().await;
        if buffer.len() == self.capacity {
            buffer.pop_front();
        }
        buffer.push_back(value);
    }

    pub async fn get_lines(&self) -> Vec<String> {
        let buffer = self.buffer.read().await;
        buffer.iter().cloned().collect()
    }
}

impl Default for RingBuffer {
    fn default() -> Self {
        RingBuffer {
            buffer: RwLock::new(VecDeque::with_capacity(10)),
            capacity: 10,
        }
    }
}

pub struct SimpleLogger {
    buffer: Arc<RingBuffer>,
    sender: mpsc::Sender<String>,
}

impl SimpleLogger {
    pub fn new(buffer: Arc<RingBuffer>, sender: mpsc::Sender<String>) -> Self {
        SimpleLogger { buffer, sender }
    }
}

impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let now: DateTime<Utc> = Utc::now();
            let log_message = format!(
                "{} {} [{}] - {}",
                now.to_rfc3339(),
                record.level(),
                record.target(),
                record.args()
            );
            println!("{}", log_message);
            let buffer = self.buffer.clone();
            let sender = self.sender.clone();
            tokio::spawn(async move {
                buffer.push(log_message.clone()).await;
                let _ = sender.send(log_message).await;
            });
        }
    }

    fn flush(&self) {}
}

pub fn init_logger(buffer: Arc<RingBuffer>) -> Arc<Mutex<mpsc::Receiver<String>>> {
    let (sender, receiver) = mpsc::channel(100);
    let logger = SimpleLogger::new(buffer, sender);
    log::set_boxed_logger(Box::new(logger)).unwrap();
    log::set_max_level(LevelFilter::Info);
    Arc::new(Mutex::new(receiver))
}
