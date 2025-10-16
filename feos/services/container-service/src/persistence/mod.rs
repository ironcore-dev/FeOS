// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use feos_proto::container_service::{ContainerConfig, ContainerState};
use uuid::Uuid;

pub mod repository;

#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
    #[error("A database error occurred")]
    Database(#[from] sqlx::Error),

    #[error("Database migration failed")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("Failed to decode ContainerConfig blob")]
    Decode(#[from] prost::DecodeError),

    #[error("Failed to encode ContainerConfig blob")]
    Encode(#[from] prost::EncodeError),

    #[error("Invalid state string '{0}' in database")]
    InvalidStateString(String),
}

#[derive(Debug, Clone)]
pub struct ContainerStatus {
    pub state: ContainerState,
    pub process_id: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct ContainerRecord {
    pub container_id: Uuid,
    pub image_uuid: Uuid,
    pub status: ContainerStatus,
    pub config: ContainerConfig,
}
