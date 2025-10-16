// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    error::ContainerServiceError,
    persistence::{repository::ContainerRepository, ContainerRecord},
    runtime::adapter::ContainerAdapter,
    worker, Command,
};
use feos_proto::{
    container_service::{ContainerInfo, ContainerState, ListContainersResponse},
    image_service::{image_service_client::ImageServiceClient, PullImageRequest},
};
use hyper_util::rt::TokioIo;
use image_service::IMAGE_SERVICE_SOCKET;
use log::{info, warn};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::mpsc;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;
use uuid::Uuid;

pub struct Dispatcher {
    rx: mpsc::Receiver<Command>,
    repository: ContainerRepository,
    adapter: Arc<ContainerAdapter>,
}

async fn get_image_service_client() -> Result<ImageServiceClient<Channel>, ContainerServiceError> {
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
        .map_err(|e| ContainerServiceError::ImageService(e.to_string()))
}

async fn initiate_image_pull(image_ref: &str) -> Result<String, ContainerServiceError> {
    info!("Dispatcher: Requesting image pull for {image_ref}");
    let mut client = get_image_service_client().await?;

    let response = client
        .pull_image(PullImageRequest {
            image_ref: image_ref.to_string(),
        })
        .await
        .map_err(|status| {
            ContainerServiceError::ImageService(format!(
                "PullImage RPC failed for {image_ref}: {status}"
            ))
        })?;

    let image_uuid = response.into_inner().image_uuid;
    info!("Dispatcher: Image pull for {image_ref} initiated. UUID: {image_uuid}");
    Ok(image_uuid)
}

impl Dispatcher {
    pub async fn new(
        rx: mpsc::Receiver<Command>,
        db_url: &str,
    ) -> Result<Self, ContainerServiceError> {
        info!("Dispatcher: Connecting to persistence layer at {db_url}...");
        let repository = ContainerRepository::connect(db_url).await?;
        info!("Dispatcher: Persistence layer connected successfully.");
        let adapter = Arc::new(ContainerAdapter::new());
        Ok(Self {
            rx,
            repository,
            adapter,
        })
    }

    pub async fn run(mut self) {
        info!("Dispatcher: Running and waiting for commands.");
        while let Some(cmd) = self.rx.recv().await {
            let repo = self.repository.clone();
            let adapter = self.adapter.clone();
            tokio::spawn(async move {
                if let Err(e) = Self::handle_command(cmd, repo, adapter).await {
                    warn!("Dispatcher: Error handling command: {}", e);
                }
            });
        }
        info!("Dispatcher: Channel closed, shutting down.");
    }

    async fn get_container_record(
        repo: &ContainerRepository,
        id_str: &str,
    ) -> Result<ContainerRecord, ContainerServiceError> {
        let container_id = Uuid::parse_str(id_str).map_err(|_| {
            ContainerServiceError::InvalidArgument("Invalid UUID format".to_string())
        })?;

        repo.get_container(container_id).await?.ok_or_else(|| {
            ContainerServiceError::InvalidArgument(format!("Container '{}' not found", id_str))
        })
    }

