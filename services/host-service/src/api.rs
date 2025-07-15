use crate::Command;
use log::info;
use proto_definitions::host_service::{
    host_service_server::HostService, Empty, HostnameResponse, UpgradeRequest, UpgradeResponse,
};
use tokio::sync::{mpsc, oneshot};
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
    async fn hostname(
        &self,
        _request: Request<Empty>,
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
}
