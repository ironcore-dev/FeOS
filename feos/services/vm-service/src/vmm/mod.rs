// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use crate::VmEventWrapper;
use feos_proto::vm_service::{
    AttachDiskRequest, AttachDiskResponse, AttachNicRequest, AttachNicResponse, CreateVmRequest,
    DeleteVmRequest, DeleteVmResponse, GetVmRequest, PauseVmRequest, PauseVmResponse,
    PingVmRequest, PingVmResponse, RemoveDiskRequest, RemoveDiskResponse, DetachNicRequest,
    DetachNicResponse, ResumeVmRequest, ResumeVmResponse, ShutdownVmRequest, ShutdownVmResponse,
    StartVmRequest, StartVmResponse, VmEvent, VmInfo, VmStateChangedEvent,
};
use prost::Message;
use prost_types::Any;
use std::path::{Path, PathBuf};
use tokio::sync::{broadcast, mpsc};
use tonic::Status;
use uuid::Uuid;

pub mod ch_adapter;

#[derive(Debug, thiserror::Error)]
pub enum VmmError {
    #[error("Hypervisor process failed to start: {0}")]
    ProcessSpawnFailed(String),

    #[error("The provided configuration is invalid for this hypervisor: {0}")]
    InvalidConfig(String),

    #[error("Could not connect to the hypervisor's API socket: {0}")]
    ApiConnectionFailed(String),

    #[error("The hypervisor's API returned an error: {0}")]
    ApiOperationFailed(String),

    #[error("The requested VM (id: {0}) could not be found")]
    VmNotFound(String),

    #[error("The image service returned an error: {0}")]
    ImageServiceFailed(String),

    #[error("An internal or unexpected error occurred: {0}")]
    Internal(String),
}

impl From<VmmError> for Status {
    fn from(err: VmmError) -> Self {
        match err {
            VmmError::VmNotFound(id) => Status::not_found(id),
            VmmError::InvalidConfig(msg) => Status::invalid_argument(msg),
            VmmError::ApiConnectionFailed(msg) | VmmError::ImageServiceFailed(msg) => {
                Status::unavailable(msg)
            }
            VmmError::ProcessSpawnFailed(msg)
            | VmmError::ApiOperationFailed(msg)
            | VmmError::Internal(msg) => Status::internal(msg),
        }
    }
}

#[tonic::async_trait]
pub trait Hypervisor: Send + Sync {
    async fn create_vm(
        &self,
        vm_id: &str,
        req: CreateVmRequest,
        image_uuid: String,
    ) -> Result<Option<i64>, VmmError>;

    async fn start_vm(&self, req: StartVmRequest) -> Result<StartVmResponse, VmmError>;

    async fn healthcheck_vm(
        &self,
        vm_id: String,
        broadcast_tx: mpsc::Sender<VmEventWrapper>,
        cancel_bus: broadcast::Receiver<Uuid>,
    );

    async fn get_vm(&self, req: GetVmRequest) -> Result<VmInfo, VmmError>;

    async fn delete_vm(
        &self,
        req: DeleteVmRequest,
        process_id: Option<i64>,
    ) -> Result<DeleteVmResponse, VmmError>;

    async fn get_console_socket_path(&self, vm_id: &str) -> Result<PathBuf, VmmError>;

    async fn ping_vm(&self, req: PingVmRequest) -> Result<PingVmResponse, VmmError>;
    async fn shutdown_vm(&self, req: ShutdownVmRequest) -> Result<ShutdownVmResponse, VmmError>;
    async fn pause_vm(&self, req: PauseVmRequest) -> Result<PauseVmResponse, VmmError>;
    async fn resume_vm(&self, req: ResumeVmRequest) -> Result<ResumeVmResponse, VmmError>;
    async fn attach_disk(&self, req: AttachDiskRequest) -> Result<AttachDiskResponse, VmmError>;
    async fn remove_disk(&self, req: RemoveDiskRequest) -> Result<RemoveDiskResponse, VmmError>;
    async fn attach_nic(&self, req: AttachNicRequest) -> Result<AttachNicResponse, VmmError>;
    async fn detach_nic(&self, req: DetachNicRequest) -> Result<DetachNicResponse, VmmError>;
}

pub async fn broadcast_state_change_event(
    broadcast_tx: &mpsc::Sender<VmEventWrapper>,
    vm_id: &str,
    component: &str,
    data: VmStateChangedEvent,
    process_id: Option<i64>,
) {
    let event = VmEvent {
        vm_id: vm_id.to_string(),
        id: Uuid::new_v4().to_string(),
        component_id: component.to_string(),
        data: Some(Any {
            type_url: "type.googleapis.com/feos.vm.vmm.api.v1.VmStateChangedEvent".to_string(),
            value: data.encode_to_vec(),
        }),
    };

    if broadcast_tx
        .send(VmEventWrapper { event, process_id })
        .await
        .is_err()
    {
        log::warn!("Failed to broadcast event for VM '{vm_id}': channel closed.");
    }
}

pub enum VmmType {
    CloudHypervisor,
}

pub fn factory(vmm_type: VmmType) -> Box<dyn Hypervisor> {
    match vmm_type {
        VmmType::CloudHypervisor => {
            let ch_binary_path = Path::new(super::VM_CH_BIN).to_path_buf();
            Box::new(ch_adapter::CloudHypervisorAdapter::new(ch_binary_path))
        }
    }
}
