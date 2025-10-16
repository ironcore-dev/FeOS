// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    error::ImageServiceError, FileCommand, ImageStateEvent, OrchestratorCommand, PulledImageData,
};
use feos_proto::image_service::{
    DeleteImageResponse, ImageInfo, ImageState, ImageStatusResponse, ListImagesResponse,
    PullImageResponse,
};
use log::{error, info, warn};
use oci_distribution::{client::ClientConfig, manifest, secrets::RegistryAuth, Client, Reference};
use std::collections::HashMap;
use tokio::sync::{broadcast, mpsc, oneshot};
use tonic::Status;
use uuid::Uuid;

const SQUASHFS_MEDIA_TYPE: &str = "application/vnd.ironcore.image.squashfs.v1alpha1.squashfs";
const INITRAMFS_MEDIA_TYPE: &str = "application/vnd.ironcore.image.initramfs.v1alpha1.initramfs";
const VMLINUZ_MEDIA_TYPE: &str = "application/vnd.ironcore.image.vmlinuz.v1alpha1.vmlinuz";
const ROOTFS_MEDIA_TYPE: &str = "application/vnd.ironcore.image.rootfs.v1alpha1.rootfs";

pub struct Orchestrator {
    command_rx: mpsc::Receiver<OrchestratorCommand>,
    command_tx: mpsc::Sender<OrchestratorCommand>,
    broadcast_tx: broadcast::Sender<ImageStateEvent>,
    filestore_tx: mpsc::Sender<FileCommand>,
    store: HashMap<String, ImageInfo>,
}

impl Orchestrator {
    pub fn new(filestore_tx: mpsc::Sender<FileCommand>) -> Self {
        let (command_tx, command_rx) = mpsc::channel(32);
        let (broadcast_tx, _) = broadcast::channel(32);
        Self {
            command_rx,
            command_tx,
            broadcast_tx,
            filestore_tx,
            store: HashMap::new(),
        }
    }

    pub fn get_command_sender(&self) -> mpsc::Sender<OrchestratorCommand> {
        self.command_tx.clone()
    }

    pub async fn run(mut self) {
        let (responder, resp_rx) = oneshot::channel();
        if self
            .filestore_tx
            .send(FileCommand::ScanExistingImages { responder })
            .await
            .is_ok()
        {
            if let Ok(initial_store) = resp_rx.await {
                self.store = initial_store;
            }
        }

        info!("Orchestrator: Running and waiting for commands.");
        while let Some(cmd) = self.command_rx.recv().await {
            self.handle_command(cmd).await;
        }
        info!("Orchestrator: Channel closed, shutting down.");
    }

