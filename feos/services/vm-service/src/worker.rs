// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    dispatcher_handlers::get_image_service_client, vmm::Hypervisor, vmm::VmmError, VmEventWrapper,
};
use feos_proto::{
    image_service::{ImageState as OciImageState, WatchImageStatusRequest},
    vm_service::{
        stream_vm_console_request as console_input, AttachConsoleMessage, AttachDiskRequest,
        AttachDiskResponse, ConsoleData, CreateVmRequest, CreateVmResponse, DeleteVmRequest,
        DeleteVmResponse, GetVmRequest, PauseVmRequest, PauseVmResponse, PingVmRequest,
        PingVmResponse, RemoveDiskRequest, RemoveDiskResponse, ResumeVmRequest, ResumeVmResponse,
        ShutdownVmRequest, ShutdownVmResponse, StartVmRequest, StartVmResponse,
        StreamVmConsoleRequest, StreamVmConsoleResponse, StreamVmEventsRequest, VmEvent, VmInfo,
        VmState, VmStateChangedEvent,
    },
};
use log::{error, info, warn};
use std::{path::PathBuf, sync::Arc};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    sync::{broadcast, mpsc, oneshot},
};
use tokio_stream::StreamExt;
use tonic::{Status, Streaming};
use uuid::Uuid;

async fn wait_for_image_ready(image_uuid: &str, image_ref: &str) -> Result<(), VmmError> {
    let mut client = get_image_service_client().await.map_err(|e| {
        VmmError::ImageServiceFailed(format!("Failed to connect to ImageService: {e}"))
    })?;

    let mut stream = client
        .watch_image_status(WatchImageStatusRequest {
            image_uuid: image_uuid.to_string(),
        })
        .await
        .map_err(|e| {
            VmmError::ImageServiceFailed(format!(
                "WatchImageStatus RPC failed for {image_uuid}: {e}"
            ))
        })?
        .into_inner();

    while let Some(status_res) = stream.next().await {
        let status = status_res.map_err(|e| {
            VmmError::ImageServiceFailed(format!("Image stream error for {image_uuid}: {e}"))
        })?;
        let state = OciImageState::try_from(status.state).unwrap_or(OciImageState::Unspecified);
        match state {
            OciImageState::Ready => return Ok(()),
            OciImageState::PullFailed => {
                return Err(VmmError::ImageServiceFailed(format!(
                    "Image pull failed for {image_ref} (uuid: {image_uuid}): {}",
                    status.message
                )))
            }
            _ => continue,
        }
    }
    Err(VmmError::ImageServiceFailed(format!(
        "Image watch stream for {image_uuid} ended before reaching a terminal state."
    )))
}

pub async fn handle_create_vm(
    vm_id: String,
    req: CreateVmRequest,
    image_uuid: String,
    responder: oneshot::Sender<Result<CreateVmResponse, Status>>,
    hypervisor: Arc<dyn Hypervisor>,
    broadcast_tx: mpsc::Sender<VmEventWrapper>,
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

    info!("VM_WORKER ({vm_id}): Starting creation process.");
    crate::vmm::broadcast_state_change_event(
        &broadcast_tx,
        &vm_id,
        "vm-service",
        VmStateChangedEvent {
            new_state: VmState::Creating as i32,
            reason: "VM creation process started".to_string(),
        },
        None,
    )
    .await;

    let image_ref = req
        .config
        .as_ref()
        .map(|c| c.image_ref.clone())
        .unwrap_or_default();

    info!(
        "VM_WORKER ({vm_id}): Waiting for image '{image_ref}' (uuid: {image_uuid}) to be ready..."
    );
    if let Err(e) = wait_for_image_ready(&image_uuid, &image_ref).await {
        let error_msg = e.to_string();
        error!("VM_WORKER ({vm_id}): {error_msg}");
        crate::vmm::broadcast_state_change_event(
            &broadcast_tx,
            &vm_id,
            "vm-service",
            VmStateChangedEvent {
                new_state: VmState::Crashed as i32,
                reason: error_msg,
            },
            None,
        )
        .await;
        return;
    }
    info!("VM_WORKER ({vm_id}): Image '{image_ref}' (uuid: {image_uuid}) is ready.");

    let result = hypervisor.create_vm(&vm_id, req, image_uuid).await;

    match result {
        Ok(pid) => {
            info!("VM_WORKER ({vm_id}): Background creation process completed successfully.");
            crate::vmm::broadcast_state_change_event(
                &broadcast_tx,
                &vm_id,
                "vm-service",
                VmStateChangedEvent {
                    new_state: VmState::Created as i32,
                    reason: "Hypervisor process started and VM configured".to_string(),
                },
                pid,
            )
            .await;
        }
        Err(e) => {
            let error_msg = e.to_string();
            error!("VM_WORKER ({vm_id}): Background creation process failed: {error_msg}");
            crate::vmm::broadcast_state_change_event(
                &broadcast_tx,
                &vm_id,
                "vm-service",
                VmStateChangedEvent {
                    new_state: VmState::Crashed as i32,
                    reason: error_msg,
                },
                None,
            )
            .await;
        }
    }
}

