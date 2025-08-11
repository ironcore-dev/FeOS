use crate::RestartSignal;
use digest::Digest;
use feos_proto::host_service::{
    upgrade_request, HostnameResponse, KernelLogEntry, UpgradeRequest, UpgradeResponse,
};
use log::{error, info, warn};
use nix::unistd;
use sha2::Sha256;
use std::fs::Permissions;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tempfile::NamedTempFile;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio_stream::StreamExt;
use tonic::{Status, Streaming};

const UPGRADE_DIR: &str = "/var/lib/feos/upgrade";
const ELF_MAGIC: [u8; 4] = [0x7f, 0x45, 0x4c, 0x46];
const KMSG_PATH: &str = "/dev/kmsg";

pub async fn handle_hostname(responder: oneshot::Sender<Result<HostnameResponse, Status>>) {
    info!("HOST_WORKER: Processing Hostname request.");
    let result = match unistd::gethostname() {
        Ok(host) => {
            let hostname = host
                .into_string()
                .unwrap_or_else(|_| "Invalid UTF-8".into());
            Ok(HostnameResponse { hostname })
        }
        Err(e) => {
            let msg = format!("Failed to get system hostname: {e}");
            error!("HOST_WORKER: ERROR - {msg}");
            Err(Status::internal(msg))
        }
    };

    if responder.send(result).is_err() {
        error!(
            "HOST_WORKER: Failed to send response for Hostname. API handler may have timed out."
        );
    }
}

pub async fn handle_stream_kernel_logs(
    grpc_tx: mpsc::Sender<Result<KernelLogEntry, Status>>,
) {
    info!("HOST_WORKER: Opening {KMSG_PATH} for streaming kernel logs.");

    let file = match File::open(KMSG_PATH).await {
        Ok(f) => f,
        Err(e) => {
            let msg = format!("Failed to open {KMSG_PATH}: {e}");
            error!("HOST_WORKER: {msg}");
            if grpc_tx.send(Err(Status::internal(msg))).await.is_err() {
                warn!("HOST_WORKER: gRPC client for kernel logs disconnected before error could be sent.");
            }
            return;
        }
    };

    let mut reader = BufReader::new(file).lines();
    info!("HOST_WORKER: Streaming logs from {KMSG_PATH}.");

    loop {
        tokio::select! {
            biased;
            _ = grpc_tx.closed() => {
                info!("HOST_WORKER: gRPC client for kernel logs disconnected. Closing stream.");
                break;
            }
            line_res = reader.next_line() => {
                match line_res {
                    Ok(Some(line)) => {
                        let entry = KernelLogEntry { message: line };
                        if grpc_tx.send(Ok(entry)).await.is_err() {
                            info!("HOST_WORKER: gRPC client for kernel logs disconnected. Stopping stream.");
                            break;
                        }
                    }
                    Ok(None) => {
                        info!("HOST_WORKER: Reached EOF on {KMSG_PATH}. Stream finished.");
                        break;
                    }
                    Err(e) => {
                        let msg = format!("Error reading from {KMSG_PATH}: {e}");
                        error!("HOST_WORKER: {msg}");
                        let _ = grpc_tx.send(Err(Status::internal(msg))).await;
                        break;
                    }
                }
            }
        }
    }
}

