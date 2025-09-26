// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use tonic::Status;

#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error("Failed to get system hostname")]
    Hostname(#[from] nix::Error),

    #[error("Failed to read system info from {path}")]
    SystemInfoRead {
        #[source]
        source: std::io::Error,
        path: String,
    },

    #[error("Host power operation failed")]
    PowerOperation(#[source] nix::Error),

    #[error("Failed to create log reader: {0}")]
    LogReader(String),
}

impl From<HostError> for Status {
    fn from(err: HostError) -> Self {
        log::error!("HostServiceError: {}", err);
        match err {
            HostError::SystemInfoRead { path, .. } => {
                Status::internal(format!("Failed to read system info from {}", path))
            }
            HostError::Hostname(_) | HostError::PowerOperation(_) => {
                Status::internal("An internal host error occurred")
            }
            HostError::LogReader(msg) => Status::internal(msg),
        }
    }
}
