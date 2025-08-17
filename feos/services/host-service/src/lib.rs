use feos_proto::host_service::{
    HostnameResponse, KernelLogEntry, MemoryResponse, UpgradeRequest, UpgradeResponse,
};
use std::path::PathBuf;
use tokio::sync::{mpsc, oneshot};
use tonic::{Status, Streaming};

pub mod api;
pub mod dispatcher;
pub mod worker;

#[derive(Debug)]
pub enum Command {
    GetHostname(oneshot::Sender<Result<HostnameResponse, Status>>),
    GetMemory(oneshot::Sender<Result<MemoryResponse, Status>>),
    UpgradeFeosBinary(
        Box<Streaming<UpgradeRequest>>,
        oneshot::Sender<Result<UpgradeResponse, Status>>,
    ),
    StreamKernelLogs(mpsc::Sender<Result<KernelLogEntry, Status>>),
}

#[derive(Debug)]
pub struct RestartSignal(pub PathBuf);
