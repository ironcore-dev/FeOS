// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    error::VmServiceError,
    persistence::{repository::VmRepository, VmRecord, VmStatus},
    vmm::Hypervisor,
    worker, VmEventWrapper,
};
use feos_proto::{
    image_service::{image_service_client::ImageServiceClient, PullImageRequest},
    vm_service::{
        AttachDiskRequest, AttachDiskResponse, CreateVmRequest, CreateVmResponse, DeleteVmRequest,
        DeleteVmResponse, GetVmRequest, ListVmsRequest, ListVmsResponse, PauseVmRequest,
        PauseVmResponse, RemoveDiskRequest, RemoveDiskResponse, ResumeVmRequest, ResumeVmResponse,
        ShutdownVmRequest, ShutdownVmResponse, StartVmRequest, StartVmResponse,
        StreamVmEventsRequest, VmEvent, VmInfo, VmState, VmStateChangedEvent,
    },
};
use hyper_util::rt::TokioIo;
use image_service::IMAGE_SERVICE_SOCKET;
use log::{error, info, warn};
use nix::unistd::Pid;
use prost::Message;
use prost_types::Any;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::{broadcast, mpsc, oneshot};
use tonic::{
    transport::{Channel, Endpoint, Error as TonicTransportError, Uri},
    Status,
};
use tower::service_fn;
use uuid::Uuid;

pub(crate) async fn get_image_service_client(
) -> Result<ImageServiceClient<Channel>, TonicTransportError> {
    let socket_path = PathBuf::from(IMAGE_SERVICE_SOCKET);
    Endpoint::try_from("http://[::1]:50051")
        .unwrap()
        .connect_with_connector(service_fn(move |_: Uri| {
            let socket_path = socket_path.clone();
            async move {
                tokio::net::UnixStream::connect(socket_path)
                    .await
                    .map(TokioIo::new)
            }
        }))
        .await
        .map(ImageServiceClient::new)
}

async fn initiate_image_pull_for_vm(req: &CreateVmRequest) -> Result<String, VmServiceError> {
    let image_ref = match req.config.as_ref() {
        Some(config) if !config.image_ref.is_empty() => config.image_ref.clone(),
        _ => {
            return Err(VmServiceError::InvalidArgument(
                "VmConfig with a non-empty image_ref is required".to_string(),
            ));
        }
    };

    info!("VmDispatcher: Requesting image pull for {image_ref}");
    let mut client = get_image_service_client()
        .await
        .map_err(|e| VmServiceError::ImageService(format!("Could not connect: {e}")))?;

    let response = client
        .pull_image(PullImageRequest {
            image_ref: image_ref.clone(),
        })
        .await
        .map_err(|status| {
            VmServiceError::ImageService(format!("PullImage RPC failed for {image_ref}: {status}"))
        })?;

    let image_uuid = response.into_inner().image_uuid;
    info!("VmDispatcher: Image pull for {image_ref} initiated. UUID: {image_uuid}");
    Ok(image_uuid)
}

async fn prepare_vm_creation(
    repository: &VmRepository,
    req: &CreateVmRequest,
) -> Result<(Uuid, String), VmServiceError> {
    let vm_id_res: Result<(Uuid, bool), VmServiceError> =
        if let Some(id_str) = req.vm_id.as_deref().filter(|s| !s.is_empty()) {
            match Uuid::parse_str(id_str) {
                Ok(id) if !id.is_nil() => Ok((id, true)),
                Ok(_) => Err(VmServiceError::InvalidArgument(
                    "Provided vm_id cannot be the nil UUID.".to_string(),
                )),
                Err(_) => Err(VmServiceError::InvalidArgument(
                    "Provided vm_id is not a valid UUID format.".to_string(),
                )),
            }
        } else {
            Ok((Uuid::new_v4(), false))
        };

    let (vm_id, is_user_provided) = vm_id_res?;

    if is_user_provided && repository.get_vm(vm_id).await?.is_some() {
        return Err(VmServiceError::AlreadyExists(format!(
            "VM with ID {vm_id} already exists."
        )));
    }

    let image_uuid_str = initiate_image_pull_for_vm(req).await?;
    let image_uuid = Uuid::parse_str(&image_uuid_str)
        .map_err(|e| VmServiceError::ImageService(format!("Failed to parse image UUID: {e}")))?;

    let record = VmRecord {
        vm_id,
        image_uuid,
        status: VmStatus {
            state: VmState::Creating,
            last_msg: "VM creation initiated".to_string(),
            process_id: None,
        },
        config: req.config.clone().unwrap(),
    };

    repository.save_vm(&record).await?;
    info!("VmDispatcher: Saved initial record for VM {vm_id}");
    Ok((vm_id, image_uuid_str))
}

