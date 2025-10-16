// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use crate::Command;
use feos_proto::container_service::{
    container_service_server::ContainerService, ContainerEvent, ContainerInfo,
    CreateContainerRequest, CreateContainerResponse, DeleteContainerRequest,
    DeleteContainerResponse, GetContainerRequest, ListContainersRequest, ListContainersResponse,
    LogEntry, StartContainerRequest, StartContainerResponse, StopContainerRequest,
    StopContainerResponse, StreamContainerEventsRequest, StreamContainerLogsRequest,
};
use log::info;
use std::pin::Pin;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::Stream;
use tonic::{Request, Response, Status};

pub struct ContainerApiHandler {
    dispatcher_tx: mpsc::Sender<Command>,
}

impl ContainerApiHandler {
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
impl ContainerService for ContainerApiHandler {
    type StreamContainerLogsStream = Pin<Box<dyn Stream<Item = Result<LogEntry, Status>> + Send>>;
    type StreamContainerEventsStream =
        Pin<Box<dyn Stream<Item = Result<ContainerEvent, Status>> + Send>>;

    async fn create_container(
        &self,
        request: Request<CreateContainerRequest>,
    ) -> Result<Response<CreateContainerResponse>, Status> {
        info!("ContainerApi: Received CreateContainer request.");
        dispatch_and_wait(&self.dispatcher_tx, |resp_tx| {
            Command::CreateContainer(request.into_inner(), resp_tx)
        })
        .await
    }

    async fn start_container(
        &self,
        request: Request<StartContainerRequest>,
    ) -> Result<Response<StartContainerResponse>, Status> {
        info!("ContainerApi: Received StartContainer request.");
        dispatch_and_wait(&self.dispatcher_tx, |resp_tx| {
            Command::StartContainer(request.into_inner(), resp_tx)
        })
        .await
    }

    async fn stop_container(
        &self,
        request: Request<StopContainerRequest>,
    ) -> Result<Response<StopContainerResponse>, Status> {
        info!("ContainerApi: Received StopContainer request.");
        dispatch_and_wait(&self.dispatcher_tx, |resp_tx| {
            Command::StopContainer(request.into_inner(), resp_tx)
        })
        .await
    }

    async fn get_container(
        &self,
        request: Request<GetContainerRequest>,
    ) -> Result<Response<ContainerInfo>, Status> {
        info!("ContainerApi: Received GetContainer request.");
        dispatch_and_wait(&self.dispatcher_tx, |resp_tx| {
            Command::GetContainer(request.into_inner(), resp_tx)
        })
        .await
    }

    async fn list_containers(
        &self,
        request: Request<ListContainersRequest>,
    ) -> Result<Response<ListContainersResponse>, Status> {
        info!("ContainerApi: Received ListContainers request.");
        dispatch_and_wait(&self.dispatcher_tx, |resp_tx| {
            Command::ListContainers(request.into_inner(), resp_tx)
        })
        .await
    }

    async fn delete_container(
        &self,
        request: Request<DeleteContainerRequest>,
    ) -> Result<Response<DeleteContainerResponse>, Status> {
        info!("ContainerApi: Received DeleteContainer request.");
        dispatch_and_wait(&self.dispatcher_tx, |resp_tx| {
            Command::DeleteContainer(request.into_inner(), resp_tx)
        })
        .await
    }

    async fn stream_container_logs(
        &self,
        _request: Request<StreamContainerLogsRequest>,
    ) -> Result<Response<Self::StreamContainerLogsStream>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }

    async fn stream_container_events(
        &self,
        _request: Request<StreamContainerEventsRequest>,
    ) -> Result<Response<Self::StreamContainerEventsStream>, Status> {
        Err(Status::unimplemented("Not yet implemented"))
    }
}
