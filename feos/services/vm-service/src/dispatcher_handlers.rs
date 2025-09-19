// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    persistence::{repository::VmRepository, VmRecord, VmStatus},
    vmm::Hypervisor,
    worker, VmEventWrapper,
};
use feos_proto::{
    image_service::{image_service_client::ImageServiceClient, PullImageRequest},
    vm_service::{
        CreateVmRequest, CreateVmResponse, DeleteVmRequest, DeleteVmResponse, GetVmRequest,
        ListVmsRequest, ListVmsResponse, StreamVmEventsRequest, VmEvent, VmInfo, VmState,
        VmStateChangedEvent,
    },
};
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
            tokio::net::UnixStream::connect(socket_path.clone())
        }))
        .await
        .map(ImageServiceClient::new)
}

async fn initiate_image_pull_for_vm(req: &CreateVmRequest) -> Result<String, Status> {
    let image_ref = match req.config.as_ref() {
        Some(config) if !config.image_ref.is_empty() => config.image_ref.clone(),
        _ => {
            return Err(Status::invalid_argument(
                "VmConfig with a non-empty image_ref is required",
            ));
        }
    };

    info!("VmDispatcher: Requesting image pull for {image_ref}");
    let mut client = get_image_service_client()
        .await
        .map_err(|e| Status::unavailable(format!("Could not connect to ImageService: {e}")))?;

    let response = client
        .pull_image(PullImageRequest {
            image_ref: image_ref.clone(),
        })
        .await
        .map_err(|status| {
            let msg = format!("PullImage RPC failed for {image_ref}: {status}");
            error!("VmDispatcher: {msg}");
            Status::unavailable(msg)
        })?;

    let image_uuid = response.into_inner().image_uuid;
    info!("VmDispatcher: Image pull for {image_ref} initiated. UUID: {image_uuid}");
    Ok(image_uuid)
}

