use crate::error::TaskError;
use crate::worker;
use crate::{Command, Container, Event, Status, WaitResponse};
use log::{info, warn};
use std::collections::HashMap;
use tokio::sync::mpsc;

pub struct Dispatcher {
    cmd_rx: mpsc::Receiver<Command>,
    event_rx: mpsc::Receiver<Event>,
    event_tx: mpsc::Sender<Event>,
    containers: HashMap<String, Container>,
}

impl Dispatcher {
    pub fn new(cmd_rx: mpsc::Receiver<Command>) -> Self {
        let (event_tx, event_rx) = mpsc::channel(32);
        Self {
            cmd_rx,
            event_rx,
            event_tx,
            containers: HashMap::new(),
        }
    }

    pub async fn run(mut self) {
        info!("Dispatcher: Running and waiting for commands and events.");
        loop {
            tokio::select! {
                Some(cmd) = self.cmd_rx.recv() => {
                    self.handle_command(cmd).await;
                },
                Some(event) = self.event_rx.recv() => {
                    self.handle_event(event).await;
                },
                else => {
                    info!("Dispatcher: A channel closed, shutting down.");
                    break;
                }
            }
        }
    }

    async fn handle_command(&mut self, cmd: Command) {
        match cmd {
            Command::Create { req, responder } => {
                let id = req.container_id.clone();
                if self.containers.contains_key(&id) {
                    let _ = responder.send(Err(TaskError::ContainerAlreadyExists(id)));
                    return;
                }

                self.containers.insert(
                    id.clone(),
                    Container {
                        status: Status::Creating,
                        pid: None,
                        bundle_path: req.bundle_path.clone(),
                        exit_code: None,
                        wait_responder: None,
                    },
                );

                tokio::spawn(worker::handle_create(req, self.event_tx.clone(), responder));
            }

            Command::Start { req, responder } => {
                let id = req.container_id.clone();
                match self.containers.get(&id) {
                    Some(container) if container.status == Status::Created => {
                        tokio::spawn(worker::handle_start(
                            req,
                            container.pid.expect("Created container must have PID"),
                            self.event_tx.clone(),
                            responder,
                        ));
                    }
                    Some(container) => {
                        let _ = responder.send(Err(TaskError::InvalidState {
                            id,
                            current_state: container.status,
                            required_states: vec![Status::Created],
                        }));
                    }
                    None => {
                        let _ = responder.send(Err(TaskError::ContainerNotFound(id)));
                    }
                }
            }

            Command::Kill { req, responder } => {
                let id = req.container_id.clone();
                match self.containers.get(&id) {
                    Some(container) if container.status == Status::Running => {
                        tokio::spawn(worker::handle_kill(req, responder));
                    }
                    Some(container) => {
                        let _ = responder.send(Err(TaskError::InvalidState {
                            id,
                            current_state: container.status,
                            required_states: vec![Status::Running],
                        }));
                    }
                    None => {
                        let _ = responder.send(Err(TaskError::ContainerNotFound(id)));
                    }
                }
            }
            Command::Delete { req, responder } => {
                let id = req.container_id.clone();
                match self.containers.get(&id) {
                    Some(container) if container.status != Status::Running => {
                        tokio::spawn(worker::handle_delete(req, self.event_tx.clone(), responder));
                    }
                    Some(container) => {
                        let _ = responder.send(Err(TaskError::InvalidState {
                            id,
                            current_state: container.status,
                            required_states: vec![Status::Created, Status::Stopped],
                        }));
                    }
                    None => {
                        let _ = responder.send(Err(TaskError::ContainerNotFound(id)));
                    }
                }
            }
            Command::Wait { req, responder } => {
                let id = req.container_id;
                match self.containers.get_mut(&id) {
                    Some(container) if container.status == Status::Stopped => {
                        // Container has already stopped, respond immediately.
                        let exit_code = container.exit_code.unwrap_or(255);
                        info!("Dispatcher: Responding to Wait for already stopped container {id} with code {exit_code}");
                        let _ = responder.send(Ok(WaitResponse { exit_code }));
                    }
                    Some(container) if container.status == Status::Running => {
                        // Container is running, store the responder to be used when the stop event arrives.
                        if container.wait_responder.is_some() {
                            // Another client is already waiting.
                            let _ = responder.send(Err(TaskError::Internal(
                                "Another client is already waiting on this container".to_string(),
                            )));
                        } else {
                            info!("Dispatcher: Client is now waiting for container {id} to stop");
                            container.wait_responder = Some(responder);
                        }
                    }
                    Some(container) => {
                        let _ = responder.send(Err(TaskError::InvalidState {
                            id,
                            current_state: container.status,
                            required_states: vec![Status::Running, Status::Stopped],
                        }));
                    }
                    None => {
                        let _ = responder.send(Err(TaskError::ContainerNotFound(id)));
                    }
                }
            }
        }
    }

    async fn handle_event(&mut self, event: Event) {
        info!("Dispatcher: Handling event: {event:?}");
        match event {
            Event::ContainerCreated { id, pid } => {
                if let Some(container) = self.containers.get_mut(&id) {
                    container.status = Status::Created;
                    container.pid = Some(pid);
                }
            }
            Event::ContainerCreateFailed { id, error: _ } => {
                self.containers.remove(&id);
            }
            Event::ContainerStarted { id } => {
                if let Some(container) = self.containers.get_mut(&id) {
                    container.status = Status::Running;
                }
            }
            Event::ContainerStartFailed { id, error: _ } => {
                if let Some(container) = self.containers.get_mut(&id) {
                    // Revert state if start fails
                    container.status = Status::Created;
                }
            }
            Event::ContainerStopped { id, exit_code } => {
                if let Some(container) = self.containers.get_mut(&id) {
                    container.status = Status::Stopped;
                    container.exit_code = Some(exit_code);
                    if let Some(responder) = container.wait_responder.take() {
                        info!(
                            "Dispatcher: Fulfilling pending Wait request for {id} with exit code {exit_code}"
                        );
                        if responder.send(Ok(WaitResponse { exit_code })).is_err() {
                            warn!("Dispatcher: Client waiting on container {id} disconnected");
                        }
                    }
                }
            }
            Event::ContainerDeleted { id } => {
                self.containers.remove(&id);
            }
        }
    }
}
