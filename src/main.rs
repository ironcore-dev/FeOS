extern crate nix;
mod daemon;
mod dhcpv6;
mod filesystem;
mod host;
mod network;
mod ringbuffer;
mod vm;

use crate::daemon::daemon_start;
use crate::filesystem::mount_virtual_filesystems;
use crate::network::configure_network_devices;

use log::{error, info, warn};
use network::configure_sriov;
use nix::unistd::Uid;
use ringbuffer::*;

#[tokio::main]
async fn main() -> Result<(), String> {
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
        env!("CARGO_PKG_VERSION")
    );

    const FEOS_RINGBUFFER_CAP: usize = 100;
    let buffer = RingBuffer::new(FEOS_RINGBUFFER_CAP);
    let log_receiver = init_logger(buffer.clone());

    // if not run as root, print warning.
    if !Uid::current().is_root() {
        warn!("Not running as root! (uid: {})", Uid::current());
    }

    // Special stuff for pid 1
    if std::process::id() == 1 {
        info!("Mounting virtual filesystems...");
        mount_virtual_filesystems();

        info!("Configuring network devices...");
        configure_network_devices()
            .await
            .expect("could not configure network devices");

        info!("Configuring sriov...");
        const VFS_NUM: u32 = 125;
        if let Err(e) = configure_sriov(VFS_NUM).await {
            warn!("failed to configure sriov: {}", e.to_string())
        }
    }

    let vmm = vm::Manager::new(String::from("cloud-hypervisor"));

    info!("Starting FeOS daemon...");
    match daemon_start(vmm, buffer, log_receiver).await {
        Err(e) => error!("FeOS daemon crashed: {}", e),
        _ => error!("FeOS daemon exited."),
    }
    Err("FeOS exited".to_string())
}