    async fn handle_command(&mut self, cmd: OrchestratorCommand) {
        match cmd {
            OrchestratorCommand::PullImage {
                image_ref,
                responder,
            } => {
                let image_uuid = Uuid::new_v4().to_string();
                info!("Orchestrator: Start pull for '{image_ref}', assigned UUID {image_uuid}");

                self.store.insert(
                    image_uuid.clone(),
                    ImageInfo {
                        image_uuid: image_uuid.clone(),
                        image_ref: image_ref.clone(),
                        state: ImageState::Downloading as i32,
                    },
                );
                self.broadcast_state_change(
                    image_uuid.clone(),
                    ImageState::Downloading,
                    "Pull initiated".to_string(),
                );

                let _ = responder.send(Ok(PullImageResponse {
                    image_uuid: image_uuid.clone(),
                }));

                tokio::spawn(pull_oci_image(
                    self.command_tx.clone(),
                    image_uuid,
                    image_ref,
                ));
            }
            OrchestratorCommand::FinalizePull {
                image_uuid,
                image_ref,
                image_data,
            } => {
                info!("Orchestrator: Finalizing pull for {image_uuid}");
                let (responder, resp_rx) = oneshot::channel();
                let file_cmd = FileCommand::StoreImage {
                    image_uuid: image_uuid.clone(),
                    image_ref,
                    image_data,
                    responder,
                };

                if self.filestore_tx.send(file_cmd).await.is_err() {
                    let err_msg = "Failed to send StoreImage command to FileStore.";
                    error!("Orchestrator: {err_msg}");
                    self.update_and_broadcast_state(
                        image_uuid,
                        ImageState::PullFailed,
                        err_msg.to_string(),
                    );
                    return;
                }

                match resp_rx.await {
                    Ok(Ok(())) => {
                        info!("Orchestrator: FileStore successfully stored image {image_uuid}");
                        self.update_and_broadcast_state(
                            image_uuid,
                            ImageState::Ready,
                            "Image is ready".to_string(),
                        );
                    }
                    Ok(Err(e)) => {
                        let err_msg = format!("FileStore failed to store image: {e}");
                        error!("Orchestrator: {err_msg} ({image_uuid})");
                        self.update_and_broadcast_state(
                            image_uuid,
                            ImageState::PullFailed,
                            err_msg,
                        );
                    }
                    Err(_) => {
                        let err_msg = "FileStore actor dropped response channel.";
                        error!("Orchestrator: {err_msg} ({image_uuid})");
                        self.update_and_broadcast_state(
                            image_uuid,
                            ImageState::PullFailed,
                            err_msg.to_string(),
                        );
                    }
                }
            }
            OrchestratorCommand::FailPull { image_uuid, error } => {
                let err_msg = format!("Pull failed: {error}");
                error!("Orchestrator: {err_msg} ({image_uuid})");
                self.update_and_broadcast_state(image_uuid, ImageState::PullFailed, err_msg);
            }
            OrchestratorCommand::ListImages { responder } => {
                let images = self.store.values().cloned().collect();
                let _ = responder.send(Ok(ListImagesResponse { images }));
            }
            OrchestratorCommand::DeleteImage {
                image_uuid,
                responder,
            } => {
                info!("Orchestrator: Deleting image {image_uuid}");
                self.store.remove(&image_uuid);

                let (file_resp_tx, file_resp_rx) = oneshot::channel();
                let file_cmd = FileCommand::DeleteImage {
                    image_uuid: image_uuid.clone(),
                    responder: file_resp_tx,
                };

                if self.filestore_tx.send(file_cmd).await.is_err() {
                    error!("Orchestrator: Failed to send DeleteImage command to FileStore.");
                } else if let Ok(Err(e)) = file_resp_rx.await {
                    if e.kind() != std::io::ErrorKind::NotFound {
                        warn!("Orchestrator: FileStore failed to delete {image_uuid}: {e}");
                    }
                }

                self.broadcast_state_change(
                    image_uuid,
                    ImageState::NotFound,
                    "Image deleted".to_string(),
                );
                let _ = responder.send(Ok(DeleteImageResponse {}));
            }
            OrchestratorCommand::WatchImageStatus {
                image_uuid,
                stream_sender,
            } => {
                let initial_state = self
                    .store
                    .get(&image_uuid)
                    .map(|info| ImageState::try_from(info.state).unwrap_or(ImageState::Unspecified))
                    .unwrap_or(ImageState::NotFound);

                tokio::spawn(watch_image_status_stream(
                    image_uuid,
                    initial_state,
                    stream_sender,
                    self.broadcast_tx.subscribe(),
                ));
            }
        }
    }

    fn update_and_broadcast_state(
        &mut self,
        image_uuid: String,
        new_state: ImageState,
        message: String,
    ) {
        if let Some(info) = self.store.get_mut(&image_uuid) {
            info.state = new_state as i32;
        }
        self.broadcast_state_change(image_uuid, new_state, message);
    }

    fn broadcast_state_change(&self, image_uuid: String, state: ImageState, message: String) {
        let event = ImageStateEvent {
            image_uuid,
            state,
            message,
        };
        if self.broadcast_tx.send(event).is_err() {
            info!("Orchestrator: Broadcast failed, no active listeners.");
        }
    }
}