async fn get_vm_info(
    repository: &VmRepository,
    req: &GetVmRequest,
) -> Result<VmInfo, VmServiceError> {
    let vm_id = Uuid::parse_str(&req.vm_id)
        .map_err(|_| VmServiceError::InvalidArgument("Invalid VM ID format.".to_string()))?;

    match repository.get_vm(vm_id).await? {
        Some(record) => Ok(VmInfo {
            vm_id: record.vm_id.to_string(),
            state: record.status.state as i32,
            config: Some(record.config),
        }),
        None => Err(VmServiceError::Vmm(crate::vmm::VmmError::VmNotFound(
            vm_id.to_string(),
        ))),
    }
}

async fn parse_vm_id_and_get_record(
    vm_id_str: &str,
    repository: &VmRepository,
) -> Result<(Uuid, VmRecord), VmServiceError> {
    let vm_id = Uuid::parse_str(vm_id_str)
        .map_err(|_| VmServiceError::InvalidArgument("Invalid VM ID format.".to_string()))?;

    match repository.get_vm(vm_id).await? {
        Some(record) => Ok((vm_id, record)),
        None => Err(VmServiceError::Vmm(crate::vmm::VmmError::VmNotFound(
            vm_id.to_string(),
        ))),
    }
}

pub(crate) async fn handle_create_vm_command(
    repository: &VmRepository,
    req: CreateVmRequest,
    responder: oneshot::Sender<Result<CreateVmResponse, VmServiceError>>,
    hypervisor: Arc<dyn Hypervisor>,
    event_bus_tx: mpsc::Sender<VmEventWrapper>,
) {
    let result = prepare_vm_creation(repository, &req).await;

    match result {
        Ok((vm_id, image_uuid_str)) => {
            tokio::spawn(worker::handle_create_vm(
                vm_id.to_string(),
                req,
                image_uuid_str,
                responder,
                hypervisor,
                event_bus_tx,
            ));
        }
        Err(e) => {
            error!("VmDispatcher: Failed to handle CreateVm command: {e}");
            if responder.send(Err(e)).is_err() {
                error!(
                    "VmDispatcher: Failed to send error response for CreateVm. Responder closed."
                );
            }
        }
    }
}

pub(crate) async fn handle_get_vm_command(
    repository: &VmRepository,
    req: GetVmRequest,
    responder: oneshot::Sender<Result<VmInfo, VmServiceError>>,
) {
    let result = get_vm_info(repository, &req).await;

    if responder.send(result).is_err() {
        error!("VmDispatcher: Failed to send response for GetVm.");
    }
}

