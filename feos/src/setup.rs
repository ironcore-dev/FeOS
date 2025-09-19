// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use feos_proto::{
    host_service::host_service_server::HostServiceServer,
    image_service::image_service_server::ImageServiceServer,
    vm_service::vm_service_server::VmServiceServer,
};
use feos_utils::filesystem::mount_virtual_filesystems;
use feos_utils::host::info::is_running_on_vm;
use feos_utils::host::memory::configure_hugepages;
use feos_utils::network::{configure_network_devices, configure_sriov};
use host_service::{
    api::HostApiHandler, dispatcher::HostServiceDispatcher, Command as HostCommand, RestartSignal,
};
use image_service::{
    api::ImageApiHandler, dispatcher::ImageServiceDispatcher, filestore::FileStore,
    worker::Orchestrator, IMAGE_DIR,
};
use log::{error, info, warn};
use nix::libc;
use std::env;
use std::ffi::CString;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tokio::fs::{self, File};
use tokio::sync::mpsc;
use vm_service::{
    api::VmApiHandler, dispatcher::VmServiceDispatcher, Command as VmCommand, DEFAULT_VM_DB_URL,
    VM_API_SOCKET_DIR, VM_CONSOLE_DIR,
};

pub(crate) const VFS_NUM: u32 = 125;
pub(crate) const HUGEPAGES_NUM: u32 = 1024;

pub(crate) async fn initialize_vm_service(db_url: &str) -> Result<VmServiceServer<VmApiHandler>> {
    info!("MAIN: Ensuring VM socket directory '{VM_API_SOCKET_DIR}' exists...");
    fs::create_dir_all(VM_API_SOCKET_DIR).await?;
    info!("MAIN: Directory check complete. Path '{VM_API_SOCKET_DIR}' is ready.");

    info!("MAIN: Ensuring VM console directory '{VM_CONSOLE_DIR}' exists...");
    fs::create_dir_all(VM_CONSOLE_DIR).await?;
    info!("MAIN: Directory check complete. Path '{VM_CONSOLE_DIR}' is ready.");

    let (vm_tx, vm_rx) = mpsc::channel::<VmCommand>(32);
    let vm_dispatcher = VmServiceDispatcher::new(vm_rx, db_url).await?;
    tokio::spawn(async move {
        vm_dispatcher.run().await;
    });
    let vm_api_handler = VmApiHandler::new(vm_tx);
    let vm_service = VmServiceServer::new(vm_api_handler);
    info!("MAIN: VM Service is configured.");

    Ok(vm_service)
}

pub(crate) fn initialize_host_service(
    restart_tx: mpsc::Sender<RestartSignal>,
    log_handle: feos_utils::feos_logger::LogHandle,
) -> HostServiceServer<HostApiHandler> {
    let (host_tx, host_rx) = mpsc::channel::<HostCommand>(32);
    let host_dispatcher = HostServiceDispatcher::new(host_rx, restart_tx, log_handle);
    tokio::spawn(async move {
        host_dispatcher.run().await;
    });
    let host_api_handler = HostApiHandler::new(host_tx);
    let host_service = HostServiceServer::new(host_api_handler);
    info!("MAIN: Host Service is configured.");

    host_service
}

pub(crate) async fn initialize_image_service() -> Result<ImageServiceServer<ImageApiHandler>> {
    info!("MAIN: Ensuring image directory '{IMAGE_DIR}' exists...");
    fs::create_dir_all(IMAGE_DIR).await?;
    info!("MAIN: Directory check complete. Path '{IMAGE_DIR}' is ready.");

    let filestore_actor = FileStore::new();
    let filestore_tx = filestore_actor.get_command_sender();
    tokio::spawn(async move {
        filestore_actor.run().await;
    });
    info!("MAIN: FileStore actor for Image Service has been started.");

    let orchestrator_actor = Orchestrator::new(filestore_tx);
    let orchestrator_tx = orchestrator_actor.get_command_sender();
    tokio::spawn(async move {
        orchestrator_actor.run().await;
    });
    info!("MAIN: Orchestrator actor for Image Service has been started.");

    let grpc_dispatcher = ImageServiceDispatcher::new(orchestrator_tx);
    let grpc_dispatcher_tx = grpc_dispatcher.get_command_sender();
    tokio::spawn(async move {
        grpc_dispatcher.run().await;
    });
    info!("MAIN: gRPC Dispatcher for Image Service has been started.");

    let image_api_handler = ImageApiHandler::new(grpc_dispatcher_tx);
    let image_service = ImageServiceServer::new(image_api_handler);
    info!("MAIN: Image Service is configured.");

    Ok(image_service)
}