pub fn start_healthcheck_monitor(
    vm_id: String,
    hypervisor: Arc<dyn Hypervisor>,
    broadcast_tx: mpsc::Sender<VmEventWrapper>,
    cancel_bus: broadcast::Receiver<Uuid>,
) {
    let health_hypervisor = hypervisor;
    let health_broadcast_tx = broadcast_tx;
    tokio::spawn(async move {
        health_hypervisor
            .healthcheck_vm(vm_id, health_broadcast_tx, cancel_bus)
            .await;
    });
}

pub async fn handle_start_vm(
    req: StartVmRequest,
    responder: oneshot::Sender<Result<StartVmResponse, Status>>,
    hypervisor: Arc<dyn Hypervisor>,
    broadcast_tx: mpsc::Sender<VmEventWrapper>,
    cancel_bus: broadcast::Receiver<Uuid>,
) {
    let vm_id = req.vm_id.clone();
    let result = hypervisor.start_vm(req).await;

    if result.is_ok() {
        crate::vmm::broadcast_state_change_event(
            &broadcast_tx,
            &vm_id,
            "vm-service",
            VmStateChangedEvent {
                new_state: VmState::Running as i32,
                reason: "Start command successful".to_string(),
            },
            None,
        )
        .await;

        start_healthcheck_monitor(vm_id, hypervisor, broadcast_tx, cancel_bus);
    }

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
    broadcast_tx: broadcast::Sender<VmEventWrapper>,
) {
    let mut broadcast_rx = broadcast_tx.subscribe();
    let vm_id_to_watch = req.vm_id;

    let watcher_desc = vm_id_to_watch
        .clone()
        .unwrap_or_else(|| "all VMs".to_string());

    loop {
        match broadcast_rx.recv().await {
            Ok(VmEventWrapper { event, .. }) => {
                if vm_id_to_watch.as_ref().is_none_or(|id| event.vm_id == *id)
                    && stream_tx.send(Ok(event)).await.is_err()
                {
                    info!("VM_WORKER (Stream): Client for '{watcher_desc}' disconnected.");
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!(
                    "VM_WORKER (Stream): Event stream for '{watcher_desc}' lagged by {n} messages."
                );
            }
            Err(broadcast::error::RecvError::Closed) => {
                info!(
                    "VM_WORKER (Stream): Broadcast channel closed. Shutting down stream for '{watcher_desc}'."
                );
                break;
            }
        }
    }
}

pub async fn handle_delete_vm(
    req: DeleteVmRequest,
    image_uuid: String,
    process_id: Option<i64>,
    responder: oneshot::Sender<Result<DeleteVmResponse, Status>>,
    hypervisor: Arc<dyn Hypervisor>,
    _broadcast_tx: mpsc::Sender<VmEventWrapper>,
) {
    let vm_id = req.vm_id.clone();
    let result = hypervisor.delete_vm(req, process_id).await;

    if !image_uuid.is_empty() {
        info!("VM_WORKER ({vm_id}): Attempting to delete associated image with UUID: {image_uuid}");
        match get_image_service_client().await {
            Ok(mut client) => {
                let delete_req = feos_proto::image_service::DeleteImageRequest {
                    image_uuid: image_uuid.clone(),
                };
                if let Err(status) = client.delete_image(delete_req).await {
                    warn!(
                        "VM_WORKER ({vm_id}): Failed to delete image {image_uuid}: {message}. This may be expected if the image is shared or already deleted.",
                        message = status.message()
                    );
                } else {
                    info!("VM_WORKER ({vm_id}): Successfully requested deletion of image {image_uuid}");
                }
            }
            Err(e) => {
                warn!("VM_WORKER ({vm_id}): Could not connect to ImageService to delete image {image_uuid}: {e}");
            }
        }
    } else {
        info!("VM_WORKER ({vm_id}): No image UUID provided, skipping image deletion.");
    }

    if responder.send(result.map_err(Into::into)).is_err() {
        error!("VM_WORKER: Failed to send response for DeleteVm.");
    }
}

pub async fn handle_stream_vm_console(
    mut input_stream: Streaming<StreamVmConsoleRequest>,
    output_tx: mpsc::Sender<Result<StreamVmConsoleResponse, Status>>,
    hypervisor: Arc<dyn Hypervisor>,
) {
    let vm_id = match get_attach_message(&mut input_stream).await {
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

    bridge_console_streams(socket_path, input_stream, output_tx).await;
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
    broadcast_tx: mpsc::Sender<VmEventWrapper>,
) {
    let vm_id = req.vm_id.clone();
    let result = hypervisor.shutdown_vm(req).await;

    if result.is_ok() {
        crate::vmm::broadcast_state_change_event(
            &broadcast_tx,
            &vm_id,
            "vm-service",
            VmStateChangedEvent {
                new_state: VmState::Stopped as i32,
                reason: "Shutdown command successful".to_string(),
            },
            None,
        )
        .await;
    }

    if responder.send(result.map_err(Into::into)).is_err() {
        error!("VM_WORKER: Failed to send response for ShutdownVm.");
    }
}

pub async fn handle_pause_vm(
    req: PauseVmRequest,
    responder: oneshot::Sender<Result<PauseVmResponse, Status>>,
    hypervisor: Arc<dyn Hypervisor>,
    broadcast_tx: mpsc::Sender<VmEventWrapper>,
) {
    let vm_id = req.vm_id.clone();
    let result = hypervisor.pause_vm(req).await;

    if result.is_ok() {
        crate::vmm::broadcast_state_change_event(
            &broadcast_tx,
            &vm_id,
            "vm-service",
            VmStateChangedEvent {
                new_state: VmState::Paused as i32,
                reason: "Pause command successful".to_string(),
            },
            None,
        )
        .await;
    }

    if responder.send(result.map_err(Into::into)).is_err() {
        error!("VM_WORKER: Failed to send response for PauseVm.");
    }
}

pub async fn handle_resume_vm(
    req: ResumeVmRequest,
    responder: oneshot::Sender<Result<ResumeVmResponse, Status>>,
    hypervisor: Arc<dyn Hypervisor>,
    broadcast_tx: mpsc::Sender<VmEventWrapper>,
) {
    let vm_id = req.vm_id.clone();
    let result = hypervisor.resume_vm(req).await;

    if result.is_ok() {
        crate::vmm::broadcast_state_change_event(
            &broadcast_tx,
            &vm_id,
            "vm-service",
            VmStateChangedEvent {
                new_state: VmState::Running as i32,
                reason: "Resume command successful".to_string(),
            },
            None,
        )
        .await;
    }

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

async fn get_attach_message(
    stream: &mut Streaming<StreamVmConsoleRequest>,
) -> Result<String, Status> {
    match stream.next().await {
        Some(Ok(msg)) => match msg.payload {
            Some(console_input::Payload::Attach(AttachConsoleMessage { vm_id })) => Ok(vm_id),
            _ => Err(Status::invalid_argument(
                "First message must be an Attach message.",
            )),
        },
        Some(Err(e)) => Err(e),
        None => Err(Status::invalid_argument(
            "Client disconnected before sending Attach message.",
        )),
    }
}

async fn bridge_console_streams(
    socket_path: PathBuf,
    mut grpc_input: Streaming<StreamVmConsoleRequest>,
    grpc_output: mpsc::Sender<Result<StreamVmConsoleResponse, Status>>,
) {
    let vm_id = socket_path
        .file_stem()
        .unwrap()
        .to_str()
        .unwrap_or("unknown")
        .to_string();

    let socket = match UnixStream::connect(&socket_path).await {
        Ok(s) => s,
        Err(e) => {
            let err_msg = format!("Failed to connect to console socket at {socket_path:?}: {e}");
            let _ = grpc_output.send(Err(Status::unavailable(err_msg))).await;
            return;
        }
    };

    let (mut socket_reader, mut socket_writer) = tokio::io::split(socket);
    let grpc_output_clone = grpc_output.clone();
    let read_task_vm_id = vm_id.clone();

    let read_task = tokio::spawn(async move {
        let mut buf = vec![0; 4096];
        loop {
            tokio::select! {
                biased;
                _ = grpc_output_clone.closed() => {
                    info!("VMM_HELPER (Console {}): gRPC client disconnected, terminating read task.", &read_task_vm_id);
                    break;
                }
                read_result = socket_reader.read(&mut buf) => {
                    match read_result {
                        Ok(0) => {
                            info!("VMM_HELPER (Console {}): Console socket closed (EOF).", &read_task_vm_id);
                            break;
                        }
                        Ok(n) => {
                            let output_msg = StreamVmConsoleResponse { output: buf[..n].to_vec() };
                            if grpc_output_clone.send(Ok(output_msg)).await.is_err() {
                            }
                        }
                        Err(e) => {
                            let err_msg = format!("Error reading from console socket: {e}");
                            let _ = grpc_output_clone.send(Err(Status::internal(err_msg))).await;
                            break;
                        }
                    }
                }
            }
        }
    });

    let write_task_vm_id = vm_id.clone();
    let write_task = tokio::spawn(async move {
        while let Some(result) = grpc_input.next().await {
            match result {
                Ok(msg) => match msg.payload {
                    Some(console_input::Payload::Data(ConsoleData { input })) => {
                        if let Err(e) = socket_writer.write_all(&input).await {
                            warn!("VMM_HELPER (Console {}): Failed to write to socket: {}. VM may have shut down.", &write_task_vm_id, e);
                            break;
                        }
                    }
                    Some(console_input::Payload::Attach(_)) => {
                        let _ = grpc_output
                            .send(Err(Status::invalid_argument(
                                "Cannot send Attach message more than once.",
                            )))
                            .await;
                        break;
                    }
                    None => {
                        let _ = grpc_output
                            .send(Err(Status::invalid_argument("Empty ConsoleInput payload.")))
                            .await;
                        break;
                    }
                },
                Err(e) => {
                    warn!(
                        "VMM_HELPER (Console {}): Error reading from gRPC client stream: {}",
                        &write_task_vm_id, e
                    );
                    break;
                }
            }
        }
    });

    tokio::select! {
        _ = read_task => {},
        _ = write_task => {},
    }
}
