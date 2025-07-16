use crate::{vmm::Hypervisor, vmservice_helper, VmEventWrapper};
use feos_proto::vm_service::{
    AttachDiskRequest, AttachDiskResponse, CreateVmRequest, CreateVmResponse, DeleteVmRequest,
    DeleteVmResponse, GetVmRequest, PauseVmRequest, PauseVmResponse, PingVmRequest, PingVmResponse,
    RemoveDiskRequest, RemoveDiskResponse, ResumeVmRequest, ResumeVmResponse, ShutdownVmRequest,
    ShutdownVmResponse, StartVmRequest, StartVmResponse, StreamVmConsoleRequest,
    StreamVmConsoleResponse, StreamVmEventsRequest, VmEvent, VmInfo,
};
use log::{error, info};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, oneshot};
use tonic::{Status, Streaming};

pub async fn handle_create_vm(
    vm_id: String,
    req: CreateVmRequest,
    image_uuid: String,
    responder: oneshot::Sender<Result<CreateVmResponse, Status>>,
    hypervisor: Arc<dyn Hypervisor>,
    broadcast_tx: broadcast::Sender<VmEventWrapper>,
) {
    if responder
        .send(Ok(CreateVmResponse {
            vm_id: vm_id.clone(),
        }))
        .is_err()
    {
        error!("VM_WORKER ({vm_id}): Client disconnected before immediate response could be sent. Aborting creation.");
        return;
    }

    tokio::spawn(async move {
        info!("VM_WORKER ({vm_id}): Starting background creation process.");

        let result = hypervisor
            .create_vm(&vm_id, req, image_uuid, broadcast_tx.clone())
            .await;

        if let Err(e) = result {
            let error_msg = e.to_string();
            error!(
                "VM_WORKER ({}): Background creation process failed: {}",
                &vm_id, &error_msg
            );
        } else {
            info!("VM_WORKER ({vm_id}): Background creation process completed successfully.");
        }
    });
}

pub async fn handle_start_vm(
    req: StartVmRequest,
    responder: oneshot::Sender<Result<StartVmResponse, Status>>,
    hypervisor: Arc<dyn Hypervisor>,
    broadcast_tx: broadcast::Sender<VmEventWrapper>,
) {
    let result = hypervisor.start_vm(req, broadcast_tx).await;
    if responder.send(result.map_err(Into::into)).is_err() {
        error!("VM_WORKER: Failed to send response for StartVm.");
    }
}

pub async fn handle_get_vm(
    req: GetVmRequest,
    responder: oneshot::Sender<Result<VmInfo, Status>>,
    hypervisor: Arc<dyn Hypervisor>,
) {
    let result = hypervisor.get_vm(req).await;
    if responder.send(result.map_err(Into::into)).is_err() {
        error!("VM_WORKER: Failed to send response for GetVm.");
    }
}

pub async fn handle_stream_vm_events(
    req: StreamVmEventsRequest,
    stream_tx: mpsc::Sender<Result<VmEvent, Status>>,
    hypervisor: Arc<dyn Hypervisor>,
    broadcast_tx: broadcast::Sender<VmEventWrapper>,
) {
    let event_stream_result = hypervisor.stream_vm_events(req, broadcast_tx).await;

    let mut event_rx = match event_stream_result {
        Ok(rx) => rx,
        Err(e) => {
            let _ = stream_tx.send(Err(e.into())).await;
            return;
        }
    };

    while let Some(event_result) = event_rx.recv().await {
        let grpc_result = event_result.map_err(Into::into);
        if stream_tx.send(grpc_result).await.is_err() {
            info!("VM_WORKER (Stream): Client disconnected. Closing bridge.");
            break;
        }
    }
}

