// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use crate::error::VmServiceError;
use feos_proto::vm_service::{
    AttachDiskRequest, AttachDiskResponse, AttachNicRequest, AttachNicResponse, CreateVmRequest,
    CreateVmResponse, DeleteVmRequest, DeleteVmResponse, GetVmRequest, ListVmsRequest,
    ListVmsResponse, PauseVmRequest, PauseVmResponse, PingVmRequest, PingVmResponse,
    DetachDiskRequest, DetachDiskResponse, DetachNicRequest, DetachNicResponse, ResumeVmRequest,
    ResumeVmResponse, ShutdownVmRequest, ShutdownVmResponse, StartVmRequest, StartVmResponse,
    StreamVmConsoleRequest, StreamVmConsoleResponse, StreamVmEventsRequest, VmEvent, VmInfo,
};
use tokio::sync::{mpsc, oneshot};
use tonic::{Status, Streaming};

pub mod api;
pub mod dispatcher;
pub mod dispatcher_handlers;
pub mod error;
pub mod persistence;
pub mod vmm;
pub mod worker;

pub const DEFAULT_VM_DB_URL: &str = "sqlite:/var/lib/feos/vms.db";
pub const VM_API_SOCKET_DIR: &str = "/tmp/feos/vm_api_sockets";
pub const VM_CH_BIN: &str = "cloud-hypervisor";
pub const CONT_YOUKI_BIN: &str = "youki";
pub const IMAGE_DIR: &str = "/var/lib/feos/images";
pub const VM_CONSOLE_DIR: &str = "/tmp/feos/consoles";

#[derive(Debug, Clone)]
pub struct VmEventWrapper {
    pub event: VmEvent,
    pub process_id: Option<i64>,
}

pub enum Command {
    CreateVm(
        CreateVmRequest,
        oneshot::Sender<Result<CreateVmResponse, VmServiceError>>,
    ),
    StartVm(
        StartVmRequest,
        oneshot::Sender<Result<StartVmResponse, VmServiceError>>,
    ),
    GetVm(
        GetVmRequest,
        oneshot::Sender<Result<VmInfo, VmServiceError>>,
    ),
    StreamVmEvents(StreamVmEventsRequest, mpsc::Sender<Result<VmEvent, Status>>),
    DeleteVm(
        DeleteVmRequest,
        oneshot::Sender<Result<DeleteVmResponse, VmServiceError>>,
    ),
    StreamVmConsole(
        Box<Streaming<StreamVmConsoleRequest>>,
        mpsc::Sender<Result<StreamVmConsoleResponse, Status>>,
    ),
    ListVms(
        ListVmsRequest,
        oneshot::Sender<Result<ListVmsResponse, VmServiceError>>,
    ),
    PingVm(
        PingVmRequest,
        oneshot::Sender<Result<PingVmResponse, VmServiceError>>,
    ),
    ShutdownVm(
        ShutdownVmRequest,
        oneshot::Sender<Result<ShutdownVmResponse, VmServiceError>>,
    ),
    PauseVm(
        PauseVmRequest,
        oneshot::Sender<Result<PauseVmResponse, VmServiceError>>,
    ),
    ResumeVm(
        ResumeVmRequest,
        oneshot::Sender<Result<ResumeVmResponse, VmServiceError>>,
    ),
    AttachDisk(
        AttachDiskRequest,
        oneshot::Sender<Result<AttachDiskResponse, VmServiceError>>,
    ),
    DetachDisk(
        DetachDiskRequest,
        oneshot::Sender<Result<DetachDiskResponse, VmServiceError>>,
    ),
    AttachNic(
        AttachNicRequest,
        oneshot::Sender<Result<AttachNicResponse, VmServiceError>>,
    ),
    DetachNic(
        DetachNicRequest,
        oneshot::Sender<Result<DetachNicResponse, VmServiceError>>,
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
            Command::DetachDisk(req, _) => f.debug_tuple("DetachDisk").field(req).finish(),
            Command::AttachNic(req, _) => f.debug_tuple("AttachNic").field(req).finish(),
            Command::DetachNic(req, _) => f.debug_tuple("DetachNic").field(req).finish(),
        }
    }
}
