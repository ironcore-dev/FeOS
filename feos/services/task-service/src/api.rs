use crate::error::TaskError;
use crate::Command;
use feos_proto::task_service::{
    task_service_server::TaskService, CreateRequest, CreateResponse, DeleteRequest, DeleteResponse,
    KillRequest, KillResponse, StartRequest, StartResponse, WaitRequest, WaitResponse,
};
use log::info;
use tokio::sync::{mpsc, oneshot};
use tonic::{Request, Response, Status};

pub struct TaskApiHandler {
    dispatcher_tx: mpsc::Sender<Command>,
}

impl TaskApiHandler {
    pub fn new(dispatcher_tx: mpsc::Sender<Command>) -> Self {
        Self { dispatcher_tx }
    }
}

/// Helper function to create a command, send it to the dispatcher, and await the response.
async fn dispatch_and_wait<T, F>(
    dispatcher: &mpsc::Sender<Command>,
    command_constructor: F,
) -> Result<Response<T>, Status>
where
    F: FnOnce(oneshot::Sender<Result<T, TaskError>>) -> Command,
{
    let (resp_tx, resp_rx) = oneshot::channel();
    let cmd = command_constructor(resp_tx);

    dispatcher
        .send(cmd)
        .await
        .map_err(|e| Status::internal(format!("Failed to send command to dispatcher: {}", e)))?;

    match resp_rx.await {
        Ok(Ok(result)) => Ok(Response::new(result)),
        Ok(Err(e)) => Err(e.into()),
        Err(_) => Err(Status::internal(
            "Dispatcher task dropped response channel.",
        )),
    }
}

#[tonic::async_trait]
impl TaskService for TaskApiHandler {
    async fn create(
        &self,
        request: Request<CreateRequest>,
    ) -> Result<Response<CreateResponse>, Status> {
        info!(
            "API: Received Create request for {}",
            request.get_ref().container_id
        );
        dispatch_and_wait(&self.dispatcher_tx, |responder| Command::Create {
            req: request.into_inner(),
            responder,
        })
        .await
    }

    async fn start(
        &self,
        request: Request<StartRequest>,
    ) -> Result<Response<StartResponse>, Status> {
        info!(
            "API: Received Start request for {}",
            request.get_ref().container_id
        );
        dispatch_and_wait(&self.dispatcher_tx, |responder| Command::Start {
            req: request.into_inner(),
            responder,
        })
        .await
    }

    async fn kill(&self, request: Request<KillRequest>) -> Result<Response<KillResponse>, Status> {
        info!(
            "API: Received Kill request for {}",
            request.get_ref().container_id
        );
        dispatch_and_wait(&self.dispatcher_tx, |responder| Command::Kill {
            req: request.into_inner(),
            responder,
        })
        .await
    }

    async fn delete(
        &self,
        request: Request<DeleteRequest>,
    ) -> Result<Response<DeleteResponse>, Status> {
        info!(
            "API: Received Delete request for {}",
            request.get_ref().container_id
        );
        dispatch_and_wait(&self.dispatcher_tx, |responder| Command::Delete {
            req: request.into_inner(),
            responder,
        })
        .await
    }

    async fn wait(&self, request: Request<WaitRequest>) -> Result<Response<WaitResponse>, Status> {
        info!(
            "API: Received Wait request for {}",
            request.get_ref().container_id
        );
        dispatch_and_wait(&self.dispatcher_tx, |responder| Command::Wait {
            req: request.into_inner(),
            responder,
        })
        .await
    }
}
