use crate::{worker, Command, RestartSignal};
use feos_utils::feos_logger::LogHandle;
use log::info;
use tokio::sync::mpsc;

pub struct HostServiceDispatcher {
    rx: mpsc::Receiver<Command>,
    restart_tx: mpsc::Sender<RestartSignal>,
    log_handle: LogHandle,
}

impl HostServiceDispatcher {
    pub fn new(
        rx: mpsc::Receiver<Command>,
        restart_tx: mpsc::Sender<RestartSignal>,
        log_handle: LogHandle,
    ) -> Self {
        Self {
            rx,
            restart_tx,
            log_handle,
        }
    }

    pub async fn run(mut self) {
        info!("HOST_DISPATCHER: Running and waiting for commands.");
        while let Some(cmd) = self.rx.recv().await {
            match cmd {
                Command::GetHostname(responder) => {
                    tokio::spawn(worker::handle_hostname(responder));
                }
                Command::GetMemory(responder) => {
                    tokio::spawn(worker::handle_get_memory(responder));
                }
                Command::GetCPUInfo(responder) => {
                    tokio::spawn(worker::handle_get_cpu_info(responder));
                }
                Command::GetNetworkInfo(responder) => {
                    tokio::spawn(worker::handle_get_network_info(responder));
                }
                Command::GetVersionInfo(responder) => {
                    tokio::spawn(worker::handle_get_version_info(responder));
                }
                Command::UpgradeFeosBinary(req, responder) => {
                    let restart_tx = self.restart_tx.clone();
                    tokio::spawn(worker::handle_upgrade(restart_tx, req, responder));
                }
                Command::StreamKernelLogs(stream_tx) => {
                    tokio::spawn(worker::handle_stream_kernel_logs(stream_tx));
                }
                Command::StreamFeOSLogs(stream_tx) => {
                    let log_handle = self.log_handle.clone();
                    tokio::spawn(worker::handle_stream_feos_logs(log_handle, stream_tx));
                }
                Command::Shutdown(req, responder) => {
                    tokio::spawn(worker::handle_shutdown(req, responder));
                }
                Command::Reboot(req, responder) => {
                    tokio::spawn(worker::handle_reboot(req, responder));
                }
            }
        }
        info!("HOST_DISPATCHER: Channel closed, shutting down.");
    }
}
