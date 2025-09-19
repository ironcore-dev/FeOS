// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use crate::RestartSignal;
use digest::Digest;
use feos_proto::host_service::{
    FeosLogEntry, KernelLogEntry, UpgradeFeosBinaryRequest, UpgradeFeosBinaryResponse,
};
use feos_utils::feos_logger::LogHandle;
use http_body_util::{BodyExt, Empty};
use hyper::body::Bytes;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::{client::legacy::Client, rt::TokioExecutor};
use log::{error, info, warn};
use prost_types::Timestamp;
use sha2::Sha256;
use std::fs::Permissions;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tempfile::NamedTempFile;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::sync::{mpsc, oneshot};
use tonic::Status;

const UPGRADE_DIR: &str = "/var/lib/feos/upgrade";
const ELF_MAGIC: [u8; 4] = [0x7f, 0x45, 0x4c, 0x46];
const KMSG_PATH: &str = "/dev/kmsg";

pub async fn handle_stream_feos_logs(
    log_handle: LogHandle,
    grpc_tx: mpsc::Sender<Result<FeosLogEntry, Status>>,
) {
    info!("HostWorker: Starting new FeOS log stream.");
    let mut reader = match log_handle.new_reader().await {
        Ok(r) => r,
        Err(e) => {
            error!("HostWorker: Failed to create log reader: {e}");
            if grpc_tx
                .send(Err(Status::internal("Failed to create log reader")))
                .await
                .is_err()
            {
                warn!("HostWorker: gRPC client for FeOS logs disconnected before error could be sent.");
            }
            return;
        }
    };

    while let Some(entry) = reader.next().await {
        let feos_log_entry = FeosLogEntry {
            seq: entry.seq,
            timestamp: Some(Timestamp {
                seconds: entry.timestamp.timestamp(),
                nanos: entry.timestamp.timestamp_subsec_nanos() as i32,
            }),
            level: entry.level.to_string(),
            target: entry.target,
            message: entry.message,
        };

        if grpc_tx.send(Ok(feos_log_entry)).await.is_err() {
            info!("HostWorker: Log stream client disconnected.");
            break;
        }
    }
    info!("HostWorker: FeOS log stream finished.");
}

pub async fn handle_stream_kernel_logs(grpc_tx: mpsc::Sender<Result<KernelLogEntry, Status>>) {
    info!("HostWorker: Opening {KMSG_PATH} for streaming kernel logs.");

    let file = match File::open(KMSG_PATH).await {
        Ok(f) => f,
        Err(e) => {
            let msg = format!("Failed to open {KMSG_PATH}: {e}");
            error!("HostWorker: {msg}");
            if grpc_tx.send(Err(Status::internal(msg))).await.is_err() {
                warn!("HostWorker: gRPC client for kernel logs disconnected before error could be sent.");
            }
            return;
        }
    };

    let mut reader = BufReader::new(file).lines();
    info!("HostWorker: Streaming logs from {KMSG_PATH}.");

    loop {
        tokio::select! {
            biased;
            _ = grpc_tx.closed() => {
                info!("HostWorker: gRPC client for kernel logs disconnected. Closing stream.");
                break;
            }
            line_res = reader.next_line() => {
                match line_res {
                    Ok(Some(line)) => {
                        let entry = KernelLogEntry { message: line };
                        if grpc_tx.send(Ok(entry)).await.is_err() {
                            info!("HostWorker: gRPC client for kernel logs disconnected. Stopping stream.");
                            break;
                        }
                    }
                    Ok(None) => {
                        info!("HostWorker: Reached EOF on {KMSG_PATH}. Stream finished.");
                        break;
                    }
                    Err(e) => {
                        let msg = format!("Error reading from {KMSG_PATH}: {e}");
                        error!("HostWorker: {msg}");
                        let _ = grpc_tx.send(Err(Status::internal(msg))).await;
                        break;
                    }
                }
            }
        }
    }
}

