// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    dispatcher_handlers::{
        handle_attach_disk_command, handle_attach_nic_command, handle_create_vm_command,
        handle_delete_vm_command, handle_get_vm_command, handle_list_vms_command,
        handle_pause_vm_command, handle_remove_disk_command, handle_remove_nic_command,
        handle_resume_vm_command, handle_shutdown_vm_command, handle_start_vm_command,
        handle_stream_vm_console_command, handle_stream_vm_events_command,
        perform_startup_sanity_check,
    },
    error::VmServiceError,
    persistence::repository::VmRepository,
    vmm::{factory, Hypervisor, VmmType},
    worker, Command, VmEventWrapper,
};
use feos_proto::vm_service::{VmState, VmStateChangedEvent};
use log::{debug, error, info};
use prost::Message;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

pub struct VmServiceDispatcher {
    rx: mpsc::Receiver<Command>,
    event_bus_tx: mpsc::Sender<VmEventWrapper>,
    event_bus_rx_for_dispatcher: mpsc::Receiver<VmEventWrapper>,
    status_channel_tx: broadcast::Sender<VmEventWrapper>,
    hypervisor: Arc<dyn Hypervisor>,
    repository: VmRepository,
    healthcheck_cancel_bus: broadcast::Sender<Uuid>,
}

impl VmServiceDispatcher {
    pub async fn new(rx: mpsc::Receiver<Command>, db_url: &str) -> Result<Self, VmServiceError> {
        let (event_bus_tx, event_bus_rx_for_dispatcher) = mpsc::channel(32);
        let (status_channel_tx, _) = broadcast::channel(32);
        let (healthcheck_cancel_bus, _) = broadcast::channel::<Uuid>(32);
        let hypervisor = Arc::from(factory(VmmType::CloudHypervisor));
        info!("VmDispatcher: Connecting to persistence layer at {db_url}...");
        let repository = VmRepository::connect(db_url).await?;
        info!("VmDispatcher: Persistence layer connected successfully.");
        Ok(Self {
            rx,
            event_bus_tx,
            event_bus_rx_for_dispatcher,
            status_channel_tx,
            hypervisor,
            repository,
            healthcheck_cancel_bus,
        })
    }

    pub async fn run(mut self) {
        perform_startup_sanity_check(
            &self.repository,
            self.hypervisor.clone(),
            self.event_bus_tx.clone(),
            &self.healthcheck_cancel_bus,
        )
        .await;

        info!("VmDispatcher: Running and waiting for commands and events.");
        loop {
            tokio::select! {
                biased;
                Some(cmd) = self.rx.recv() => {
                    let hypervisor = self.hypervisor.clone();
                    let event_bus_tx = self.event_bus_tx.clone();
                    let status_channel_tx = self.status_channel_tx.clone();

                    match cmd {
                        Command::CreateVm(req, responder) => {
                            handle_create_vm_command(&self.repository, req, responder, hypervisor, event_bus_tx).await;
                        }
                        Command::StartVm(req, responder) => {
                            handle_start_vm_command(&self.repository, req, responder, hypervisor, event_bus_tx, &self.healthcheck_cancel_bus).await;
                        }
                        Command::GetVm(req, responder) => {
                            handle_get_vm_command(&self.repository, req, responder).await;
                        }
                        Command::StreamVmEvents(req, stream_tx) => {
                            handle_stream_vm_events_command(&self.repository, req, stream_tx, status_channel_tx).await;
                        }
                        Command::DeleteVm(req, responder) => {
                            handle_delete_vm_command(&self.repository, &self.healthcheck_cancel_bus, req, responder, hypervisor, event_bus_tx).await;
                        }
                        Command::StreamVmConsole(input_stream, output_tx) => {
                            handle_stream_vm_console_command(&self.repository, *input_stream, output_tx, hypervisor).await;
                        }
                        Command::ListVms(req, responder) => {
                            handle_list_vms_command(&self.repository, req, responder).await;
                        }
                        Command::PingVm(req, responder) => {
                            tokio::spawn(worker::handle_ping_vm(req, responder, hypervisor));
                        }
                        Command::ShutdownVm(req, responder) => {
                            handle_shutdown_vm_command(&self.repository, req, responder, hypervisor, event_bus_tx).await;
                        }
                        Command::PauseVm(req, responder) => {
                            handle_pause_vm_command(&self.repository, req, responder, hypervisor, event_bus_tx).await;
                        }
                        Command::ResumeVm(req, responder) => {
                            handle_resume_vm_command(&self.repository, req, responder, hypervisor, event_bus_tx).await;
                        }
                        Command::AttachDisk(req, responder) => {
                            handle_attach_disk_command(&self.repository, req, responder, hypervisor).await;
                        }
                        Command::RemoveDisk(req, responder) => {
                            handle_remove_disk_command(&self.repository, req, responder, hypervisor).await;
                        }
                        Command::AttachNic(req, responder) => {
                            handle_attach_nic_command(&self.repository, req, responder, hypervisor).await;
                        }
                        Command::RemoveNic(req, responder) => {
                            handle_remove_nic_command(&self.repository, req, responder, hypervisor).await;
                        }
                    }
                },
                Some(event) = self.event_bus_rx_for_dispatcher.recv() => {
                    self.handle_vm_event(event).await;
                }
                else => {
                    info!("VmDispatcher: A channel closed, shutting down.");
                    break;
                }
            }
        }
    }

