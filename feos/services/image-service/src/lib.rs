// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use crate::error::ImageServiceError;
use feos_proto::image_service::{
    DeleteImageRequest, DeleteImageResponse, ImageInfo, ImageState, ImageStatusResponse,
    ListImagesRequest, ListImagesResponse, PullImageRequest, PullImageResponse,
    WatchImageStatusRequest,
};
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};
use tonic::Status;
pub mod api;
pub mod dispatcher;
pub mod error;
pub mod filestore;
pub mod worker;

pub const IMAGE_DIR: &str = "/var/lib/feos/images";
pub const IMAGE_SERVICE_SOCKET: &str = "/var/lib/feos/image_service.sock";

#[derive(Debug, Clone)]
pub struct ImageStateEvent {
    pub image_uuid: String,
    pub state: ImageState,
    pub message: String,
}

#[derive(Debug)]
pub enum Command {
    PullImage(
        PullImageRequest,
        oneshot::Sender<Result<PullImageResponse, ImageServiceError>>,
    ),
    WatchImageStatus(
        WatchImageStatusRequest,
        mpsc::Sender<Result<ImageStatusResponse, Status>>,
    ),
    ListImages(
        ListImagesRequest,
        oneshot::Sender<Result<ListImagesResponse, ImageServiceError>>,
    ),
    DeleteImage(
        DeleteImageRequest,
        oneshot::Sender<Result<DeleteImageResponse, ImageServiceError>>,
    ),
}

#[derive(Debug)]
pub struct PulledLayer {
    pub media_type: String,
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct PulledImageData {
    pub config: Vec<u8>,
    pub layers: Vec<PulledLayer>,
}

#[derive(Debug)]
pub enum OrchestratorCommand {
    PullImage {
        image_ref: String,
        responder: oneshot::Sender<Result<PullImageResponse, ImageServiceError>>,
    },
    FinalizePull {
        image_uuid: String,
        image_ref: String,
        image_data: PulledImageData,
    },
    FailPull {
        image_uuid: String,
        error: ImageServiceError,
    },
    WatchImageStatus {
        image_uuid: String,
        stream_sender: mpsc::Sender<Result<ImageStatusResponse, Status>>,
    },
    ListImages {
        responder: oneshot::Sender<Result<ListImagesResponse, ImageServiceError>>,
    },
    DeleteImage {
        image_uuid: String,
        responder: oneshot::Sender<Result<DeleteImageResponse, ImageServiceError>>,
    },
}

#[derive(Debug)]
pub enum FileCommand {
    StoreImage {
        image_uuid: String,
        image_ref: String,
        image_data: PulledImageData,
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
