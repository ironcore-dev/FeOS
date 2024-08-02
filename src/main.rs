extern crate nix;
mod container;
mod daemon;
mod dhcpv6;
mod filesystem;
mod fsmount;
mod host;
mod move_root;
mod network;
mod ringbuffer;
mod vm;

use std::{env::args, ffi::CString};

use crate::daemon::daemon_start;
use crate::filesystem::mount_virtual_filesystems;
use crate::network::configure_network_devices;

use log::{error, info, warn};
use move_root::{get_root_fstype, move_root};
use network::configure_sriov;
use nix::unistd::{execv, Uid};
use ringbuffer::*;

use tokio::fs::File;
use tokio::io::AsyncReadExt;

#[tokio::main]
async fn main() -> Result<(), String> {
    if std::process::id() == 1 {
        let root_fstype = get_root_fstype().unwrap_or_default();
        if root_fstype == "rootfs" {
            move_root().map_err(|e| format!("move_root: {}", e))?;

            let argv: Vec<CString> = args()
                .map(|arg| CString::new(arg).unwrap_or_default())
                .collect();
            execv(&argv[0], &argv).map_err(|e| format!("execv: {}", e))?;
        }
    }

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

    let is_nested = match is_running_on_vm().await {
        Ok(result) => result,
        Err(e) => {
            error!("Error checking VM status: {}", e);
            false // Default to false in case of error
        }
    };

    let vmm = vm::Manager::new(String::from("cloud-hypervisor"), is_nested);

    info!("Starting FeOS daemon...");
    match daemon_start(vmm, buffer, log_receiver, is_nested).await {
        Err(e) => error!("FeOS daemon crashed: {}", e),
        _ => error!("FeOS daemon exited."),
    }
    Err("FeOS exited".to_string())
}

async fn is_running_on_vm() -> Result<bool, Box<dyn std::error::Error>> {
    let files = [
        "/sys/class/dmi/id/product_name",
        "/sys/class/dmi/id/sys_vendor",
    ];

    let mut match_count = 0;

    for file_path in files.iter() {
        let mut file = File::open(file_path).await?;
        let mut contents = String::new();
        file.read_to_string(&mut contents).await?;

        let lowercase_contents = contents.to_lowercase();
        if lowercase_contents.contains("cloud") && lowercase_contents.contains("hypervisor") {
            match_count += 1;
        }
    }

    Ok(match_count == 2)
}