    async fn handle_vm_event(&mut self, event_wrapper: VmEventWrapper) {
        let event_to_forward = event_wrapper.clone();
        let event = event_wrapper.event;

        info!(
            "VmDispatcher_LOGGER: Event for VM '{}': ID '{}', Component '{}', Data Type '{}'",
            event.vm_id,
            event.id,
            event.component_id,
            event.data.as_ref().map_or("None", |d| &d.type_url)
        );

        let vm_id_uuid = match Uuid::parse_str(&event.vm_id) {
            Ok(id) => id,
            Err(e) => {
                error!(
                    "DatabaseUpdate: Could not parse UUID from event vm_id '{}': {e}",
                    &event.vm_id
                );
                return;
            }
        };

        if let Some(pid) = event_wrapper.process_id {
            info!("DatabaseUpdate: Updating pid for VM {vm_id_uuid} to {pid}");
            if let Err(e) = self.repository.update_vm_pid(vm_id_uuid, pid).await {
                error!("DatabaseUpdate: Failed to update pid for VM {vm_id_uuid}: {e}");
            }
        }

        if let Some(data) = &event.data {
            if data.type_url.contains("VmStateChangedEvent") {
                self.handle_vm_state_changed_event(
                    data,
                    vm_id_uuid,
                    &event.vm_id,
                    event_to_forward,
                )
                .await;
            }
        }
    }

    async fn handle_vm_state_changed_event(
        &mut self,
        data: &prost_types::Any,
        vm_id_uuid: Uuid,
        vm_id: &str,
        event_to_forward: VmEventWrapper,
    ) {
        match VmStateChangedEvent::decode(&*data.value) {
            Ok(state_change) => {
                let new_state = match VmState::try_from(state_change.new_state) {
                    Ok(s) => s,
                    Err(e) => {
                        error!(
                            "DatabaseUpdate: Invalid VmState value '{}' in event: {e}",
                            state_change.new_state
                        );
                        return;
                    }
                };

                info!(
                    "DatabaseUpdate: Updating status for VM {vm_id_uuid} to {new_state:?} with message: '{}'",
                    state_change.reason
                );
                match self
                    .repository
                    .update_vm_status(vm_id_uuid, new_state, &state_change.reason)
                    .await
                {
                    Ok(true) => {
                        if let Err(e) = self.status_channel_tx.send(event_to_forward) {
                            debug!(
                                "VmDispatcher: Failed to forward successful VM status event for {vm_id}: {e}"
                            );
                        }
                    }
                    Ok(false) => {
                        info!(
                            "DatabaseUpdate: Update for VM {vm_id_uuid} was a no-op (record likely already deleted). Event not forwarded."
                        );
                    }
                    Err(e) => {
                        error!(
                            "DatabaseUpdate: Failed to execute status update for VM {vm_id_uuid}: {e}"
                        );
                    }
                }
            }
            Err(e) => {
                error!("DatabaseUpdate: Failed to decode VmStateChangedEvent for VM {vm_id}: {e}");
            }
        }
    }
}
