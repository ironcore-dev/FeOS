use crate::Command;
use feos_proto::host_service::{
    host_service_server::HostService, FeosLogEntry, GetCpuInfoRequest, GetCpuInfoResponse,
    GetNetworkInfoRequest, GetNetworkInfoResponse, HostnameRequest, HostnameResponse,
    KernelLogEntry, MemoryRequest, MemoryResponse, RebootRequest, RebootResponse, ShutdownRequest,
    ShutdownResponse, StreamFeosLogsRequest, StreamKernelLogsRequest, UpgradeRequest,
    UpgradeResponse,
};
use log::info;
use std::pin::Pin;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::{wrappers::ReceiverStream, Stream};
use tonic::{Request, Response, Status, Streaming};

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
        info!("HOST_API_HANDLER: Received Hostname request.");
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
        info!("HOST_API_HANDLER: Received GetMemory request.");
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
        info!("HOST_API_HANDLER: Received GetCPUInfo request.");
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
        info!("HOST_API_HANDLER: Received GetNetworkInfo request.");
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
        info!("HOST_API_HANDLER: Received Shutdown request.");
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
        info!("HOST_API_HANDLER: Received Reboot request.");
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
        request: Request<Streaming<UpgradeRequest>>,
    ) -> Result<Response<UpgradeResponse>, Status> {
        info!("HOST_API_HANDLER: Received UpgradeFeosBinary request.");
        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = Command::UpgradeFeosBinary(Box::new(request.into_inner()), resp_tx);
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
        info!("HOST_API_HANDLER: Received StreamKernelLogs request.");
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
        info!("HOST_API_HANDLER: Received StreamFeOSLogs request.");
        let (stream_tx, stream_rx) = mpsc::channel(128);
        let cmd = Command::StreamFeOSLogs(stream_tx);
        self.dispatcher_tx
            .send(cmd)
            .await
            .map_err(|e| Status::internal(format!("Failed to send command to dispatcher: {e}")))?;
        let output_stream = ReceiverStream::new(stream_rx);
        Ok(Response::new(Box::pin(output_stream)))
    }
}
