// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use oci_distribution::{errors::OciDistributionError, ParseError};
use tonic::Status;

#[derive(Debug, thiserror::Error)]
pub enum ImageServiceError {
    #[error("Failed to pull OCI image")]
    OciPull(#[from] OciDistributionError),

    #[error("Failed to parse OCI image reference")]
    OciParse(#[from] ParseError),

    #[error("Required image layer '{0}' not found in manifest")]
    MissingLayer(String),

    #[error("A file storage error occurred")]
    Storage(#[from] std::io::Error),

    #[error("Image with ID '{0}' not found")]
    NotFound(String),

    #[error("An internal orchestrator error occurred: {0}")]
    Internal(String),
}

impl From<ImageServiceError> for Status {
    fn from(err: ImageServiceError) -> Self {
        log::error!("ImageServiceError: {err}");
        match err {
            ImageServiceError::NotFound(id) => {
                Status::not_found(format!("Image with ID '{id}' not found"))
            }
            ImageServiceError::OciParse(_) => Status::invalid_argument(err.to_string()),
            ImageServiceError::OciPull(_) | ImageServiceError::MissingLayer(_) => {
                Status::unavailable(err.to_string())
            }
            ImageServiceError::Storage(_) | ImageServiceError::Internal(_) => {
                Status::internal(err.to_string())
            }
        }
    }
}
