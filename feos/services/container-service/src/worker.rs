// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    error::ContainerServiceError, persistence::repository::ContainerRepository,
    runtime::adapter::ContainerAdapter,
};
use feos_proto::{
    container_service::{
        ContainerState, CreateContainerResponse, DeleteContainerRequest, DeleteContainerResponse,
        StartContainerRequest, StartContainerResponse, StopContainerRequest, StopContainerResponse,
    },
    image_service::{
        image_service_client::ImageServiceClient, ImageState as OciImageState,
        WatchImageStatusRequest,
    },
};
use hyper_util::rt::TokioIo;
use image_service::IMAGE_SERVICE_SOCKET;
use log::{error, info, warn};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::oneshot;
use tokio_stream::StreamExt;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;
use uuid::Uuid;

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

async fn wait_for_image_ready(
    image_uuid: &str,
    image_ref: &str,
) -> Result<(), ContainerServiceError> {
    let mut client = get_image_service_client()
        .await
        .map_err(|e| ContainerServiceError::ImageService(format!("Failed to connect: {e}")))?;

    let mut stream = client
        .watch_image_status(WatchImageStatusRequest {
            image_uuid: image_uuid.to_string(),
        })
        .await
        .map_err(|e| {
            ContainerServiceError::ImageService(format!(
                "WatchImageStatus RPC failed for {image_uuid}: {e}"
            ))
        })?
        .into_inner();

    while let Some(status_res) = stream.next().await {
        let status = status_res.map_err(|e| {
            ContainerServiceError::ImageService(format!("Image stream error for {image_uuid}: {e}"))
        })?;
        let state = OciImageState::try_from(status.state).unwrap_or(OciImageState::Unspecified);
        match state {
            OciImageState::Ready => return Ok(()),
            OciImageState::PullFailed => {
                return Err(ContainerServiceError::ImageService(format!(
                    "Image pull failed for {image_ref} (uuid: {image_uuid}): {}",
                    status.message
                )))
            }
            _ => continue,
        }
    }
    Err(ContainerServiceError::ImageService(format!(
        "Image watch stream for {image_uuid} ended before reaching a terminal state."
    )))
}

pub async fn handle_create_container(
    container_id: Uuid,
    image_uuid: Uuid,
    image_ref: String,
    responder: oneshot::Sender<Result<CreateContainerResponse, ContainerServiceError>>,
    repository: ContainerRepository,
    adapter: Arc<ContainerAdapter>,
) {
    if responder
        .send(Ok(CreateContainerResponse {
            container_id: container_id.to_string(),
        }))
        .is_err()
    {
        error!("ContainerWorker ({container_id}): Client disconnected before immediate response could be sent. Aborting creation.");
        if let Err(e) = repository.delete_container(container_id).await {
            warn!(
                "Failed to cleanup initial DB record for aborted creation of {container_id}: {e}"
            );
        }
        return;
    }

    info!("ContainerWorker ({container_id}): Waiting for image '{image_ref}' (uuid: {image_uuid}) to be ready...");

    if let Err(e) = wait_for_image_ready(&image_uuid.to_string(), &image_ref).await {
        let error_msg = e.to_string();
        error!("ContainerWorker ({container_id}): {error_msg}");
        if let Err(e) = repository.delete_container(container_id).await {
            warn!("Failed to cleanup DB record for failed creation of {container_id}: {e}");
        }
        return;
    }
    info!("ContainerWorker ({container_id}): Image is ready.");

    let bundle_path = PathBuf::from(image_service::IMAGE_DIR).join(image_uuid.to_string());

    match adapter
        .create_container(&container_id.to_string(), &bundle_path)
        .await
    {
        Ok(pid) => {
            info!("ContainerWorker ({container_id}): Container created successfully by runtime with PID {pid}.");
            if let Err(e) = repository.update_container_pid(container_id, pid).await {
                error!("ContainerWorker ({container_id}): Failed to update PID in DB: {e}");
            }
            if let Err(e) = repository
                .update_container_state(container_id, ContainerState::Created)
                .await
            {
                error!("ContainerWorker ({container_id}): Failed to update state to CREATED in DB: {e}");
            }
        }
        Err(e) => {
            let error_msg = format!("Adapter failed to create container: {}", e);
            error!("ContainerWorker ({container_id}): {error_msg}");
            if let Err(e) = repository.delete_container(container_id).await {
                warn!("Failed to cleanup DB record for failed creation of {container_id}: {e}");
            }
        }
    }
}

pub async fn handle_start_container(
    req: StartContainerRequest,
    responder: oneshot::Sender<Result<StartContainerResponse, ContainerServiceError>>,
    repository: ContainerRepository,
    adapter: Arc<ContainerAdapter>,
) {
    let id_str = req.container_id.clone();
    let result = adapter.start_container(&id_str).await;

    match result {
        Ok(_) => {
            info!("Worker: Start command sent for container {}", id_str);
            let container_id = Uuid::parse_str(&id_str).unwrap();
            if let Err(e) = repository
                .update_container_state(container_id, ContainerState::Running)
                .await
            {
                let err = ContainerServiceError::Persistence(e);
                error!("Worker: {}", err);
                let _ = responder.send(Err(err));
                return;
            }
            let _ = responder.send(Ok(StartContainerResponse {}));
        }
        Err(e) => {
            let err = ContainerServiceError::Adapter(e.to_string());
            error!("Worker: {}", err);
            let _ = responder.send(Err(err));
        }
    }
}

pub async fn handle_stop_container(
    req: StopContainerRequest,
    responder: oneshot::Sender<Result<StopContainerResponse, ContainerServiceError>>,
    repository: ContainerRepository,
    adapter: Arc<ContainerAdapter>,
) {
    let id_str = req.container_id.clone();
    let signal = req.signal.unwrap_or(9);
    let result = adapter.stop_container(&id_str, signal).await;

    match result {
        Ok(_) => {
            info!("Worker: Stop command sent for container {}", id_str);
            let container_id = Uuid::parse_str(&id_str).unwrap();
            if let Err(e) = repository
                .update_container_state(container_id, ContainerState::Stopped)
                .await
            {
                let err = ContainerServiceError::Persistence(e);
                error!("Worker: {}", err);
                let _ = responder.send(Err(err));
                return;
            }
            let _ = responder.send(Ok(StopContainerResponse {}));
        }
        Err(e) => {
            let err = ContainerServiceError::Adapter(e.to_string());
            error!("Worker: {}", err);
            let _ = responder.send(Err(err));
        }
    }
}

pub async fn handle_delete_container(
    req: DeleteContainerRequest,
    responder: oneshot::Sender<Result<DeleteContainerResponse, ContainerServiceError>>,
    repository: ContainerRepository,
    adapter: Arc<ContainerAdapter>,
) {
    let id_str = req.container_id.clone();
    let result = adapter.delete_container(&id_str).await;

    match result {
        Ok(_) => {
            info!("Worker: Delete command sent for container {}", id_str);
            let container_id = Uuid::parse_str(&id_str).unwrap();
            if let Err(e) = repository.delete_container(container_id).await {
                let err = ContainerServiceError::Persistence(e);
                error!("Worker: {}", err);
                let _ = responder.send(Err(err));
                return;
            }
            let _ = responder.send(Ok(DeleteContainerResponse {}));
        }
        Err(e) => {
            let err = ContainerServiceError::Adapter(e.to_string());
            error!("Worker: {}", err);
            let _ = responder.send(Err(err));
        }
    }
}
