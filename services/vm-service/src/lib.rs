pub mod vmm;

use proto_definitions::vm_service::{
    AttachDiskRequest, AttachDiskResponse, CreateVmRequest, CreateVmResponse, DeleteVmRequest,
    DeleteVmResponse, GetVmRequest, ListVmsRequest, ListVmsResponse, PauseVmRequest,
    PauseVmResponse, PingVmRequest, PingVmResponse, RemoveDiskRequest, RemoveDiskResponse,
    ResumeVmRequest, ResumeVmResponse, ShutdownVmRequest, ShutdownVmResponse, StartVmRequest,
    StartVmResponse, StreamVmConsoleRequest, StreamVmConsoleResponse, StreamVmEventsRequest,
    VmEvent, VmInfo,
};
use tokio::sync::{mpsc, oneshot};
use tonic::{Status, Streaming};

pub mod persistence;
pub mod api;
pub mod dispatcher;
pub mod vmservice_helper;
pub mod worker;

pub const DEFAULT_VM_DB_URL: &str = "sqlite:/var/lib/feos/vms.db";
pub const VM_API_SOCKET_DIR: &str = "/tmp/vm_api_sockets";
pub const VM_CH_BIN: &str = "cloud-hypervisor";
pub const IMAGE_DIR: &str = "/tmp/images";
pub const VM_CONSOLE_DIR: &str = "/tmp/consoles";

#[derive(Debug, Clone)]
pub struct VmEventWrapper(pub VmEvent);

pub enum Command {
    CreateVm(
        CreateVmRequest,
        oneshot::Sender<Result<CreateVmResponse, Status>>,
    ),
    StartVm(
        StartVmRequest,
        oneshot::Sender<Result<StartVmResponse, Status>>,
    ),
    GetVm(GetVmRequest, oneshot::Sender<Result<VmInfo, Status>>),
    StreamVmEvents(
        StreamVmEventsRequest,
        mpsc::Sender<Result<VmEvent, Status>>,
    ),
    DeleteVm(
        DeleteVmRequest,
        oneshot::Sender<Result<DeleteVmResponse, Status>>,
    ),
    StreamVmConsole(
        Streaming<StreamVmConsoleRequest>,
        mpsc::Sender<Result<StreamVmConsoleResponse, Status>>,
    ),
    ListVms(
        ListVmsRequest,
        oneshot::Sender<Result<ListVmsResponse, Status>>,
    ),
    PingVm(
        PingVmRequest,
        oneshot::Sender<Result<PingVmResponse, Status>>,
    ),
    ShutdownVm(
        ShutdownVmRequest,
        oneshot::Sender<Result<ShutdownVmResponse, Status>>,
    ),
    PauseVm(
        PauseVmRequest,
        oneshot::Sender<Result<PauseVmResponse, Status>>,
    ),
    ResumeVm(
        ResumeVmRequest,
        oneshot::Sender<Result<ResumeVmResponse, Status>>,
    ),
    AttachDisk(
        AttachDiskRequest,
        oneshot::Sender<Result<AttachDiskResponse, Status>>,
    ),
    RemoveDisk(
        RemoveDiskRequest,
        oneshot::Sender<Result<RemoveDiskResponse, Status>>,
    ),
}

impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Command::CreateVm(req, _) => f.debug_tuple("CreateVm").field(req).finish(),
            Command::StartVm(req, _) => f.debug_tuple("StartVm").field(req).finish(),
            Command::GetVm(req, _) => f.debug_tuple("GetVm").field(req).finish(),
            Command::StreamVmEvents(req, _) => f.debug_tuple("StreamVmEvents").field(req).finish(),
            Command::DeleteVm(req, _) => f.debug_tuple("DeleteVm").field(req).finish(),
            Command::StreamVmConsole(_, _) => {
                f.write_str("StreamVmConsole(<gRPC Stream>, <mpsc::Sender>)")
            }
            Command::ListVms(req, _) => f.debug_tuple("ListVms").field(req).finish(),
            Command::PingVm(req, _) => f.debug_tuple("PingVm").field(req).finish(),
            Command::ShutdownVm(req, _) => f.debug_tuple("ShutdownVm").field(req).finish(),
            Command::PauseVm(req, _) => f.debug_tuple("PauseVm").field(req).finish(),
            Command::ResumeVm(req, _) => f.debug_tuple("ResumeVm").field(req).finish(),
            Command::AttachDisk(req, _) => f.debug_tuple("AttachDisk").field(req).finish(),
            Command::RemoveDisk(req, _) => f.debug_tuple("RemoveDisk").field(req).finish(),
        }
    }
}