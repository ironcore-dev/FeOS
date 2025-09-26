// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use crate::error::HostError;
use feos_proto::host_service::{RebootRequest, RebootResponse, ShutdownRequest, ShutdownResponse};
use log::{error, info};
use nix::sys::reboot::{reboot, RebootMode};
use tokio::sync::oneshot;

pub async fn handle_shutdown(
    _req: ShutdownRequest,
    responder: oneshot::Sender<Result<ShutdownResponse, HostError>>,
) {
    info!("HostWorker: Processing Shutdown request.");

    if responder.send(Ok(ShutdownResponse {})).is_err() {
        error!(
            "HostWorker: Failed to send response for Shutdown. The client may have disconnected."
        );
    }

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    info!("HostWorker: Executing system shutdown.");
    match reboot(RebootMode::RB_POWER_OFF) {
        Ok(infallible) => match infallible {},
        Err(e) => {
            error!("HostWorker: CRITICAL - Failed to execute system shutdown: {e}");
        }
    }
}

pub async fn handle_reboot(
    _req: RebootRequest,
    responder: oneshot::Sender<Result<RebootResponse, HostError>>,
) {
    info!("HostWorker: Processing Reboot request.");

    if responder.send(Ok(RebootResponse {})).is_err() {
        error!("HostWorker: Failed to send response for Reboot. The client may have disconnected.");
    }

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    info!("HostWorker: Executing system reboot.");
    match reboot(RebootMode::RB_AUTOBOOT) {
        Ok(infallible) => match infallible {},
        Err(e) => {
            error!("HostWorker: CRITICAL - Failed to execute system reboot: {e}");
        }
    }
}
