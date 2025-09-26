// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use crate::persistence::PersistenceError;
use tonic::Status;

#[derive(Debug, thiserror::Error)]
pub enum VmServiceError {
    #[error("VMM Error: {0}")]
    Vmm(#[from] crate::vmm::VmmError),

    #[error("Persistence Error: {0}")]
    Persistence(#[from] PersistenceError),

    #[error("Image Service Error: {0}")]
    ImageService(String),

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("VM with ID {0} already exists")]
    AlreadyExists(String),

    #[error("Invalid VM state for operation: {0}")]
    InvalidState(String),
}

impl From<VmServiceError> for Status {
    fn from(err: VmServiceError) -> Self {
        log::error!("VmServiceError: {err}");
        match err {
            VmServiceError::Vmm(vmm_err) => vmm_err.into(),
            VmServiceError::Persistence(PersistenceError::Database(ref e))
                if matches!(e, sqlx::Error::RowNotFound) =>
            {
                Status::not_found("Record not found in database")
            }
            VmServiceError::Persistence(_) => Status::internal("A database error occurred"),
            VmServiceError::ImageService(msg) => {
                Status::unavailable(format!("Image service unavailable: {msg}"))
            }
            VmServiceError::InvalidArgument(msg) => Status::invalid_argument(msg),
            VmServiceError::AlreadyExists(msg) => Status::already_exists(msg),
            VmServiceError::InvalidState(msg) => Status::failed_precondition(msg),
        }
    }
}
