use proto_definitions::image_service::{
    DeleteImageRequest, DeleteImageResponse, ImageInfo, ImageState, ImageStatusResponse,
    ListImagesRequest, ListImagesResponse, PullImageRequest, PullImageResponse,
    WatchImageStatusRequest,
};
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};
use tonic::Status;

pub mod api;
pub mod dispatcher;
pub mod filestore;
pub mod worker;

pub const IMAGE_DIR: &str = "/tmp/images";
pub const IMAGE_SERVICE_SOCKET: &str = "/tmp/image_service.sock";

#[derive(Debug, Clone)]
pub struct ImageStateEvent {
    pub image_uuid: String,
    pub state: ImageState,
}

#[derive(Debug)]
pub enum Command {
    PullImage(
        PullImageRequest,
        oneshot::Sender<Result<PullImageResponse, Status>>,
    ),
    WatchImageStatus(
        WatchImageStatusRequest,
        mpsc::Sender<Result<ImageStatusResponse, Status>>,
    ),
    ListImages(
        ListImagesRequest,
        oneshot::Sender<Result<ListImagesResponse, Status>>,
    ),
    DeleteImage(
        DeleteImageRequest,
        oneshot::Sender<Result<DeleteImageResponse, Status>>,
    ),
}

#[derive(Debug)]
pub enum OrchestratorCommand {
    PullImage {
        image_ref: String,
        responder: oneshot::Sender<Result<PullImageResponse, Status>>,
    },
    FinalizePull {
        image_uuid: String,
        image_ref: String,
        image_data: Vec<u8>,
    },
    FailPull {
        image_uuid: String,
        error: String,
    },
    WatchImageStatus {
        image_uuid: String,
        stream_sender: mpsc::Sender<Result<ImageStatusResponse, Status>>,
    },
    ListImages {
        responder: oneshot::Sender<Result<ListImagesResponse, Status>>,
    },
    DeleteImage {
        image_uuid: String,
        responder: oneshot::Sender<Result<DeleteImageResponse, Status>>,
    },
}

#[derive(Debug)]
pub enum FileCommand {
    StoreImage {
        image_uuid: String,
        image_ref: String,
        image_data: Vec<u8>,
        responder: oneshot::Sender<Result<(), std::io::Error>>,
    },
    DeleteImage {
        image_uuid: String,
        responder: oneshot::Sender<Result<(), std::io::Error>>,
    },
    ScanExistingImages {
        responder: oneshot::Sender<HashMap<String, ImageInfo>>,
    },
}
