// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use feos_proto::{
    host_service::host_service_client::HostServiceClient,
    image_service::image_service_client::ImageServiceClient,
    vm_service::vm_service_client::VmServiceClient,
};
use hyper_util::rt::TokioIo;
use image_service::IMAGE_SERVICE_SOCKET;
use log::{error, info};
use once_cell::sync::{Lazy, OnceCell as SyncOnceCell};
use std::env;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UnixStream;
use tokio::sync::OnceCell as TokioOnceCell;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;
use vm_service::VM_CH_BIN;

pub mod fixtures;
pub mod host_tests;
pub mod image_tests;
pub mod vm_tests;

pub const PUBLIC_SERVER_ADDRESS: &str = "http://[::1]:1337";
pub const DEFAULT_TEST_IMAGE_REF: &str = "ghcr.io/ironcore-dev/os-images/gardenlinux-ch-dev";

pub static TEST_IMAGE_REF: Lazy<String> =
    Lazy::new(|| env::var("TEST_IMAGE_REF").unwrap_or_else(|_| DEFAULT_TEST_IMAGE_REF.to_string()));

static SERVER_RUNTIME: TokioOnceCell<Arc<tokio::runtime::Runtime>> = TokioOnceCell::const_new();
static TEMP_DIR_GUARD: SyncOnceCell<tempfile::TempDir> = SyncOnceCell::new();

pub async fn ensure_server() {
    SERVER_RUNTIME
        .get_or_init(|| async { setup_server().await })
        .await;
}

async fn setup_server() -> Arc<tokio::runtime::Runtime> {
    let temp_dir = TEMP_DIR_GUARD.get_or_init(|| {
        tempfile::Builder::new()
            .prefix("feos-test-")
            .tempdir()
            .expect("Failed to create temp dir")
    });

    let db_path = temp_dir.path().join("vms.db");
    let db_url = format!("sqlite:{}", db_path.to_str().unwrap());

    env::set_var("DATABASE_URL", &db_url);
    info!("Using temporary database for tests: {}", db_url);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create a new Tokio runtime for the server");

    runtime.spawn(async move {
        if let Err(e) = main_server::run_server(false).await {
            panic!("Test server failed to run: {}", e);
        }
    });

    info!("Waiting for the server to start...");
    for _ in 0..20 {
        if Channel::from_static(PUBLIC_SERVER_ADDRESS)
            .connect()
            .await
            .is_ok()
        {
            info!("Server is up and running at {}", PUBLIC_SERVER_ADDRESS);
            return Arc::new(runtime);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    error!("Server did not start in time.");
    panic!("Server did not start in time.");
}

pub async fn get_public_clients() -> Result<(VmServiceClient<Channel>, HostServiceClient<Channel>)>
{
    let vm_client = VmServiceClient::connect(PUBLIC_SERVER_ADDRESS).await?;
    let host_client = HostServiceClient::connect(PUBLIC_SERVER_ADDRESS).await?;
    Ok((vm_client, host_client))
}

pub async fn get_image_service_client() -> Result<ImageServiceClient<Channel>> {
    let endpoint = Endpoint::from_static("http://[::1]:50051");
    let channel = endpoint
        .connect_with_connector(service_fn(|_: Uri| async {
            UnixStream::connect(IMAGE_SERVICE_SOCKET)
                .await
                .map(TokioIo::new)
        }))
        .await?;
    Ok(ImageServiceClient::new(channel))
}

pub fn check_ch_binary() -> bool {
    Command::new("which")
        .arg(VM_CH_BIN)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn skip_if_ch_binary_missing() -> bool {
    if !check_ch_binary() {
        log::warn!(
            "Skipping test because '{}' binary was not found in PATH.",
            VM_CH_BIN
        );
        return true;
    }
    false
}
