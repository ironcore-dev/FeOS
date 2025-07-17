use crate::{
    persistence::{repository::VmRepository, VmRecord, VmStatus},
    vmm::{factory, Hypervisor, VmmType},
    vmservice_helper, worker, Command, CreateVmRequest, VmEventWrapper,
};
use anyhow::Result;
use feos_proto::{
    image_service::PullImageRequest,
    vm_service::{ListVmsResponse, VmEvent, VmInfo, VmState, VmStateChangedEvent},
};
use log::{error, info, warn};
use prost::Message;
use prost_types::Any;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tonic::Status;
use uuid::Uuid;

pub struct VmServiceDispatcher {
    rx: mpsc::Receiver<Command>,
    broadcast_tx: broadcast::Sender<VmEventWrapper>,
    broadcast_rx_for_logger: broadcast::Receiver<VmEventWrapper>,
    hypervisor: Arc<dyn Hypervisor>,
    repository: VmRepository,
}

impl VmServiceDispatcher {
    pub async fn new(rx: mpsc::Receiver<Command>, db_url: &str) -> Result<Self> {
        let (broadcast_tx, _) = broadcast::channel(32);
        let broadcast_rx_for_logger = broadcast_tx.subscribe();
        let hypervisor = Arc::from(factory(VmmType::CloudHypervisor));
        info!("VM_DISPATCHER: Connecting to persistence layer at {db_url}...");
        let repository = VmRepository::connect(db_url).await?;
        info!("VM_DISPATCHER: Persistence layer connected successfully.");
        Ok(Self {
            rx,
            broadcast_tx,
            broadcast_rx_for_logger,
            hypervisor,
            repository,
        })
    }

    async fn initiate_image_pull_for_vm(&self, req: &CreateVmRequest) -> Result<String, Status> {
        let image_ref = match req.config.as_ref() {
            Some(config) if !config.image_ref.is_empty() => config.image_ref.clone(),
            _ => {
                return Err(Status::invalid_argument(
                    "VmConfig with a non-empty image_ref is required",
                ));
            }
        };

        info!("VM_DISPATCHER: Requesting image pull for '{image_ref}'");
        let mut client = vmservice_helper::get_image_service_client()
            .await
            .map_err(|e| Status::unavailable(format!("Could not connect to ImageService: {e}")))?;

        let response = client
            .pull_image(PullImageRequest {
                image_ref: image_ref.clone(),
            })
            .await
            .map_err(|status| {
                let msg = format!("PullImage RPC failed for '{image_ref}': {status}");
                error!("VM_DISPATCHER: {msg}");
                Status::unavailable(msg)
            })?;

        let image_uuid = response.into_inner().image_uuid;
        info!("VM_DISPATCHER: Image pull for '{image_ref}' initiated. UUID: {image_uuid}");
        Ok(image_uuid)
    }

    async fn handle_vm_event(&mut self, event_wrapper: VmEventWrapper) {
        let event = event_wrapper.0;
        info!(
            "VM_DISPATCHER_LOGGER: Event for VM '{}': ID '{}', Component '{}', Data Type '{}'",
            event.vm_id,
            event.id,
            event.component_id,
            event.data.as_ref().map_or("None", |d| &d.type_url)
        );

        if let Some(data) = &event.data {
            if data.type_url.contains("VmStateChangedEvent") {
                match VmStateChangedEvent::decode(&*data.value) {
                    Ok(state_change) => {
                        let vm_id_uuid = match Uuid::parse_str(&event.vm_id) {
                            Ok(id) => id,
                            Err(e) => {
                                error!(
                                    "DB_UPDATE: Could not parse UUID from event vm_id '{}': {}",
                                    &event.vm_id, e
                                );
                                return;
                            }
                        };
                        let new_state = match VmState::try_from(state_change.new_state) {
                            Ok(s) => s,
                            Err(e) => {
                                error!(
                                    "DB_UPDATE: Invalid VmState value '{}' in event: {}",
                                    state_change.new_state, e
                                );
                                return;
                            }
                        };

                        info!(
                            "DB_UPDATE: Updating status for VM {} to {:?} with message: '{}'",
                            vm_id_uuid, new_state, &state_change.reason
                        );
                        if let Err(e) = self
                            .repository
                            .update_vm_status(vm_id_uuid, new_state, &state_change.reason)
                            .await
                        {
                            error!("DB_UPDATE: Failed to update status for VM {vm_id_uuid}: {e}");
                        }
                    }
                    Err(e) => {
                        error!(
                            "DB_UPDATE: Failed to decode VmStateChangedEvent for VM {}: {}",
                            event.vm_id, e
                        );
                    }
                }
            }
        }
    }

