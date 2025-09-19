// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use crate::persistence::{VmRecord, VmStatus};
use anyhow::{anyhow, Result};
use feos_proto::vm_service::{VmConfig, VmState};
use log::info;
use prost::Message;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use uuid::Uuid;

#[derive(Clone)]
pub struct VmRepository {
    pool: SqlitePool,
}

#[derive(sqlx::FromRow, Debug)]
struct DbVmRow {
    vm_id: Uuid,
    image_uuid: Uuid,
    state: String,
    last_msg: String,
    pid: Option<i64>,
    config_blob: Vec<u8>,
}

fn string_to_vm_state(s: &str) -> Result<VmState, anyhow::Error> {
    match s {
        "VM_STATE_CREATING" => Ok(VmState::Creating),
        "VM_STATE_CREATED" => Ok(VmState::Created),
        "VM_STATE_RUNNING" => Ok(VmState::Running),
        "VM_STATE_PAUSED" => Ok(VmState::Paused),
        "VM_STATE_STOPPED" => Ok(VmState::Stopped),
        "VM_STATE_CRASHED" => Ok(VmState::Crashed),
        "VM_STATE_UNSPECIFIED" => Ok(VmState::Unspecified),
        _ => Err(anyhow!("Invalid state string '{}' in database", s)),
    }
}

impl VmRepository {
    pub async fn connect(db_url: &str) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(db_url)
            .await?;

        info!("Persistence: Running database migrations...");
        sqlx::migrate!("./migrations").run(&pool).await?;
        info!("Persistence: Database migrations completed.");

        Ok(Self { pool })
    }

    pub async fn get_vm(&self, vm_id: Uuid) -> Result<Option<VmRecord>> {
        let row_opt = sqlx::query_as::<_, DbVmRow>(
            "SELECT vm_id, image_uuid, state, last_msg, pid, config_blob FROM vms WHERE vm_id = ?1",
        )
        .bind(vm_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row_opt {
            let config = VmConfig::decode(&*row.config_blob)
                .map_err(|e| anyhow!("Failed to decode VmConfig blob: {}", e))?;

            let state = string_to_vm_state(&row.state)?;

            let record = VmRecord {
                vm_id: row.vm_id,
                image_uuid: row.image_uuid,
                status: VmStatus {
                    state,
                    last_msg: row.last_msg,
                    process_id: row.pid,
                },
                config,
            };
            Ok(Some(record))
        } else {
            Ok(None)
        }
    }

    pub async fn list_all_vms(&self) -> Result<Vec<VmRecord>> {
        let rows = sqlx::query_as::<_, DbVmRow>(
            "SELECT vm_id, image_uuid, state, last_msg, pid, config_blob FROM vms",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut records = Vec::with_capacity(rows.len());
        for row in rows {
            let config = VmConfig::decode(&*row.config_blob).map_err(|e| {
                anyhow!(
                    "Failed to decode VmConfig blob for vm_id {}: {}",
                    row.vm_id,
                    e
                )
            })?;

            let state = string_to_vm_state(&row.state)?;

            let record = VmRecord {
                vm_id: row.vm_id,
                image_uuid: row.image_uuid,
                status: VmStatus {
                    state,
                    last_msg: row.last_msg,
                    process_id: row.pid,
                },
                config,
            };
            records.push(record);
        }

        Ok(records)
    }

    pub async fn save_vm(&self, vm: &VmRecord) -> Result<()> {
        let mut config_blob = Vec::new();
        vm.config.encode(&mut config_blob)?;

        let state_str = format!("VM_STATE_{:?}", vm.status.state).to_uppercase();

        sqlx::query_unchecked!(
            r#"
            INSERT OR REPLACE INTO vms (vm_id, image_uuid, state, last_msg, pid, config_blob)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            vm.vm_id,
            vm.image_uuid,
            state_str,
            vm.status.last_msg,
            vm.status.process_id,
            config_blob,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_vm_status(
        &self,
        vm_id: Uuid,
        new_state: VmState,
        message: &str,
    ) -> Result<bool> {
        let state_str = format!("VM_STATE_{new_state:?}").to_uppercase();

        let result = sqlx::query!(
            r#"
            UPDATE vms
            SET state = ?1, last_msg = ?2
            WHERE vm_id = ?3
            "#,
            state_str,
            message,
            vm_id,
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn update_vm_pid(&self, vm_id: Uuid, pid: i64) -> Result<()> {
        sqlx::query!("UPDATE vms SET pid = ?1 WHERE vm_id = ?2", pid, vm_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_vm(&self, vm_id: Uuid) -> Result<()> {
        let result = sqlx::query!("DELETE FROM vms WHERE vm_id = ?1", vm_id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            log::warn!("Attempted to delete VM {vm_id} from DB, but no record was found.");
        }

        Ok(())
    }
}