pub(crate) async fn perform_first_boot_initialization() -> Result<()> {
    info!("MAIN: Performing first-boot initialization...");
    info!("MAIN: Mounting virtual filesystems...");
    mount_virtual_filesystems();

    info!("MAIN: Configuring hugepages...");
    if let Err(e) = configure_hugepages(HUGEPAGES_NUM).await {
        warn!("Failed to configure hugepages: {e}");
    }

    let is_on_vm = is_running_on_vm().await.unwrap_or_else(|e| {
        error!("Error checking VM status: {e}");
        false // Default to false in case of error
    });

    info!("MAIN: Configuring network devices...");
    if let Some((delegated_prefix, delegated_prefix_length)) = configure_network_devices()
        .await
        .expect("could not configure network devices")
    {
        info!("MAIN: Delegated prefix: {delegated_prefix}/{delegated_prefix_length}");
    }

    if !is_on_vm {
        info!("configuring sriov...");
        if let Err(e) = configure_sriov(VFS_NUM).await {
            warn!("failed to configure sriov: {e}")
        }
    }

    Ok(())
}

pub(crate) async fn setup_database() -> Result<String> {
    dotenvy::dotenv().ok();

    let db_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
        info!("MAIN: DATABASE_URL not set, using default '{DEFAULT_VM_DB_URL}'");
        DEFAULT_VM_DB_URL.to_string()
    });

    if let Some(db_path_str) = db_url.strip_prefix("sqlite:") {
        let db_path = Path::new(db_path_str);
        if let Some(db_dir) = db_path.parent() {
            info!(
                "MAIN: Ensuring database directory '{}' exists...",
                db_dir.display()
            );
            fs::create_dir_all(db_dir).await?;
        }
        if !db_path.exists() {
            info!(
                "MAIN: Database file does not exist, creating at '{}'...",
                db_path.display()
            );
            File::create(db_path).await?;
        }
    }

    Ok(db_url)
}

pub(crate) fn handle_upgrade(new_binary_path: &Path) -> Result<()> {
    info!("MAIN: Upgrade signal received. New binary at {new_binary_path:?}. Preparing to execv.");

    let current_exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(e) => {
            // Using panic here as not knowing the current exe is a fatal state.
            panic!("FATAL: Could not get current executable path: {e}");
        }
    };
    info!("MAIN: Current binary is at {:?}", &current_exe);

    let rename_result = std::fs::rename(new_binary_path, &current_exe);

    match rename_result {
        Ok(_) => {
            info!("MAIN: Successfully replaced on-disk binary via atomic rename.");
        }
        Err(e) if e.raw_os_error() == Some(libc::EXDEV) => {
            info!("MAIN: Cross-device link detected. Falling back to copy-then-rename strategy.");
            let staging_path = current_exe.with_extension("staging");
            if let Err(copy_err) = std::fs::copy(new_binary_path, &staging_path) {
                error!(
                    "CRITICAL: Failed to copy new binary to staging path {:?}: {}. Aborting upgrade.",
                    &staging_path, copy_err
                );
                return Ok(());
            }
            if let Err(perm_err) =
                std::fs::set_permissions(&staging_path, std::fs::Permissions::from_mode(0o755))
            {
                error!(
                    "CRITICAL: Failed to set permissions on staged binary {:?}: {}. Aborting upgrade.",
                    &staging_path, perm_err
                );
                let _ = std::fs::remove_file(&staging_path);
                return Ok(());
            }
            if let Err(final_rename_err) = std::fs::rename(&staging_path, &current_exe) {
                error!(
                    "CRITICAL: Failed to perform final atomic rename from {:?}: {}. Aborting upgrade.",
                    &staging_path, final_rename_err
                );
                let _ = std::fs::remove_file(&staging_path);
                return Ok(());
            }
            let _ = std::fs::remove_file(new_binary_path);
            info!("MAIN: Successfully replaced on-disk binary via copy-then-rename.");
        }
        Err(e) => {
            error!(
                "CRITICAL: Failed to rename new binary into place with an unexpected error: {e}. Aborting upgrade."
            );
            return Ok(());
        }
    }

    let mut args: Vec<String> = std::env::args().collect();
    let restart_flag = "--restarted-after-upgrade";
    if !args.contains(&restart_flag.to_string()) {
        args.push(restart_flag.to_string());
    }

    let cstr_args: Vec<CString> = args
        .into_iter()
        .map(|arg| CString::new(arg).unwrap())
        .collect();
    let cstr_path = CString::new(current_exe.into_os_string().into_vec()).unwrap();

    info!(
        "MAIN: Executing new binary with arguments: {:?}",
        &cstr_args
    );
    let Err(e) = nix::unistd::execv(&cstr_path, &cstr_args);
    // execv replaces the current process, so it should not return.
    // If it returns, it's a fatal error.
    panic!("FATAL: execv failed after replacing binary: {e}");
}