async fn download_file(url: &str, temp_file_writer: &mut std::fs::File) -> Result<(), String> {
    info!("HostWorker: Starting download from {url}");

    let https = HttpsConnectorBuilder::new()
        .with_native_roots()
        .map_err(|e| format!("Could not load native root certificates: {e}"))?
        .https_or_http()
        .enable_http1()
        .build();

    let client: Client<_, Empty<Bytes>> = Client::builder(TokioExecutor::new()).build(https);
    let uri = url.parse::<hyper::Uri>().map_err(|e| e.to_string())?;
    let mut res = client
        .get(uri)
        .await
        .map_err(|e| format!("HTTP GET request failed: {e}"))?;

    info!("HostWorker: Download response status: {}", res.status());
    if !res.status().is_success() {
        return Err(format!("Download failed with status: {}", res.status()));
    }

    while let Some(next) = res.frame().await {
        let frame = next.map_err(|e| format!("Error reading response frame: {e}"))?;
        if let Some(chunk) = frame.data_ref() {
            temp_file_writer
                .write_all(chunk)
                .map_err(|e| format!("Failed to write chunk to temp file: {e}"))?;
        }
    }

    info!("HostWorker: Download completed successfully.");
    Ok(())
}

pub async fn handle_upgrade(
    restart_tx: mpsc::Sender<RestartSignal>,
    req: UpgradeFeosBinaryRequest,
    responder: oneshot::Sender<Result<UpgradeFeosBinaryResponse, Status>>,
) {
    info!(
        "HostWorker: Processing UpgradeFeosBinary request for url {}",
        req.url
    );

    if responder.send(Ok(UpgradeFeosBinaryResponse {})).is_err() {
        warn!("HostWorker: Could not send response for UpgradeFeosBinary. Client may have disconnected.");
    }

    let temp_file = match tokio::task::block_in_place(|| {
        std::fs::create_dir_all(UPGRADE_DIR)?;
        NamedTempFile::new_in(UPGRADE_DIR)
    }) {
        Ok(f) => f,
        Err(e) => {
            error!("HostWorker: Failed to create temp file: {e}");
            return;
        }
    };

    let mut temp_file_writer = match temp_file.reopen() {
        Ok(f) => f,
        Err(e) => {
            error!("HostWorker: Failed to reopen temp file for writing: {e}");
            return;
        }
    };

    if let Err(e) = download_file(&req.url, &mut temp_file_writer).await {
        error!("HostWorker: Failed to download binary: {e}");
        return;
    }

    let temp_path = temp_file.path().to_path_buf();
    let mut hasher = Sha256::new();
    let mut file_to_hash = match File::open(&temp_path).await {
        Ok(f) => f,
        Err(e) => {
            error!("HostWorker: Failed to reopen temp file for validation: {e}");
            return;
        }
    };

    let mut first_bytes = [0u8; 4];
    if file_to_hash.read_exact(&mut first_bytes).await.is_err() {
        error!("HostWorker: Downloaded file is too small to be a valid binary.");
        return;
    }

    if first_bytes != ELF_MAGIC {
        error!("HostWorker: Downloaded file is not a valid ELF binary.");
        return;
    }
    hasher.update(first_bytes);

    let mut buf = [0; 8192];
    loop {
        match file_to_hash.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => hasher.update(&buf[..n]),
            Err(e) => {
                error!("HostWorker: Failed to read temp file for hashing: {e}");
                return;
            }
        }
    }
    let actual_checksum = hex::encode(hasher.finalize());

    if actual_checksum != req.sha256_sum {
        error!(
            "HostWorker: Checksum mismatch. Expected: {}, Got: {}",
            req.sha256_sum, actual_checksum
        );
        return;
    }
    info!("HostWorker: Checksum validation successful.");

    let final_path = PathBuf::from(UPGRADE_DIR).join("feos.new");
    if let Err(e) = tokio::task::block_in_place(|| {
        let perms = Permissions::from_mode(0o755);
        temp_file.persist(&final_path)?;
        std::fs::set_permissions(&final_path, perms)
    }) {
        error!("HostWorker: Failed to persist and set permissions on new binary: {e}");
        return;
    }

    info!("HostWorker: Staged new binary at {:?}", &final_path);

    if let Err(e) = restart_tx.send(RestartSignal(final_path)).await {
        error!("HostWorker: CRITICAL - Failed to send restart signal to main process: {e}");
        return;
    }

    info!("HostWorker: Restart signal sent.");
}
