// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use crate::persistence::PersistenceError;
use tonic::Status;

#[derive(Debug, thiserror::Error)]
pub enum ContainerServiceError {
    #[error("Persistence Error: {0}")]
    Persistence(#[from] PersistenceError),

    #[error("Image Service Error: {0}")]
    ImageService(String),

    #[error("Task Service Error: {0}")]
    TaskService(String),

    #[error("Runtime adapter error: {0}")]
    Adapter(String),

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("Container with ID {0} already exists")]
    AlreadyExists(String),

    #[error("Invalid container state for operation: {0}")]
    InvalidState(String),
}

impl From<ContainerServiceError> for Status {
    fn from(err: ContainerServiceError) -> Self {
        log::error!("ContainerServiceError: {err}");
        match err {
            ContainerServiceError::Persistence(PersistenceError::Database(ref e))
                if matches!(e, sqlx::Error::RowNotFound) =>
            {
                Status::not_found("Record not found in database")
            }
            ContainerServiceError::Persistence(_) => Status::internal("A database error occurred"),
            ContainerServiceError::ImageService(msg) => {
                Status::unavailable(format!("Image service unavailable: {msg}"))
            }
            ContainerServiceError::TaskService(msg) => {
                Status::unavailable(format!("Task service unavailable: {msg}"))
            }
            ContainerServiceError::Adapter(msg) => {
                Status::internal(format!("Runtime adapter error: {msg}"))
            }
            ContainerServiceError::InvalidArgument(msg) => Status::invalid_argument(msg),
            ContainerServiceError::AlreadyExists(msg) => Status::already_exists(msg),
            ContainerServiceError::InvalidState(msg) => Status::failed_precondition(msg),
        }
    }
}
