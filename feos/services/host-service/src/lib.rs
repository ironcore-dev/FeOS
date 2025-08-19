use feos_proto::host_service::{
    GetCpuInfoResponse, GetNetworkInfoResponse, HostnameResponse, KernelLogEntry, MemoryResponse,
    RebootRequest, RebootResponse, ShutdownRequest, ShutdownResponse, UpgradeRequest,
    UpgradeResponse,
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
    GetCPUInfo(oneshot::Sender<Result<GetCpuInfoResponse, Status>>),
    GetNetworkInfo(oneshot::Sender<Result<GetNetworkInfoResponse, Status>>),
    UpgradeFeosBinary(
        Box<Streaming<UpgradeRequest>>,
        oneshot::Sender<Result<UpgradeResponse, Status>>,
    ),
    StreamKernelLogs(mpsc::Sender<Result<KernelLogEntry, Status>>),
    Shutdown(
        ShutdownRequest,
        oneshot::Sender<Result<ShutdownResponse, Status>>,
    ),
    Reboot(
        RebootRequest,
        oneshot::Sender<Result<RebootResponse, Status>>,
    ),
}

#[derive(Debug)]
pub struct RestartSignal(pub PathBuf);