pub async fn handle_delete_vm(
    req: DeleteVmRequest,
    image_uuid: String,
    responder: oneshot::Sender<Result<DeleteVmResponse, Status>>,
    hypervisor: Arc<dyn Hypervisor>,
    broadcast_tx: broadcast::Sender<VmEventWrapper>,
) {
    let result = hypervisor.delete_vm(req, image_uuid, broadcast_tx).await;
    if responder.send(result.map_err(Into::into)).is_err() {
        error!("VM_WORKER: Failed to send response for DeleteVm.");
    }
}

pub async fn handle_stream_vm_console(
    mut input_stream: Streaming<StreamVmConsoleRequest>,
    output_tx: mpsc::Sender<Result<StreamVmConsoleResponse, Status>>,
    hypervisor: Arc<dyn Hypervisor>,
) {
    let vm_id = match vmservice_helper::get_attach_message(&mut input_stream).await {
        Ok(id) => id,
        Err(status) => {
            let _ = output_tx.send(Err(status)).await;
            return;
        }
    };

    let socket_path = match hypervisor.get_console_socket_path(&vm_id).await {
        Ok(path) => path,
        Err(e) => {
            let _ = output_tx.send(Err(e.into())).await;
            return;
        }
    };

    vmservice_helper::bridge_console_streams(socket_path, input_stream, output_tx).await;
}

pub async fn handle_ping_vm(
    req: PingVmRequest,
    responder: oneshot::Sender<Result<PingVmResponse, Status>>,
    hypervisor: Arc<dyn Hypervisor>,
) {
    let result = hypervisor.ping_vm(req).await;
    if responder.send(result.map_err(Into::into)).is_err() {
        error!("VM_WORKER: Failed to send response for PingVm.");
    }
}

pub async fn handle_shutdown_vm(
    req: ShutdownVmRequest,
    responder: oneshot::Sender<Result<ShutdownVmResponse, Status>>,
    hypervisor: Arc<dyn Hypervisor>,
    broadcast_tx: broadcast::Sender<VmEventWrapper>,
) {
    let result = hypervisor.shutdown_vm(req, broadcast_tx).await;
    if responder.send(result.map_err(Into::into)).is_err() {
        error!("VM_WORKER: Failed to send response for ShutdownVm.");
    }
}

pub async fn handle_pause_vm(
    req: PauseVmRequest,
    responder: oneshot::Sender<Result<PauseVmResponse, Status>>,
    hypervisor: Arc<dyn Hypervisor>,
    broadcast_tx: broadcast::Sender<VmEventWrapper>,
) {
    let result = hypervisor.pause_vm(req, broadcast_tx).await;
    if responder.send(result.map_err(Into::into)).is_err() {
        error!("VM_WORKER: Failed to send response for PauseVm.");
    }
}

pub async fn handle_resume_vm(
    req: ResumeVmRequest,
    responder: oneshot::Sender<Result<ResumeVmResponse, Status>>,
    hypervisor: Arc<dyn Hypervisor>,
    broadcast_tx: broadcast::Sender<VmEventWrapper>,
) {
    let result = hypervisor.resume_vm(req, broadcast_tx).await;
    if responder.send(result.map_err(Into::into)).is_err() {
        error!("VM_WORKER: Failed to send response for ResumeVm.");
    }
}

pub async fn handle_attach_disk(
    req: AttachDiskRequest,
    responder: oneshot::Sender<Result<AttachDiskResponse, Status>>,
    hypervisor: Arc<dyn Hypervisor>,
) {
    let result = hypervisor.attach_disk(req).await;
    if responder.send(result.map_err(Into::into)).is_err() {
        error!("VM_WORKER: Failed to send response for AttachDisk.");
    }
}

pub async fn handle_remove_disk(
    req: RemoveDiskRequest,
    responder: oneshot::Sender<Result<RemoveDiskResponse, Status>>,
    hypervisor: Arc<dyn Hypervisor>,
) {
    let result = hypervisor.remove_disk(req).await;
    if responder.send(result.map_err(Into::into)).is_err() {
        error!("VM_WORKER: Failed to send response for RemoveDisk.");
    }
}