pub(crate) async fn handle_stream_vm_events_command(
    repository: &VmRepository,
    req: StreamVmEventsRequest,
    stream_tx: mpsc::Sender<Result<VmEvent, Status>>,
    status_channel_tx: broadcast::Sender<VmEventWrapper>,
) {
    if let Some(vm_id_str) = req.vm_id.clone() {
        let vm_id = match Uuid::parse_str(&vm_id_str) {
            Ok(id) => id,
            Err(_) => {
                let status = Status::invalid_argument("Invalid VM ID format.");
                if stream_tx.send(Err(status)).await.is_err() {
                    warn!("StreamEvents: Client for {vm_id_str} disconnected before error could be sent.");
                }
                return;
            }
        };

        match repository.get_vm(vm_id).await {
            Ok(Some(record)) => {
                info!(
                    "StreamEvents: Sending initial state for VM {vm_id_str}: {:?}",
                    record.status.state
                );
                let state_change_event = VmStateChangedEvent {
                    new_state: record.status.state as i32,
                    reason: record.status.last_msg,
                };
                let initial_event = VmEvent {
                    vm_id: vm_id_str.clone(),
                    id: Uuid::new_v4().to_string(),
                    component_id: "vm-service-db".to_string(),
                    data: Some(Any {
                        type_url: "type.googleapis.com/feos.vm.vmm.api.v1.VmStateChangedEvent"
                            .to_string(),
                        value: state_change_event.encode_to_vec(),
                    }),
                };

                if stream_tx.send(Ok(initial_event)).await.is_err() {
                    info!(
                        "StreamEvents: Client for {vm_id_str} disconnected before live events could be streamed."
                    );
                    return;
                }

                tokio::spawn(worker::handle_stream_vm_events(
                    req,
                    stream_tx,
                    status_channel_tx,
                ));
            }
            Ok(None) => {
                warn!("VM with ID {vm_id} not found");
                if stream_tx
                    .send(Err(Status::not_found(format!(
                        "VM with ID {vm_id} not found"
                    ))))
                    .await
                    .is_err()
                {
                    warn!(
                        "StreamEvents: Client for {vm_id_str} disconnected before not-found error could be sent."
                    );
                }
            }
            Err(e) => {
                error!("StreamEvents: Failed to get VM {vm_id_str} from database for event stream: {e}");
                if stream_tx
                    .send(Err(Status::internal(
                        "Failed to retrieve VM information for event stream.",
                    )))
                    .await
                    .is_err()
                {
                    warn!("StreamEvents: Client for {vm_id_str} disconnected before internal-error could be sent.");
                }
            }
        }
    } else {
        info!("StreamEvents: Request to stream events for all VMs received.");
        match repository.list_all_vms().await {
            Ok(records) => {
                info!(
                    "StreamEvents: Found {} existing VMs to send initial state for.",
                    records.len()
                );
                for record in records {
                    let state_change_event = VmStateChangedEvent {
                        new_state: record.status.state as i32,
                        reason: format!("Initial state from DB: {}", record.status.last_msg),
                    };
                    let initial_event = VmEvent {
                        vm_id: record.vm_id.to_string(),
                        id: Uuid::new_v4().to_string(),
                        component_id: "vm-service-db".to_string(),
                        data: Some(Any {
                            type_url: "type.googleapis.com/feos.vm.vmm.api.v1.VmStateChangedEvent"
                                .to_string(),
                            value: state_change_event.encode_to_vec(),
                        }),
                    };

                    if stream_tx.send(Ok(initial_event)).await.is_err() {
                        info!("StreamEvents: Client for all VMs disconnected while sending initial states.");
                        return;
                    }
                }
            }
            Err(e) => {
                error!("StreamEvents: Failed to list all VMs from database for event stream: {e}");
                if stream_tx
                    .send(Err(Status::internal(
                        "Failed to retrieve initial VM list for event stream.",
                    )))
                    .await
                    .is_err()
                {
                    warn!("StreamEvents: Client for all VMs disconnected before internal-error could be sent.");
                }
                return;
            }
        }

        info!("StreamEvents: Initial states sent. Starting live event stream for all VMs.");
        tokio::spawn(worker::handle_stream_vm_events(
            req,
            stream_tx,
            status_channel_tx,
        ));
    }
}