async fn pull_oci_data(image_ref: &str) -> Result<PulledImageData, ImageServiceError> {
    info!("ImagePuller: fetching image: {image_ref}");
    let reference = Reference::try_from(image_ref.to_string())?;

    let accepted_media_types = vec![
        // VM-specific types
        ROOTFS_MEDIA_TYPE,
        SQUASHFS_MEDIA_TYPE,
        INITRAMFS_MEDIA_TYPE,
        VMLINUZ_MEDIA_TYPE,
        // Standard container layer types
        manifest::IMAGE_LAYER_GZIP_MEDIA_TYPE,
        manifest::IMAGE_DOCKER_LAYER_GZIP_MEDIA_TYPE,
    ];

    let config = ClientConfig {
        ..Default::default()
    };
    let client = Client::new(config);
    let auth = &RegistryAuth::Anonymous;

    info!("ImagePuller: pulling manifest and config for {image_ref}");
    let (manifest, _, _) = client.pull_manifest_and_config(&reference, auth).await?;

    let mut config_data = Vec::new();
    client
        .pull_blob(&reference, &manifest.config, &mut config_data)
        .await?;
    info!(
        "ImagePuller: pulled config blob {} bytes",
        config_data.len()
    );

    let mut layers = Vec::new();
    for layer in manifest.layers {
        if !accepted_media_types.contains(&layer.media_type.as_str()) {
            warn!(
                "ImagePuller: skipping layer with unsupported media type: {}",
                layer.media_type
            );
            continue;
        }

        info!(
            "ImagePuller: pulling layer {} ({})",
            layer.digest, layer.media_type
        );

        let mut layer_data = Vec::new();
        client
            .pull_blob(&reference, &layer, &mut layer_data)
            .await?;
        info!("ImagePuller: pulled layer blob {} bytes", layer_data.len());
        layers.push(layer_data);
    }

    if layers.is_empty() {
        return Err(ImageServiceError::MissingLayer(
            "No compatible layers found".to_string(),
        ));
    }

    Ok(PulledImageData {
        config: config_data,
        layers,
    })
}

pub async fn pull_oci_image(
    command_tx: mpsc::Sender<OrchestratorCommand>,
    image_uuid: String,
    image_ref: String,
) {
    match pull_oci_data(&image_ref).await {
        Ok(image_data) => {
            let cmd = OrchestratorCommand::FinalizePull {
                image_uuid,
                image_ref,
                image_data,
            };
            if command_tx.send(cmd).await.is_err() {
                error!("ImagePuller: Failed to send FinalizePull command. Actor may be down.");
            }
        }
        Err(e) => {
            let cmd = OrchestratorCommand::FailPull {
                image_uuid,
                error: e,
            };
            if command_tx.send(cmd).await.is_err() {
                error!("ImagePuller: Failed to send FailPull command. Actor may be down.");
            }
        }
    }
}

pub async fn watch_image_status_stream(
    image_uuid_to_watch: String,
    initial_state: ImageState,
    stream_sender: mpsc::Sender<Result<ImageStatusResponse, Status>>,
    mut broadcast_rx: broadcast::Receiver<ImageStateEvent>,
) {
    info!("ImageWatcher: Starting watch stream for {image_uuid_to_watch}");

    let initial_response = ImageStatusResponse {
        state: initial_state as i32,
        progress_percent: if initial_state == ImageState::Ready {
            100
        } else {
            0
        },
        message: format!("Initial state: {initial_state:?}"),
    };
    if stream_sender.send(Ok(initial_response)).await.is_err() {
        info!(
            "ImageWatcher: Client disconnected before initial state send for {image_uuid_to_watch}"
        );
        return;
    }

    if matches!(
        initial_state,
        ImageState::Ready | ImageState::PullFailed | ImageState::NotFound
    ) {
        info!(
            "ImageWatcher: Closing stream for {image_uuid_to_watch} due to terminal initial state."
        );
        return;
    }

    loop {
        match broadcast_rx.recv().await {
            Ok(event) => {
                if event.image_uuid == image_uuid_to_watch {
                    info!(
                        "ImageWatcher: Got relevant event for {image_uuid_to_watch}: {:?}",
                        event.state
                    );
                    let response = ImageStatusResponse {
                        state: event.state as i32,
                        progress_percent: if event.state == ImageState::Ready {
                            100
                        } else {
                            0
                        },
                        message: event.message,
                    };

                    if stream_sender.send(Ok(response)).await.is_err() {
                        info!("ImageWatcher: Client disconnected. Closing watch for {image_uuid_to_watch}");
                        break;
                    }

                    if matches!(
                        event.state,
                        ImageState::Ready | ImageState::PullFailed | ImageState::NotFound
                    ) {
                        info!("ImageWatcher: Reached terminal state. Closing watch for {image_uuid_to_watch}");
                        break;
                    }
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!("ImageWatcher: Stream for {image_uuid_to_watch} lagged by {n} messages. Continuing.");
            }
            Err(broadcast::error::RecvError::Closed) => {
                info!("ImageWatcher: Broadcast channel closed. Shutting down watch for {image_uuid_to_watch}");
                break;
            }
        }
    }
}