    pub async fn run(mut self) {
        info!("VM_DISPATCHER: Running and waiting for commands and events.");
        loop {
            tokio::select! {
                biased;
                Some(cmd) = self.rx.recv() => {
                    let hypervisor = self.hypervisor.clone();
                    let broadcast_tx = self.broadcast_tx.clone();
                    match cmd {
                        Command::CreateVm(req, responder) => {
                            let vm_id_res: Result<(Uuid, bool), Status> = if let Some(id_str) =
                                req.vm_id.as_deref().filter(|s| !s.is_empty())
                            {
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
                                        error!("VM_DISPATCHER: Failed to send error response for CreateVm. Responder closed.");
                                    }
                                    continue;
                                }
                            };

                            if is_user_provided {
                                match self.repository.get_vm(vm_id).await {
                                    Ok(Some(_)) => {
                                        let status = Status::already_exists(format!(
                                            "VM with ID {vm_id} already exists."
                                        ));
                                        if responder.send(Err(status)).is_err() {
                                            error!("VM_DISPATCHER: Failed to send error response for CreateVm. Responder closed.");
                                        }
                                        continue;
                                    }
                                    Ok(None) => {}
                                    Err(e) => {
                                        let status = Status::internal(format!(
                                            "Failed to check DB for existing VM: {e}"
                                        ));
                                        if responder.send(Err(status)).is_err() {
                                            error!("VM_DISPATCHER: Failed to send error response for CreateVm. Responder closed.");
                                        }
                                        continue;
                                    }
                                }
                            }

                            match self.initiate_image_pull_for_vm(&req).await {
                                Ok(image_uuid_str) => {
                                    let image_uuid = match Uuid::parse_str(&image_uuid_str) {
                                        Ok(uuid) => uuid,
                                        Err(e) => {
                                            let status = Status::internal(format!("Failed to parse image UUID: {e}"));
                                            if responder.send(Err(status)).is_err() {
                                                error!("VM_DISPATCHER: Failed to send error response for CreateVm. Responder closed.");
                                            }
                                            continue;
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

                                    if let Err(e) = self.repository.save_vm(&record).await {
                                        let status = Status::internal(format!("Failed to save VM to database: {e}"));
                                        error!("VM_DISPATCHER: {}", status.message());
                                        if responder.send(Err(status)).is_err() {
                                            error!("VM_DISPATCHER: Failed to send error response for CreateVm. Responder closed.");
                                        }
                                        continue;
                                    }
                                    info!("VM_DISPATCHER: Saved initial record for VM {vm_id}");

                                    tokio::spawn(worker::handle_create_vm(
                                        vm_id.to_string(),
                                        req,
                                        image_uuid_str,
                                        responder,
                                        hypervisor,
                                        broadcast_tx,
                                    ));
                                }
                                Err(status) => {
                                    if responder.send(Err(status)).is_err() {
                                        error!("VM_DISPATCHER: Failed to send error response for CreateVm. Responder closed.");
                                    }
                                }
                            }
                        }
                        Command::StartVm(req, responder) => {
                            tokio::spawn(worker::handle_start_vm(req, responder, hypervisor, broadcast_tx));
                        }
                        Command::GetVm(req, responder) => {
                             let vm_id = match Uuid::parse_str(&req.vm_id) {
                                Ok(id) => id,
                                Err(_) => {
                                    let _ = responder.send(Err(Status::invalid_argument("Invalid VM ID format.")));
                                    continue;
                                }
                            };

                            match self.repository.get_vm(vm_id).await {
                                Ok(Some(record)) => {
                                    let vm_info = VmInfo {
                                        vm_id: record.vm_id.to_string(),
                                        state: record.status.state as i32,
                                        config: Some(record.config),
                                    };
                                    let _ = responder.send(Ok(vm_info));
                                }
                                Ok(None) => {
                                    let _ = responder.send(Err(Status::not_found(format!("VM with ID {vm_id} not found"))));
                                }
                                Err(e) => {
                                    error!("Failed to get VM from database: {e}");
                                    let _ = responder.send(Err(Status::internal("Failed to retrieve VM information.")));
                                }
                            }
                        }
                        Command::StreamVmEvents(req, stream_tx) => {
                            let vm_id_str = req.vm_id.clone();
                            let vm_id = match Uuid::parse_str(&vm_id_str) {
                                Ok(id) => id,
                                Err(_) => {
                                    if stream_tx.send(Err(Status::invalid_argument("Invalid VM ID format."))).await.is_err() {
                                        warn!("STREAM_EVENTS: Client for {vm_id_str} disconnected before error could be sent.");
                                    }
                                    continue;
                                }
                            };

                            match self.repository.get_vm(vm_id).await {
                                Ok(Some(record)) => {
                                    info!("STREAM_EVENTS: Sending initial state for VM {}: {:?}", vm_id_str, record.status.state);
                                    let state_change_event = VmStateChangedEvent {
                                        new_state: record.status.state as i32,
                                        reason: record.status.last_msg
                                    };
                                    let initial_event = VmEvent {
                                        vm_id: vm_id_str.clone(),
                                        id: Uuid::new_v4().to_string(),
                                        component_id: "vm-service-db".to_string(),
                                        data: Some(Any {
                                            type_url: "type.googleapis.com/feos.vm.vmm.api.v1.VmStateChangedEvent".to_string(),
                                            value: state_change_event.encode_to_vec(),
                                        }),
                                    };

                                    if stream_tx.send(Ok(initial_event)).await.is_err() {
                                        info!("STREAM_EVENTS: Client for {vm_id_str} disconnected before live events could be streamed.");
                                        continue;
                                    }

                                    tokio::spawn(worker::handle_stream_vm_events(
                                        req,
                                        stream_tx,
                                        hypervisor,
                                        broadcast_tx,
                                    ));
                                }
                                Ok(None) => {
                                    warn!("STREAM_EVENTS: VM with ID {vm_id_str} not found in database.");
                                    if stream_tx.send(Err(Status::not_found(format!("VM with ID {vm_id} not found")))).await.is_err() {
                                        warn!("STREAM_EVENTS: Client for {vm_id_str} disconnected before not-found error could be sent.");
                                    }
                                }
                                Err(e) => {
                                    error!("STREAM_EVENTS: Failed to get VM {vm_id_str} from database for event stream: {e}");
                                    if stream_tx.send(Err(Status::internal("Failed to retrieve VM information for event stream."))).await.is_err() {
                                        warn!("STREAM_EVENTS: Client for {vm_id_str} disconnected before internal-error could be sent.");
                                    }
                                }
                            }
                        }
                        Command::DeleteVm(req, responder) => {
                            let vm_id = match Uuid::parse_str(&req.vm_id) {
                                Ok(id) => id,
                                Err(_) => {
                                    let _ = responder.send(Err(Status::invalid_argument("Invalid VM ID format.")));
                                    continue;
                                }
                            };

                             match self.repository.get_vm(vm_id).await {
                                Ok(Some(record)) => {
                                    let image_uuid_to_delete = record.image_uuid.to_string();

                                    if let Err(e) = self.repository.delete_vm(vm_id).await {
                                        error!("Failed to delete VM {vm_id} from database: {e}");
                                        let _ = responder.send(Err(Status::internal("Failed to delete VM from database.")));
                                        continue;
                                    }
                                    info!("VM_DISPATCHER: Deleted record for VM {vm_id} from database.");

                                    tokio::spawn(worker::handle_delete_vm(
                                        req,
                                        image_uuid_to_delete,
                                        responder,
                                        hypervisor,
                                        broadcast_tx,
                                    ));
                                }
                                Ok(None) => {
                                    let msg = format!("VM with ID {vm_id} not found in database for deletion");
                                    warn!("VM_DISPATCHER: {}. Still attempting hypervisor cleanup.", &msg);
                                    tokio::spawn(worker::handle_delete_vm(
                                        req,
                                        String::new(),
                                        responder,
                                        hypervisor,
                                        broadcast_tx,
                                    ));
                                }
                                Err(e) => {
                                    error!("Failed to get VM {vm_id} from database: {e}");
                                    let _ = responder.send(Err(Status::internal("Failed to retrieve VM for deletion.")));
                                }
                            }
                        }
                        Command::StreamVmConsole(input_stream, output_tx) => {
                            tokio::spawn(worker::handle_stream_vm_console(input_stream, output_tx, hypervisor));
                        }
                        Command::ListVms(_req, responder) => {
                            match self.repository.list_all_vms().await {
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
                                        error!("VM_DISPATCHER: Failed to send response for ListVms.");
                                    }
                                }
                                Err(e) => {
                                    error!("VM_DISPATCHER: Failed to list VMs from database: {e}");
                                    let status = Status::internal("Failed to retrieve VM list.");
                                    if responder.send(Err(status)).is_err() {
                                        error!("VM_DISPATCHER: Failed to send error response for ListVms.");
                                    }
                                }
                            }
                        }
                        Command::PingVm(req, responder) => {
                            tokio::spawn(worker::handle_ping_vm(req, responder, hypervisor));
                        }
                        Command::ShutdownVm(req, responder) => {
                            tokio::spawn(worker::handle_shutdown_vm(req, responder, hypervisor, broadcast_tx));
                        }
                        Command::PauseVm(req, responder) => {
                            tokio::spawn(worker::handle_pause_vm(req, responder, hypervisor, broadcast_tx));
                        }
                        Command::ResumeVm(req, responder) => {
                            tokio::spawn(worker::handle_resume_vm(req, responder, hypervisor, broadcast_tx));
                        }
                        Command::AttachDisk(req, responder) => {
                            tokio::spawn(worker::handle_attach_disk(req, responder, hypervisor));
                        }
                        Command::RemoveDisk(req, responder) => {
                            tokio::spawn(worker::handle_remove_disk(req, responder, hypervisor));
                        }
                    }
                },
                Ok(event) = self.broadcast_rx_for_logger.recv() => {
                    self.handle_vm_event(event).await;
                }
                else => {
                    info!("VM_DISPATCHER: A channel closed, shutting down.");
                    break;
                }
            }
        }
    }
}
