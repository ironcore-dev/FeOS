use crate::error::TaskError;
use tokio::sync::oneshot;

pub mod api;
pub mod dispatcher;
pub mod error;
pub mod worker;

pub use feos_proto::task_service::{
    CreateRequest, CreateResponse, DeleteRequest, DeleteResponse, KillRequest, KillResponse,
    StartRequest, StartResponse, WaitRequest, WaitResponse,
};

pub const TASK_SERVICE_SOCKET: &str = "/tmp/feos/task_service.sock";

#[derive(Debug)]
pub struct Container {
    pub status: Status,
    pub pid: Option<i32>,
    pub bundle_path: String,
    pub exit_code: Option<i32>,
    pub wait_responder: Option<oneshot::Sender<Result<WaitResponse, TaskError>>>,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Status {
    Creating,
    Created,
    Running,
    Stopped,
}

#[derive(Debug)]
pub enum Command {
    Create {
        req: CreateRequest,
        responder: oneshot::Sender<Result<CreateResponse, TaskError>>,
    },
    Start {
        req: StartRequest,
        responder: oneshot::Sender<Result<StartResponse, TaskError>>,
    },
    Kill {
        req: KillRequest,
        responder: oneshot::Sender<Result<KillResponse, TaskError>>,
    },
    Delete {
        req: DeleteRequest,
        responder: oneshot::Sender<Result<DeleteResponse, TaskError>>,
    },
    Wait {
        req: WaitRequest,
        responder: oneshot::Sender<Result<WaitResponse, TaskError>>,
    },
}

#[derive(Debug)]
pub enum Event {
    ContainerCreated { id: String, pid: i32 },
    ContainerCreateFailed { id: String, error: TaskError },
    ContainerStarted { id: String },
    ContainerStartFailed { id: String, error: TaskError },
    ContainerStopped { id: String, exit_code: i32 },
    ContainerDeleted { id: String },
}