pub(crate) async fn handle_delete_vm_command(
    repository: &VmRepository,
    healthcheck_cancel_bus: &broadcast::Sender<Uuid>,
    req: DeleteVmRequest,
    responder: oneshot::Sender<Result<DeleteVmResponse, VmServiceError>>,
    hypervisor: Arc<dyn Hypervisor>,
    event_bus_tx: mpsc::Sender<VmEventWrapper>,
) {
    let vm_id = match Uuid::parse_str(&req.vm_id) {
        Ok(id) => id,
        Err(_) => {
            let _ = responder.send(Err(VmServiceError::InvalidArgument(
                "Invalid VM ID format.".to_string(),
            )));
            return;
        }
    };

    match repository.get_vm(vm_id).await {
        Ok(Some(record)) => {
            let image_uuid_to_delete = record.image_uuid.to_string();
            let process_id_to_kill = record.status.process_id;

            if let Err(e) = repository.delete_vm(vm_id).await {
                error!("Failed to delete VM {vm_id} from database: {e}");
                let _ = responder.send(Err(e.into()));
                return;
            }
            info!("VmDispatcher: Deleted record for VM {vm_id} from database.");

            if let Err(e) = healthcheck_cancel_bus.send(vm_id) {
                warn!("VmDispatcher: Failed to send healthcheck cancellation for {vm_id}: {e}");
            }

            tokio::spawn(worker::handle_delete_vm(
                req,
                image_uuid_to_delete,
                process_id_to_kill,
                responder,
                hypervisor,
                event_bus_tx,
            ));
        }
        Ok(None) => {
            let msg = format!("VM with ID {vm_id} not found in database for deletion");
            warn!("VmDispatcher: {msg}. Still attempting hypervisor cleanup.");

            if let Err(e) = healthcheck_cancel_bus.send(vm_id) {
                warn!("VmDispatcher: Failed to send healthcheck cancellation for {vm_id}: {e}");
            }

            tokio::spawn(worker::handle_delete_vm(
                req,
                String::new(),
                None,
                responder,
                hypervisor,
                event_bus_tx,
            ));
        }
        Err(e) => {
            error!("Failed to get VM {vm_id} from database: {e}");
            let _ = responder.send(Err(e.into()));
        }
    }
}

pub(crate) async fn handle_list_vms_command(
    repository: &VmRepository,
    _req: ListVmsRequest,
    responder: oneshot::Sender<Result<ListVmsResponse, VmServiceError>>,
) {
    let result = repository.list_all_vms().await.map(|records| {
        let vms = records
            .into_iter()
            .map(|record| VmInfo {
                vm_id: record.vm_id.to_string(),
                state: record.status.state as i32,
                config: Some(record.config),
            })
            .collect();
        ListVmsResponse { vms }
    });

    if responder.send(result.map_err(Into::into)).is_err() {
        error!("VmDispatcher: Failed to send response for ListVms.");
    }
}

pub(crate) async fn handle_start_vm_command(
    repository: &VmRepository,
    req: StartVmRequest,
    responder: oneshot::Sender<Result<StartVmResponse, VmServiceError>>,
    hypervisor: Arc<dyn Hypervisor>,
    event_bus_tx: mpsc::Sender<VmEventWrapper>,
    healthcheck_cancel_bus_tx: &broadcast::Sender<Uuid>,
) {
    let (_vm_id, record) = match parse_vm_id_and_get_record(&req.vm_id, repository).await {
        Ok(result) => result,
        Err(e) => {
            let _ = responder.send(Err(e));
            return;
        }
    };

    let current_state = record.status.state;
    if !matches!(current_state, VmState::Created | VmState::Stopped) {
        let _ = responder.send(Err(VmServiceError::InvalidState(format!(
            "Cannot start VM in {current_state:?} state. Must be in Created or Stopped."
        ))));
        return;
    }

    let cancel_bus = healthcheck_cancel_bus_tx.subscribe();
    tokio::spawn(worker::handle_start_vm(
        req,
        responder,
        hypervisor,
        event_bus_tx,
        cancel_bus,
    ));
}

