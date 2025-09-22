// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use crate::Command;
use feos_proto::vm_service::{
    vm_service_server::VmService, AttachDiskRequest, AttachDiskResponse, CreateVmRequest,
    CreateVmResponse, DeleteVmRequest, DeleteVmResponse, GetVmRequest, ListVmsRequest,
    ListVmsResponse, PauseVmRequest, PauseVmResponse, PingVmRequest, PingVmResponse,
    RemoveDiskRequest, RemoveDiskResponse, ResumeVmRequest, ResumeVmResponse, ShutdownVmRequest,
    ShutdownVmResponse, StartVmRequest, StartVmResponse, StreamVmConsoleRequest,
    StreamVmConsoleResponse, StreamVmEventsRequest, VmEvent, VmInfo,
};
use log::info;
use std::pin::Pin;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::{wrappers::ReceiverStream, Stream};
use tonic::{Request, Response, Status, Streaming};

pub struct VmApiHandler {
    dispatcher_tx: mpsc::Sender<Command>,
}

impl VmApiHandler {
    pub fn new(dispatcher_tx: mpsc::Sender<Command>) -> Self {
        Self { dispatcher_tx }
    }
}

#[tonic::async_trait]
impl VmService for VmApiHandler {
    type StreamVmEventsStream = Pin<Box<dyn Stream<Item = Result<VmEvent, Status>> + Send>>;
    type StreamVmConsoleStream =
        Pin<Box<dyn Stream<Item = Result<StreamVmConsoleResponse, Status>> + Send>>;

    async fn create_vm(
        &self,
        request: Request<CreateVmRequest>,
    ) -> Result<Response<CreateVmResponse>, Status> {
        info!("VmApi: Received CreateVm request.");
        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = Command::CreateVm(request.into_inner(), resp_tx);
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

    async fn start_vm(
        &self,
        request: Request<StartVmRequest>,
    ) -> Result<Response<StartVmResponse>, Status> {
        info!("VmApi: Received StartVm request.");
        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = Command::StartVm(request.into_inner(), resp_tx);
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

    async fn get_vm(&self, request: Request<GetVmRequest>) -> Result<Response<VmInfo>, Status> {
        info!("VmApi: Received GetVm request.");
        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = Command::GetVm(request.into_inner(), resp_tx);
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

    async fn stream_vm_events(
        &self,
        request: Request<StreamVmEventsRequest>,
    ) -> Result<Response<Self::StreamVmEventsStream>, Status> {
        info!("VmApi: Received StreamVmEvents stream request.");
        let (stream_tx, stream_rx) = mpsc::channel(16);
        let cmd = Command::StreamVmEvents(request.into_inner(), stream_tx);
        self.dispatcher_tx
            .send(cmd)
            .await
            .map_err(|e| Status::internal(format!("Failed to send command to dispatcher: {e}")))?;
        let output_stream = ReceiverStream::new(stream_rx);
        Ok(Response::new(Box::pin(output_stream)))
    }

    async fn delete_vm(
        &self,
        request: Request<DeleteVmRequest>,
    ) -> Result<Response<DeleteVmResponse>, Status> {
        info!("VmApi: Received DeleteVm request.");
        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = Command::DeleteVm(request.into_inner(), resp_tx);
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

    async fn stream_vm_console(
        &self,
        request: Request<Streaming<StreamVmConsoleRequest>>,
    ) -> Result<Response<Self::StreamVmConsoleStream>, Status> {
        info!("VmApi: Received StreamVmConsole stream request.");
        let grpc_input_stream = request.into_inner();
        let (grpc_output_tx, grpc_output_rx) = mpsc::channel(32);
        let cmd = Command::StreamVmConsole(Box::new(grpc_input_stream), grpc_output_tx);
        self.dispatcher_tx
            .send(cmd)
            .await
            .map_err(|e| Status::internal(format!("Failed to send command to dispatcher: {e}")))?;
        let output_stream = ReceiverStream::new(grpc_output_rx);
        Ok(Response::new(Box::pin(output_stream)))
    }

    async fn list_vms(
        &self,
        request: Request<ListVmsRequest>,
    ) -> Result<Response<ListVmsResponse>, Status> {
        info!("VmApi: Received ListVms request.");
        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = Command::ListVms(request.into_inner(), resp_tx);
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

    async fn ping_vm(
        &self,
        request: Request<PingVmRequest>,
    ) -> Result<Response<PingVmResponse>, Status> {
        info!("VmApi: Received PingVm request.");
        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = Command::PingVm(request.into_inner(), resp_tx);
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

    async fn shutdown_vm(
        &self,
        request: Request<ShutdownVmRequest>,
    ) -> Result<Response<ShutdownVmResponse>, Status> {
        info!("VmApi: Received ShutdownVm request.");
        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = Command::ShutdownVm(request.into_inner(), resp_tx);
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

    async fn pause_vm(
        &self,
        request: Request<PauseVmRequest>,
    ) -> Result<Response<PauseVmResponse>, Status> {
        info!("VmApi: Received PauseVm request.");
        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = Command::PauseVm(request.into_inner(), resp_tx);
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

    async fn resume_vm(
        &self,
        request: Request<ResumeVmRequest>,
    ) -> Result<Response<ResumeVmResponse>, Status> {
        info!("VmApi: Received ResumeVm request.");
        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = Command::ResumeVm(request.into_inner(), resp_tx);
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

    async fn attach_disk(
        &self,
        request: Request<AttachDiskRequest>,
    ) -> Result<Response<AttachDiskResponse>, Status> {
        info!("VmApi: Received AttachDisk request.");
        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = Command::AttachDisk(request.into_inner(), resp_tx);
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

    async fn remove_disk(
        &self,
        request: Request<RemoveDiskRequest>,
    ) -> Result<Response<RemoveDiskResponse>, Status> {
        info!("VmApi: Received RemoveDisk request.");
        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = Command::RemoveDisk(request.into_inner(), resp_tx);
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
