// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use crate::Command;
use feos_proto::host_service::{
    host_service_server::HostService, FeosLogEntry, GetCpuInfoRequest, GetCpuInfoResponse,
    GetNetworkInfoRequest, GetNetworkInfoResponse, GetVersionInfoRequest, GetVersionInfoResponse,
    HostnameRequest, HostnameResponse, KernelLogEntry, MemoryRequest, MemoryResponse,
    RebootRequest, RebootResponse, ShutdownRequest, ShutdownResponse, StreamFeosLogsRequest,
    StreamKernelLogsRequest, UpgradeFeosBinaryRequest, UpgradeFeosBinaryResponse,
};
use log::info;
use std::pin::Pin;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::{wrappers::ReceiverStream, Stream};
use tonic::{Request, Response, Status};

pub struct HostApiHandler {
    dispatcher_tx: mpsc::Sender<Command>,
}

impl HostApiHandler {
    pub fn new(dispatcher_tx: mpsc::Sender<Command>) -> Self {
        Self { dispatcher_tx }
    }
}

#[tonic::async_trait]
impl HostService for HostApiHandler {
    type StreamKernelLogsStream =
        Pin<Box<dyn Stream<Item = Result<KernelLogEntry, Status>> + Send>>;
    type StreamFeOSLogsStream = Pin<Box<dyn Stream<Item = Result<FeosLogEntry, Status>> + Send>>;

    async fn hostname(
        &self,
        _request: Request<HostnameRequest>,
    ) -> Result<Response<HostnameResponse>, Status> {
        info!("HostApi: Received Hostname request.");
        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = Command::GetHostname(resp_tx);
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

    async fn get_memory(
        &self,
        _request: Request<MemoryRequest>,
    ) -> Result<Response<MemoryResponse>, Status> {
        info!("HostApi: Received GetMemory request.");
        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = Command::GetMemory(resp_tx);
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

    async fn get_cpu_info(
        &self,
        _request: Request<GetCpuInfoRequest>,
    ) -> Result<Response<GetCpuInfoResponse>, Status> {
        info!("HostApi: Received GetCPUInfo request.");
        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = Command::GetCPUInfo(resp_tx);
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

    async fn get_network_info(
        &self,
        _request: Request<GetNetworkInfoRequest>,
    ) -> Result<Response<GetNetworkInfoResponse>, Status> {
        info!("HostApi: Received GetNetworkInfo request.");
        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = Command::GetNetworkInfo(resp_tx);
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

    async fn shutdown(
        &self,
        request: Request<ShutdownRequest>,
    ) -> Result<Response<ShutdownResponse>, Status> {
        info!("HostApi: Received Shutdown request.");
        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = Command::Shutdown(request.into_inner(), resp_tx);
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

    async fn reboot(
        &self,
        request: Request<RebootRequest>,
    ) -> Result<Response<RebootResponse>, Status> {
        info!("HostApi: Received Reboot request.");
        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = Command::Reboot(request.into_inner(), resp_tx);
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

    async fn upgrade_feos_binary(
        &self,
        request: Request<UpgradeFeosBinaryRequest>,
    ) -> Result<Response<UpgradeFeosBinaryResponse>, Status> {
        info!("HostApi: Received UpgradeFeosBinary request.");
        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = Command::UpgradeFeosBinary(request.into_inner(), resp_tx);
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

    async fn stream_kernel_logs(
        &self,
        _request: Request<StreamKernelLogsRequest>,
    ) -> Result<Response<Self::StreamKernelLogsStream>, Status> {
        info!("HostApi: Received StreamKernelLogs request.");
        let (stream_tx, stream_rx) = mpsc::channel(128);
        let cmd = Command::StreamKernelLogs(stream_tx);
        self.dispatcher_tx
            .send(cmd)
            .await
            .map_err(|e| Status::internal(format!("Failed to send command to dispatcher: {e}")))?;
        let output_stream = ReceiverStream::new(stream_rx);
        Ok(Response::new(Box::pin(output_stream)))
    }

    async fn stream_fe_os_logs(
        &self,
        _request: Request<StreamFeosLogsRequest>,
    ) -> Result<Response<Self::StreamFeOSLogsStream>, Status> {
        info!("HostApi: Received StreamFeOSLogs request.");
        let (stream_tx, stream_rx) = mpsc::channel(128);
        let cmd = Command::StreamFeOSLogs(stream_tx);
        self.dispatcher_tx
            .send(cmd)
            .await
            .map_err(|e| Status::internal(format!("Failed to send command to dispatcher: {e}")))?;
        let output_stream = ReceiverStream::new(stream_rx);
        Ok(Response::new(Box::pin(output_stream)))
    }

    async fn get_version_info(
        &self,
        _request: Request<GetVersionInfoRequest>,
    ) -> Result<Response<GetVersionInfoResponse>, Status> {
        info!("HostApi: Received GetVersionInfo request.");
        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = Command::GetVersionInfo(resp_tx);
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