pub(crate) async fn handle_shutdown_vm_command(
    repository: &VmRepository,
    req: ShutdownVmRequest,
    responder: oneshot::Sender<Result<ShutdownVmResponse, VmServiceError>>,
    hypervisor: Arc<dyn Hypervisor>,
    event_bus_tx: mpsc::Sender<VmEventWrapper>,
) {
    let (_vm_id, record) = match parse_vm_id_and_get_record(&req.vm_id, repository).await {
        Ok(result) => result,
        Err(e) => {
            let _ = responder.send(Err(e));
            return;
        }
    };

    let current_state = record.status.state;
    if current_state != VmState::Running {
        let _ = responder.send(Err(VmServiceError::InvalidState(format!(
            "Cannot shutdown VM in {current_state:?} state. Must be in Running."
        ))));
        return;
    }

    tokio::spawn(worker::handle_shutdown_vm(
        req,
        responder,
        hypervisor,
        event_bus_tx,
    ));
}

pub(crate) async fn handle_pause_vm_command(
    repository: &VmRepository,
    req: PauseVmRequest,
    responder: oneshot::Sender<Result<PauseVmResponse, VmServiceError>>,
    hypervisor: Arc<dyn Hypervisor>,
    event_bus_tx: mpsc::Sender<VmEventWrapper>,
) {
    let (_vm_id, record) = match parse_vm_id_and_get_record(&req.vm_id, repository).await {
        Ok(result) => result,
        Err(e) => {
            let _ = responder.send(Err(e));
            return;
        }
    };

    let current_state = record.status.state;
    if current_state != VmState::Running {
        let _ = responder.send(Err(VmServiceError::InvalidState(format!(
            "Cannot pause VM in {current_state:?} state. Must be in Running."
        ))));
        return;
    }

    tokio::spawn(worker::handle_pause_vm(
        req,
        responder,
        hypervisor,
        event_bus_tx,
    ));
}

pub(crate) async fn handle_resume_vm_command(
    repository: &VmRepository,
    req: ResumeVmRequest,
    responder: oneshot::Sender<Result<ResumeVmResponse, VmServiceError>>,
    hypervisor: Arc<dyn Hypervisor>,
    event_bus_tx: mpsc::Sender<VmEventWrapper>,
) {
    let (_vm_id, record) = match parse_vm_id_and_get_record(&req.vm_id, repository).await {
        Ok(result) => result,
        Err(e) => {
            let _ = responder.send(Err(e));
            return;
        }
    };

    let current_state = record.status.state;
    if current_state != VmState::Paused {
        let _ = responder.send(Err(VmServiceError::InvalidState(format!(
            "Cannot resume VM in {current_state:?} state. Must be in Paused."
        ))));
        return;
    }

    tokio::spawn(worker::handle_resume_vm(
        req,
        responder,
        hypervisor,
        event_bus_tx,
    ));
}

pub(crate) async fn handle_attach_disk_command(
    repository: &VmRepository,
    req: AttachDiskRequest,
    responder: oneshot::Sender<Result<AttachDiskResponse, VmServiceError>>,
    hypervisor: Arc<dyn Hypervisor>,
) {
    let (_vm_id, record) = match parse_vm_id_and_get_record(&req.vm_id, repository).await {
        Ok(result) => result,
        Err(e) => {
            let _ = responder.send(Err(e));
            return;
        }
    };

    let current_state = record.status.state;
    if !matches!(
        current_state,
        VmState::Created | VmState::Running | VmState::Paused | VmState::Stopped
    ) {
        let _ = responder.send(Err(VmServiceError::InvalidState(format!(
            "Cannot attach disk to VM in {current_state:?} state."
        ))));
        return;
    }

    tokio::spawn(worker::handle_attach_disk(req, responder, hypervisor));
}

