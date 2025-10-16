// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

mod setup;

use anyhow::Result;
use host_service::RestartSignal;
use image_service::IMAGE_SERVICE_SOCKET;
use log::{error, info, warn};
use nix::unistd::Uid;
use setup::*;
use task_service::TASK_SERVICE_SOCKET;
use tokio::{fs, net::UnixListener, sync::mpsc};
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::Server;

pub async fn run_server(restarted_after_upgrade: bool) -> Result<()> {
    println!(
        "
    ███████╗███████╗ ██████╗ ███████╗
    ██╔════╝██╔════╝██╔═══██╗██╔════╝
    █████╗  █████╗  ██║   ██║███████╗
    ██╔══╝  ██╔══╝  ██║   ██║╚════██║
    ██║     ███████╗╚██████╔╝███████║
    ╚═╝     ╚══════╝ ╚═════╝ ╚══════╝
                 v{}
    ",
        feos_utils::version::full_version_string()
    );

    let log_handle = feos_utils::feos_logger::Builder::new()
        .filter_level(log::LevelFilter::Info)
        .max_history(150)
        .init()
        .expect("Failed to initialize feos_logger");

    if !Uid::current().is_root() {
        warn!("Not running as root! (uid: {})", Uid::current());
    }

    if !restarted_after_upgrade {
        if std::process::id() == 1 {
            perform_first_boot_initialization().await?;
        }
    } else {
        info!("Main: Skipping one-time initialization on restart after upgrade.");
    }

    // This now only sets up the database for the VM service.
    let vm_db_url = setup_database().await?;

    let (restart_tx, mut restart_rx) = mpsc::channel::<RestartSignal>(1);

    // Initialize each service. container_service will now set up its own database.
    let vm_service = initialize_vm_service(&vm_db_url).await?;
    let container_service = initialize_container_service().await?;

    let host_service = initialize_host_service(restart_tx.clone(), log_handle);

    let image_service = initialize_image_service().await?;
    let task_service = initialize_task_service().await?;

    let tcp_addr = "[::]:1337".parse().unwrap();
    let tcp_server = Server::builder()
        .add_service(vm_service)
        .add_service(container_service)
        .add_service(host_service)
        .serve(tcp_addr);

    fs::remove_file(IMAGE_SERVICE_SOCKET).await.ok();
    let image_uds = UnixListener::bind(IMAGE_SERVICE_SOCKET)?;
    let image_uds_stream = UnixListenerStream::new(image_uds);
    let image_unix_socket_server = Server::builder()
        .add_service(image_service)
        .serve_with_incoming(image_uds_stream);

    fs::remove_file(TASK_SERVICE_SOCKET).await.ok();
    let task_uds = UnixListener::bind(TASK_SERVICE_SOCKET)?;
    let task_uds_stream = UnixListenerStream::new(task_uds);
    let task_unix_socket_server = Server::builder()
        .add_service(task_service)
        .serve_with_incoming(task_uds_stream);

    info!("Main: Public gRPC Server listening on {tcp_addr}");
    info!("Main: Internal ImageService listening on Unix socket {IMAGE_SERVICE_SOCKET}");
    info!("Main: Internal TaskService listening on Unix socket {TASK_SERVICE_SOCKET}");

    tokio::select! {
        res = tcp_server => {
            if let Err(e) = res {
                error!("TCP server failed: {e}");
            }
        },
        res = image_unix_socket_server => {
             if let Err(e) = res {
                error!("Image unix socket server failed: {e}");
            }
        },
        res = task_unix_socket_server => {
             if let Err(e) = res {
                error!("Task unix socket server failed: {e}");
            }
        },
        Some(RestartSignal(new_binary_path)) = restart_rx.recv() => {
            if let Err(e) = handle_upgrade(&new_binary_path) {
                error!("Upgrade failed: {e}");
            }
        }
    };

    Ok(())
}
