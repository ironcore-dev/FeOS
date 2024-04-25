extern crate nix;
mod daemon;
mod filesystem;
mod network;
mod vm;
mod dhcpv6;

use crate::daemon::daemon_start;
use crate::filesystem::mount_virtual_filesystems;
use crate::network::configure_network_devices;

use log::{error, info, warn, LevelFilter};
use nix::unistd::Uid;
use simple_logger::SimpleLogger;

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

    SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .with_module_level("feos::filesystem", LevelFilter::Debug)
        .with_utc_timestamps()
        .init()
        .unwrap();

    // if not run as root, print warning.
    if !Uid::current().is_root() {
        warn!("Not running as root! (uid: {})", Uid::current());
    }

    // Special stuff for pid 1
    if std::process::id() == 1 {
        info!("Mounting virtual filesystems...");
        mount_virtual_filesystems();
    }
    /*
        info!("Configuring network devices...");
        configure_network_devices().await.map_err(|e| {
            error!("could not configure network devices: {}", e);
            "fuck".to_string()
        })?;
    */
    info!("Configuring network devices...");
    configure_network_devices()
        .await
        .expect("could not configure network devices");


    let vmm =  vm::Manager::new(
        String::from("cloud-hypervisor"),
    );

    info!("Starting FeOS daemon...");
    match daemon_start(vmm).await {
        Err(e) => error!("FeOS daemon crashed: {}", e),
        _ => error!("FeOS daemon exited."),
    }
    Err("FeOS exited".to_string())
}