    async fn handle_command(
        cmd: Command,
        repository: ContainerRepository,
        adapter: Arc<ContainerAdapter>,
    ) -> Result<(), ContainerServiceError> {
        match cmd {
            Command::CreateContainer(req, responder) => {
                let container_id =
                    if let Some(id_str) = req.container_id.as_deref().filter(|s| !s.is_empty()) {
                        Uuid::parse_str(id_str).map_err(|_| {
                            ContainerServiceError::InvalidArgument(
                                "Invalid container_id UUID format.".to_string(),
                            )
                        })?
                    } else {
                        Uuid::new_v4()
                    };

                if repository.get_container(container_id).await?.is_some() {
                    let _ = responder.send(Err(ContainerServiceError::AlreadyExists(
                        container_id.to_string(),
                    )));
                    return Ok(());
                }

                let config = req.config.clone().ok_or_else(|| {
                    ContainerServiceError::InvalidArgument(
                        "ContainerConfig is required".to_string(),
                    )
                })?;
                let image_ref = config.image_ref.clone();

                let image_uuid_str = initiate_image_pull(&image_ref).await?;
                let image_uuid = Uuid::parse_str(&image_uuid_str).map_err(|e| {
                    ContainerServiceError::ImageService(format!("Invalid image UUID: {}", e))
                })?;

                let record = crate::persistence::ContainerRecord {
                    container_id,
                    image_uuid,
                    status: crate::persistence::ContainerStatus {
                        state: ContainerState::PullingImage,
                        process_id: None,
                    },
                    config,
                };
                repository.save_container(&record).await?;

                tokio::spawn(worker::handle_create_container(
                    container_id,
                    image_uuid,
                    image_ref,
                    responder,
                    repository.clone(),
                    adapter.clone(),
                ));
            }
            Command::StartContainer(req, responder) => {
                let record = Self::get_container_record(&repository, &req.container_id).await;
                match record {
                    Ok(rec) if rec.status.state == ContainerState::Created => {
                        tokio::spawn(worker::handle_start_container(
                            req, responder, repository, adapter,
                        ));
                    }
                    Ok(rec) => {
                        let _ = responder.send(Err(ContainerServiceError::InvalidState(format!(
                            "Cannot start container in state {:?}",
                            rec.status.state
                        ))));
                    }
                    Err(e) => {
                        let _ = responder.send(Err(e));
                    }
                }
            }
            Command::StopContainer(req, responder) => {
                let record = Self::get_container_record(&repository, &req.container_id).await;
                match record {
                    Ok(rec) if rec.status.state == ContainerState::Running => {
                        tokio::spawn(worker::handle_stop_container(
                            req, responder, repository, adapter,
                        ));
                    }
                    Ok(rec) => {
                        let _ = responder.send(Err(ContainerServiceError::InvalidState(format!(
                            "Cannot stop container in state {:?}",
                            rec.status.state
                        ))));
                    }
                    Err(e) => {
                        let _ = responder.send(Err(e));
                    }
                }
            }
            Command::DeleteContainer(req, responder) => {
                let record = Self::get_container_record(&repository, &req.container_id).await;
                match record {
                    Ok(rec) if rec.status.state != ContainerState::Running => {
                        tokio::spawn(worker::handle_delete_container(
                            req, responder, repository, adapter,
                        ));
                    }
                    Ok(rec) => {
                        let _ = responder.send(Err(ContainerServiceError::InvalidState(format!(
                            "Cannot delete container in state {:?}. Stop it first.",
                            rec.status.state
                        ))));
                    }
                    Err(e) => {
                        let _ = responder.send(Err(e));
                    }
                }
            }
            Command::GetContainer(req, responder) => {
                let result = Self::get_container_record(&repository, &req.container_id)
                    .await
                    .map(|rec| ContainerInfo {
                        container_id: rec.container_id.to_string(),
                        state: rec.status.state as i32,
                        config: Some(rec.config),
                        pid: rec.status.process_id,
                        exit_code: None, // This would require waiting for the process
                    });
                let _ = responder.send(result);
            }
            Command::ListContainers(_req, responder) => {
                let result = repository
                    .list_all_containers()
                    .await
                    .map(|records| {
                        let containers = records
                            .into_iter()
                            .map(|rec| ContainerInfo {
                                container_id: rec.container_id.to_string(),
                                state: rec.status.state as i32,
                                config: Some(rec.config),
                                pid: rec.status.process_id,
                                exit_code: None,
                            })
                            .collect();
                        ListContainersResponse { containers }
                    })
                    .map_err(ContainerServiceError::Persistence);
                let _ = responder.send(result);
            }
        }
        Ok(())
    }
}
