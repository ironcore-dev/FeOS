// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use feos_proto::vm_service::{VmConfig, VmState};
use uuid::Uuid;

pub mod repository;

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
