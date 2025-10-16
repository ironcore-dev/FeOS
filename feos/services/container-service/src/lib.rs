// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use crate::error::ContainerServiceError;
use feos_proto::container_service::{
    ContainerInfo, CreateContainerRequest, CreateContainerResponse, DeleteContainerRequest,
    DeleteContainerResponse, GetContainerRequest, ListContainersRequest, ListContainersResponse,
    StartContainerRequest, StartContainerResponse, StopContainerRequest, StopContainerResponse,
};
use tokio::sync::oneshot;

pub mod api;
pub mod dispatcher;
pub mod error;
pub mod persistence;
pub mod runtime;
pub mod worker;

pub const DEFAULT_CONTAINER_DB_URL: &str = "sqlite:/var/lib/feos/containers.db";

pub enum Command {
    CreateContainer(
        CreateContainerRequest,
        oneshot::Sender<Result<CreateContainerResponse, ContainerServiceError>>,
    ),
    StartContainer(
        StartContainerRequest,
        oneshot::Sender<Result<StartContainerResponse, ContainerServiceError>>,
    ),
    StopContainer(
        StopContainerRequest,
        oneshot::Sender<Result<StopContainerResponse, ContainerServiceError>>,
    ),
    GetContainer(
        GetContainerRequest,
        oneshot::Sender<Result<ContainerInfo, ContainerServiceError>>,
    ),
    ListContainers(
        ListContainersRequest,
        oneshot::Sender<Result<ListContainersResponse, ContainerServiceError>>,
    ),
    DeleteContainer(
        DeleteContainerRequest,
        oneshot::Sender<Result<DeleteContainerResponse, ContainerServiceError>>,
    ),
}

impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Command::CreateContainer(req, _) => {
                f.debug_tuple("CreateContainer").field(req).finish()
            }
            Command::StartContainer(req, _) => f.debug_tuple("StartContainer").field(req).finish(),
            Command::StopContainer(req, _) => f.debug_tuple("StopContainer").field(req).finish(),
            Command::GetContainer(req, _) => f.debug_tuple("GetContainer").field(req).finish(),
            Command::ListContainers(req, _) => f.debug_tuple("ListContainers").field(req).finish(),
            Command::DeleteContainer(req, _) => {
                f.debug_tuple("DeleteContainer").field(req).finish()
            }
        }
    }
}
