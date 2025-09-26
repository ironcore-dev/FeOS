// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use feos_proto::vm_service::{VmConfig, VmState};
use uuid::Uuid;

pub mod repository;

#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
    #[error("A database error occurred")]
    Database(#[from] sqlx::Error),

    #[error("Database migration failed")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("Failed to decode VmConfig blob")]
    Decode(#[from] prost::DecodeError),

    #[error("Failed to encode VmConfig blob")]
    Encode(#[from] prost::EncodeError),

    #[error("Invalid state string '{0}' in database")]
    InvalidStateString(String),
}

#[derive(Debug, Clone)]
pub struct VmStatus {
    pub state: VmState,
    pub last_msg: String,
    pub process_id: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct VmRecord {
    pub vm_id: Uuid,
    pub image_uuid: Uuid,
    pub status: VmStatus,
    pub config: VmConfig,
}
