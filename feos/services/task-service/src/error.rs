use thiserror::Error;
use tonic::Status;

#[derive(Debug, Error, Clone)]
pub enum TaskError {
    #[error("Container with ID '{0}' not found")]
    ContainerNotFound(String),

    #[error("Container with ID '{0}' already exists")]
    ContainerAlreadyExists(String),

    #[error("Invalid state for operation on container '{id}': current state is {current_state:?}, required one of {required_states:?}")]
    InvalidState {
        id: String,
        current_state: crate::Status,
        required_states: Vec<crate::Status>,
    },

    #[error("Youki command failed: {0}")]
    YoukiCommand(String),

    #[error("An internal error occurred: {0}")]
    Internal(String),

    #[error("I/O error: {0}")]
    Io(String),
}

impl From<std::io::Error> for TaskError {
    fn from(err: std::io::Error) -> Self {
        TaskError::Io(err.to_string())
    }
}

impl From<TaskError> for Status {
    fn from(err: TaskError) -> Self {
        log::error!("Task service error: {}", err);
        match err {
            TaskError::ContainerNotFound(id) => Status::not_found(id),
            TaskError::ContainerAlreadyExists(id) => Status::already_exists(id),
            TaskError::InvalidState {
                id,
                current_state,
                required_states,
            } => Status::failed_precondition(format!(
                "Invalid state for operation on container '{}': current state is {:?}, required one of {:?}",
                id, current_state, required_states
            )),
            TaskError::YoukiCommand(msg) | TaskError::Internal(msg) => Status::internal(msg),
            TaskError::Io(msg) => Status::internal(format!("I/O error: {}", msg)),
        }
    }
}