pub(crate) async fn handle_remove_disk_command(
    repository: &VmRepository,
    req: RemoveDiskRequest,
    responder: oneshot::Sender<Result<RemoveDiskResponse, VmServiceError>>,
    hypervisor: Arc<dyn Hypervisor>,
) {
    let (_vm_id, record) = match parse_vm_id_and_get_record(&req.vm_id, repository).await {
        Ok(result) => result,
        Err(e) => {
            let _ = responder.send(Err(e));
            return;
        }
    };

    let current_state = record.status.state;
    if !matches!(
        current_state,
        VmState::Created | VmState::Running | VmState::Paused | VmState::Stopped
    ) {
        let _ = responder.send(Err(VmServiceError::InvalidState(format!(
            "Cannot remove disk from VM in {current_state:?} state."
        ))));
        return;
    }

    tokio::spawn(worker::handle_remove_disk(req, responder, hypervisor));
}

pub(crate) async fn check_and_cleanup_vms(
    repository: &VmRepository,
    hypervisor: Arc<dyn Hypervisor>,
    event_bus_tx: mpsc::Sender<VmEventWrapper>,
    healthcheck_cancel_bus: &broadcast::Sender<Uuid>,
    vms: Vec<VmRecord>,
) {
    for vm in vms {
        if let Some(pid) = vm.status.process_id {
            let pid_obj = Pid::from_raw(pid as i32);
            let process_exists = nix::sys::signal::kill(pid_obj, None).is_ok();

            if process_exists {
                info!("VmDispatcher (Sanity Check): Found running VM {} (PID: {}) from previous session. Starting health monitor.", vm.vm_id, pid);
                let cancel_bus = healthcheck_cancel_bus.subscribe();
                worker::start_healthcheck_monitor(
                    vm.vm_id.to_string(),
                    hypervisor.clone(),
                    event_bus_tx.clone(),
                    cancel_bus,
                );
            } else {
                warn!("VmDispatcher (Sanity Check): Found VM {} in DB with PID {}, but process does not exist. Cleaning up.", vm.vm_id, pid);
                let (resp_tx, resp_rx) = oneshot::channel();
                let req = DeleteVmRequest {
                    vm_id: vm.vm_id.to_string(),
                };
                let vm_id_for_log = vm.vm_id;

                handle_delete_vm_command(
                    repository,
                    healthcheck_cancel_bus,
                    req,
                    resp_tx,
                    hypervisor.clone(),
                    event_bus_tx.clone(),
                )
                .await;

                match resp_rx.await {
                    Ok(Ok(_)) => info!("VmDispatcher (Sanity Check): Successfully cleaned up zombie VM {vm_id_for_log}."),
                    Ok(Err(status)) => error!("VmDispatcher (Sanity Check): Failed to clean up zombie VM {vm_id_for_log}: {status}"),
                    Err(_) => error!("VmDispatcher (Sanity Check): Cleanup task for zombie VM {vm_id_for_log} did not return a response."),
                }
            }
        }
    }
}

pub(crate) async fn perform_startup_sanity_check(
    repository: &VmRepository,
    hypervisor: Arc<dyn Hypervisor>,
    event_bus_tx: mpsc::Sender<VmEventWrapper>,
    healthcheck_cancel_bus: &broadcast::Sender<Uuid>,
) {
    info!("VmDispatcher: Running initial sanity check...");
    match repository.list_all_vms().await {
        Ok(vms) => {
            if vms.is_empty() {
                info!("VmDispatcher (Sanity Check): No VMs found in persistence, check complete.");
            } else {
                info!(
                    "VmDispatcher (Sanity Check): Found {} VMs in persistence, checking status...",
                    vms.len()
                );
                check_and_cleanup_vms(
                    repository,
                    hypervisor,
                    event_bus_tx,
                    healthcheck_cancel_bus,
                    vms,
                )
                .await;
                info!("VmDispatcher (Sanity Check): Check complete.");
            }
        }
        Err(e) => {
            error!("VmDispatcher (Sanity Check): Failed to list VMs from repository: {e}. Skipping check.");
        }
    }
}
