use crate::{worker, Command, RestartSignal};
use log::info;
use tokio::sync::mpsc;

pub struct HostServiceDispatcher {
    rx: mpsc::Receiver<Command>,
    restart_tx: mpsc::Sender<RestartSignal>,
}

impl HostServiceDispatcher {
    pub fn new(rx: mpsc::Receiver<Command>, restart_tx: mpsc::Sender<RestartSignal>) -> Self {
        Self { rx, restart_tx }
    }

    pub async fn run(mut self) {
        info!("HOST_DISPATCHER: Running and waiting for commands.");
        while let Some(cmd) = self.rx.recv().await {
            match cmd {
                Command::GetHostname(responder) => {
                    tokio::spawn(worker::handle_hostname(responder));
                }
                Command::UpgradeFeosBinary(stream, responder) => {
                    let restart_tx = self.restart_tx.clone();
                    tokio::spawn(worker::handle_upgrade(restart_tx, *stream, responder));
                }
                Command::StreamKernelLogs(stream_tx) => {
                    tokio::spawn(worker::handle_stream_kernel_logs(stream_tx));
                }
            }
        }
        info!("HOST_DISPATCHER: Channel closed, shutting down.");
    }
}
