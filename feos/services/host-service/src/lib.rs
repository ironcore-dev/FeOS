use feos_proto::host_service::{
    FeosLogEntry, GetCpuInfoResponse, GetNetworkInfoResponse, GetVersionInfoResponse,
    HostnameResponse, KernelLogEntry, MemoryResponse, RebootRequest, RebootResponse,
    ShutdownRequest, ShutdownResponse, UpgradeFeosBinaryRequest, UpgradeFeosBinaryResponse,
};
use std::path::PathBuf;
use tokio::sync::{mpsc, oneshot};
use tonic::Status;

pub mod api;
pub mod dispatcher;
pub mod worker;

#[derive(Debug)]
pub enum Command {
    GetHostname(oneshot::Sender<Result<HostnameResponse, Status>>),
    GetMemory(oneshot::Sender<Result<MemoryResponse, Status>>),
    GetCPUInfo(oneshot::Sender<Result<GetCpuInfoResponse, Status>>),
    GetNetworkInfo(oneshot::Sender<Result<GetNetworkInfoResponse, Status>>),
    GetVersionInfo(oneshot::Sender<Result<GetVersionInfoResponse, Status>>),
    UpgradeFeosBinary(
        UpgradeFeosBinaryRequest,
        oneshot::Sender<Result<UpgradeFeosBinaryResponse, Status>>,
    ),
    StreamKernelLogs(mpsc::Sender<Result<KernelLogEntry, Status>>),
    StreamFeOSLogs(mpsc::Sender<Result<FeosLogEntry, Status>>),
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