pub(crate) async fn handle_create_vm_command(
    repository: &VmRepository,
    req: CreateVmRequest,
    responder: oneshot::Sender<Result<CreateVmResponse, Status>>,
    hypervisor: Arc<dyn Hypervisor>,
    event_bus_tx: mpsc::Sender<VmEventWrapper>,
) {
    let vm_id_res: Result<(Uuid, bool), Status> =
        if let Some(id_str) = req.vm_id.as_deref().filter(|s| !s.is_empty()) {
            match Uuid::parse_str(id_str) {
                Ok(id) if !id.is_nil() => Ok((id, true)),
                Ok(_) => Err(Status::invalid_argument(
                    "Provided vm_id cannot be the nil UUID.",
                )),
                Err(_) => Err(Status::invalid_argument(
                    "Provided vm_id is not a valid UUID format.",
                )),
            }
        } else {
            Ok((Uuid::new_v4(), false))
        };

    let (vm_id, is_user_provided) = match vm_id_res {
        Ok(val) => val,
        Err(status) => {
            if responder.send(Err(status)).is_err() {
                error!(
                    "VmDispatcher: Failed to send error response for CreateVm. Responder closed."
                );
            }
            return;
        }
    };

    if is_user_provided {
        match repository.get_vm(vm_id).await {
            Ok(Some(_)) => {
                let status = Status::already_exists(format!("VM with ID {vm_id} already exists."));
                if responder.send(Err(status)).is_err() {
                    error!("VmDispatcher: Failed to send error response for CreateVm. Responder closed.");
                }
                return;
            }
            Ok(None) => {}
            Err(e) => {
                let status = Status::internal(format!("Failed to check DB for existing VM: {e}"));
                if responder.send(Err(status)).is_err() {
                    error!("VmDispatcher: Failed to send error response for CreateVm. Responder closed.");
                }
                return;
            }
        }
    }

    match initiate_image_pull_for_vm(&req).await {
        Ok(image_uuid_str) => {
            let image_uuid = match Uuid::parse_str(&image_uuid_str) {
                Ok(uuid) => uuid,
                Err(e) => {
                    let status = Status::internal(format!("Failed to parse image UUID: {e}"));
                    if responder.send(Err(status)).is_err() {
                        error!("VmDispatcher: Failed to send error response for CreateVm. Responder closed.");
                    }
                    return;
                }
            };

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

            if let Err(e) = repository.save_vm(&record).await {
                let status = Status::internal(format!("Failed to save VM to database: {e}"));
                error!("VmDispatcher: {message}", message = status.message());
                if responder.send(Err(status)).is_err() {
                    error!("VmDispatcher: Failed to send error response for CreateVm. Responder closed.");
                }
                return;
            }
            info!("VmDispatcher: Saved initial record for VM {vm_id}");

            tokio::spawn(worker::handle_create_vm(
                vm_id.to_string(),
                req,
                image_uuid_str,
                responder,
                hypervisor,
                event_bus_tx,
            ));
        }
        Err(status) => {
            if responder.send(Err(status)).is_err() {
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
    responder: oneshot::Sender<Result<VmInfo, Status>>,
) {
    let vm_id = match Uuid::parse_str(&req.vm_id) {
        Ok(id) => id,
        Err(_) => {
            let _ = responder.send(Err(Status::invalid_argument("Invalid VM ID format.")));
            return;
        }
    };

    match repository.get_vm(vm_id).await {
        Ok(Some(record)) => {
            let vm_info = VmInfo {
                vm_id: record.vm_id.to_string(),
                state: record.status.state as i32,
                config: Some(record.config),
            };
            let _ = responder.send(Ok(vm_info));
        }
        Ok(None) => {
            let _ = responder.send(Err(Status::not_found(format!(
                "VM with ID {vm_id} not found"
            ))));
        }
        Err(e) => {
            error!("Failed to get VM from database: {e}");
            let _ = responder.send(Err(Status::internal("Failed to retrieve VM information.")));
        }
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
                if stream_tx
                    .send(Err(Status::invalid_argument("Invalid VM ID format.")))
                    .await
                    .is_err()
                {
                    warn!(
                        "StreamEvents: Client for {vm_id_str} disconnected before error could be sent."
                    );
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
                error!(
                    "StreamEvents: Failed to get VM {vm_id_str} from database for event stream: {e}"
                );
                if stream_tx
                    .send(Err(Status::internal(
                        "Failed to retrieve VM information for event stream.",
                    )))
                    .await
                    .is_err()
                {
                    warn!(
                        "StreamEvents: Client for {vm_id_str} disconnected before internal-error could be sent."
                    );
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
    responder: oneshot::Sender<Result<DeleteVmResponse, Status>>,
    hypervisor: Arc<dyn Hypervisor>,
    event_bus_tx: mpsc::Sender<VmEventWrapper>,
) {
    let vm_id = match Uuid::parse_str(&req.vm_id) {
        Ok(id) => id,
        Err(_) => {
            let _ = responder.send(Err(Status::invalid_argument("Invalid VM ID format.")));
            return;
        }
    };

    match repository.get_vm(vm_id).await {
        Ok(Some(record)) => {
            let image_uuid_to_delete = record.image_uuid.to_string();
            let process_id_to_kill = record.status.process_id;

            if let Err(e) = repository.delete_vm(vm_id).await {
                error!("Failed to delete VM {vm_id} from database: {e}");
                let _ = responder.send(Err(Status::internal("Failed to delete VM from database.")));
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
            let _ = responder.send(Err(Status::internal("Failed to retrieve VM for deletion.")));
        }
    }
}

pub(crate) async fn handle_list_vms_command(
    repository: &VmRepository,
    _req: ListVmsRequest,
    responder: oneshot::Sender<Result<ListVmsResponse, Status>>,
) {
    match repository.list_all_vms().await {
        Ok(records) => {
            let vms = records
                .into_iter()
                .map(|record| VmInfo {
                    vm_id: record.vm_id.to_string(),
                    state: record.status.state as i32,
                    config: Some(record.config),
                })
                .collect();

            let response = ListVmsResponse { vms };
            if responder.send(Ok(response)).is_err() {
                error!("VmDispatcher: Failed to send response for ListVms.");
            }
        }
        Err(e) => {
            error!("VmDispatcher: Failed to list VMs from database: {e}");
            let status = Status::internal("Failed to retrieve VM list.");
            if responder.send(Err(status)).is_err() {
                error!("VmDispatcher: Failed to send error response for ListVms.");
            }
        }
    }
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
