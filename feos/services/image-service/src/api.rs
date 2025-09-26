// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use crate::Command;
use feos_proto::image_service::{
    image_service_server::ImageService, DeleteImageRequest, DeleteImageResponse,
    ImageStatusResponse, ListImagesRequest, ListImagesResponse, PullImageRequest,
    PullImageResponse, WatchImageStatusRequest,
};
use log::info;
use std::pin::Pin;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::{wrappers::ReceiverStream, Stream};
use tonic::{Request, Response, Status};

pub struct ImageApiHandler {
    dispatcher_tx: mpsc::Sender<Command>,
}

impl ImageApiHandler {
    pub fn new(dispatcher_tx: mpsc::Sender<Command>) -> Self {
        Self { dispatcher_tx }
    }
}

async fn dispatch_and_wait<T, E>(
    dispatcher: &mpsc::Sender<Command>,
    command_constructor: impl FnOnce(oneshot::Sender<Result<T, E>>) -> Command,
) -> Result<Response<T>, Status>
where
    E: Into<Status>,
{
    let (resp_tx, resp_rx) = oneshot::channel();
    let cmd = command_constructor(resp_tx);

    dispatcher
        .send(cmd)
        .await
        .map_err(|e| Status::internal(format!("Failed to send command to dispatcher: {e}")))?;

    match resp_rx.await {
        Ok(Ok(result)) => Ok(Response::new(result)),
        Ok(Err(e)) => Err(e.into()),
        Err(_) => Err(Status::internal(
            "Dispatcher task dropped response channel.",
        )),
    }
}

#[tonic::async_trait]
impl ImageService for ImageApiHandler {
    type WatchImageStatusStream =
        Pin<Box<dyn Stream<Item = Result<ImageStatusResponse, Status>> + Send>>;

    async fn pull_image(
        &self,
        request: Request<PullImageRequest>,
    ) -> Result<Response<PullImageResponse>, Status> {
        info!("ImageApi: Received PullImage request.");
        dispatch_and_wait(&self.dispatcher_tx, |resp_tx| {
            Command::PullImage(request.into_inner(), resp_tx)
        })
        .await
    }

    async fn watch_image_status(
        &self,
        request: Request<WatchImageStatusRequest>,
    ) -> Result<Response<Self::WatchImageStatusStream>, Status> {
        info!("ImageApi: Received WatchImageStatus stream request.");
        let (stream_tx, stream_rx) = mpsc::channel(16);
        let cmd = Command::WatchImageStatus(request.into_inner(), stream_tx);
        self.dispatcher_tx
            .send(cmd)
            .await
            .map_err(|e| Status::internal(format!("Failed to send command to dispatcher: {e}")))?;
        let output_stream = ReceiverStream::new(stream_rx);
        Ok(Response::new(Box::pin(output_stream)))
    }

    async fn list_images(
        &self,
        request: Request<ListImagesRequest>,
    ) -> Result<Response<ListImagesResponse>, Status> {
        info!("ImageApi: Received ListImages request.");
        dispatch_and_wait(&self.dispatcher_tx, |resp_tx| {
            Command::ListImages(request.into_inner(), resp_tx)
        })
        .await
    }

    async fn delete_image(
        &self,
        request: Request<DeleteImageRequest>,
    ) -> Result<Response<DeleteImageResponse>, Status> {
        info!("ImageApi: Received DeleteImage request.");
        dispatch_and_wait(&self.dispatcher_tx, |resp_tx| {
            Command::DeleteImage(request.into_inner(), resp_tx)
        })
        .await
    }
}
