use crate::Command;
use log::info;
use proto_definitions::image_service::{
    image_service_server::ImageService, DeleteImageRequest, Empty, ImageStatusResponse,
    ListImagesRequest, ListImagesResponse, PullImageRequest, PullImageResponse,
    WatchImageStatusRequest,
};
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

#[tonic::async_trait]
impl ImageService for ImageApiHandler {
    type WatchImageStatusStream =
        Pin<Box<dyn Stream<Item = Result<ImageStatusResponse, Status>> + Send>>;

    async fn pull_image(
        &self,
        request: Request<PullImageRequest>,
    ) -> Result<Response<PullImageResponse>, Status> {
        info!("IMAGE_API_HANDLER: Received PullImage request.");
        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = Command::PullImage(request.into_inner(), resp_tx);
        self.dispatcher_tx
            .send(cmd)
            .await
            .map_err(|e| Status::internal(format!("Failed to send command to dispatcher: {e}")))?;

        match resp_rx.await {
            Ok(Ok(result)) => Ok(Response::new(result)),
            Ok(Err(status)) => Err(status),
            Err(_) => Err(Status::internal(
                "Dispatcher task dropped response channel.",
            )),
        }
    }

    async fn watch_image_status(
        &self,
        request: Request<WatchImageStatusRequest>,
    ) -> Result<Response<Self::WatchImageStatusStream>, Status> {
        info!("IMAGE_API_HANDLER: Received WatchImageStatus stream request.");
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
        info!("IMAGE_API_HANDLER: Received ListImages request.");
        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = Command::ListImages(request.into_inner(), resp_tx);
        self.dispatcher_tx
            .send(cmd)
            .await
            .map_err(|e| Status::internal(format!("Failed to send command to dispatcher: {e}")))?;

        match resp_rx.await {
            Ok(Ok(result)) => Ok(Response::new(result)),
            Ok(Err(status)) => Err(status),
            Err(_) => Err(Status::internal(
                "Dispatcher task dropped response channel.",
            )),
        }
    }

    async fn delete_image(
        &self,
        request: Request<DeleteImageRequest>,
    ) -> Result<Response<Empty>, Status> {
        info!("IMAGE_API_HANDLER: Received DeleteImage request.");
        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = Command::DeleteImage(request.into_inner(), resp_tx);
        self.dispatcher_tx
            .send(cmd)
            .await
            .map_err(|e| Status::internal(format!("Failed to send command to dispatcher: {e}")))?;

        match resp_rx.await {
            Ok(Ok(result)) => Ok(Response::new(result)),
            Ok(Err(status)) => Err(status),
            Err(_) => Err(Status::internal(
                "Dispatcher task dropped response channel.",
            )),
        }
    }
}