pub async fn handle_upgrade(
    restart_tx: mpsc::Sender<RestartSignal>,
    mut stream: Streaming<UpgradeRequest>,
    responder: oneshot::Sender<Result<UpgradeResponse, Status>>,
) {
    info!("HOST_WORKER: Processing UpgradeFeosBinary request.");

    let expected_checksum = match stream.next().await {
        Some(Ok(req)) => match req.payload {
            Some(upgrade_request::Payload::Metadata(meta)) => {
                info!(
                    "HOST_WORKER: Received upgrade metadata. Checksum: {}",
                    meta.sha256_sum
                );
                meta.sha256_sum
            }
            _ => {
                let _ = responder.send(Err(Status::invalid_argument(
                    "First message must be UpgradeMetadata.",
                )));
                return;
            }
        },
        Some(Err(e)) => {
            let _ = responder.send(Err(e));
            return;
        }
        None => {
            let _ = responder.send(Err(Status::invalid_argument("Empty request stream.")));
            return;
        }
    };

    let temp_file = match tokio::task::block_in_place(|| {
        std::fs::create_dir_all(UPGRADE_DIR)?;
        NamedTempFile::new_in(UPGRADE_DIR)
    }) {
        Ok(f) => f,
        Err(e) => {
            let msg = format!("Failed to create temp file: {e}");
            error!("HOST_WORKER: {msg}");
            let _ = responder.send(Err(Status::internal(msg)));
            return;
        }
    };
    let mut temp_file_writer = temp_file.reopen().unwrap();

    while let Some(result) = stream.next().await {
        match result {
            Ok(req) => match req.payload {
                Some(upgrade_request::Payload::Chunk(chunk)) => {
                    if let Err(e) = temp_file_writer.write_all(&chunk) {
                        let msg = format!("Failed to write to temp file: {e}");
                        error!("HOST_WORKER: {msg}");
                        let _ = responder.send(Err(Status::internal(msg)));
                        return;
                    }
                }
                _ => {
                    let _ = responder.send(Err(Status::invalid_argument(
                        "Subsequent messages must be binary chunks.",
                    )));
                    return;
                }
            },
            Err(e) => {
                let _ = responder.send(Err(e));
                return;
            }
        }
    }

    let temp_path = temp_file.path().to_path_buf();
    let mut hasher = Sha256::new();
    let mut file_to_hash = match File::open(&temp_path).await {
        Ok(f) => f,
        Err(e) => {
            let msg = format!("Failed to reopen temp file for validation: {e}");
            error!("HOST_WORKER: {msg}");
            let _ = responder.send(Err(Status::internal(msg)));
            return;
        }
    };

    let mut first_bytes = [0u8; 4];
    if file_to_hash.read_exact(&mut first_bytes).await.is_err() {
        let _ = responder.send(Err(Status::invalid_argument(
            "Received file is too small to be a valid binary.",
        )));
        return;
    }

    if first_bytes != ELF_MAGIC {
        let _ = responder.send(Err(Status::invalid_argument(
            "Uploaded file is not a valid ELF binary.",
        )));
        return;
    }
    hasher.update(first_bytes);

    let mut buf = [0; 8192];
    loop {
        match file_to_hash.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => hasher.update(&buf[..n]),
            Err(e) => {
                let msg = format!("Failed to read temp file for hashing: {e}");
                error!("HOST_WORKER: {msg}");
                let _ = responder.send(Err(Status::internal(msg)));
                return;
            }
        }
    }
    let actual_checksum = hex::encode(hasher.finalize());

    if actual_checksum != expected_checksum {
        let msg =
            format!("Checksum mismatch. Expected: {expected_checksum}, Got: {actual_checksum}",);
        warn!("HOST_WORKER: {msg}");
        let _ = responder.send(Err(Status::invalid_argument(msg)));
        return;
    }
    info!("HOST_WORKER: Checksum validation successful.");

    let final_path = PathBuf::from(UPGRADE_DIR).join("feos.new");
    if let Err(e) = tokio::task::block_in_place(|| {
        let perms = Permissions::from_mode(0o755);
        temp_file.persist(&final_path)?;
        std::fs::set_permissions(&final_path, perms)
    }) {
        let msg = format!("Failed to persist and set permissions on new binary: {e}");
        error!("HOST_WORKER: {msg}");
        let _ = responder.send(Err(Status::internal(msg)));
        return;
    }

    info!("HOST_WORKER: Staged new binary at {:?}", &final_path);

    if let Err(e) = restart_tx.send(RestartSignal(final_path)).await {
        let msg = format!("Failed to send restart signal to main process: {e}");
        error!("HOST_WORKER: CRITICAL - {msg}");
        let _ = responder.send(Err(Status::internal(msg)));
        return;
    }

    info!("HOST_WORKER: Restart signal sent. Responding to client.");
    let _ = responder.send(Ok(UpgradeResponse {
        message: "Binary received and validated. System will now restart with the new binary."
            .to_string(),
    }));
}