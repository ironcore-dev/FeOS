use log::info;
use nix::errno::Errno;
use nix::sys::sysinfo::sysinfo;
use nix::unistd::sysconf;
use nix::unistd::SysconfVar;
use std::fs;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

#[derive(Default)]
pub struct HostInfo {
    pub uptime: u64,
    pub ram_total: u64,
    pub ram_unused: u64,
    pub num_cores: u64,
    pub net_interfaces: Vec<Interface>,
}

#[derive(Default)]
pub struct Interface {
    pub name: String,
    pub pci_address: Option<String>,
    pub mac_address: Option<String>,
}

fn get_pci_address(interface_name: &str) -> Option<String> {
    let path = format!("/sys/class/net/{interface_name}/device");
    if let Ok(device_path) = fs::read_link(path) {
        let pci_address = device_path.file_name()?.to_str()?.to_string();
        return Some(pci_address);
    }
    None
}

fn get_mac_address(interface_name: &str) -> Option<String> {
    let path = format!("/sys/class/net/{interface_name}/address");
    if let Ok(mac) = fs::read_to_string(path) {
        Some(mac.trim().to_string())
    } else {
        None
    }
}

fn get_interfaces() -> Result<Vec<Interface>, Errno> {
    let mut interfaces = Vec::new();
    let ifaces = nix::net::if_::if_nameindex()?;
    for iface in &ifaces {
        info!("found network interface: {iface:?}");
        let name = iface.name().to_str().unwrap();
        let interface = Interface {
            name: name.to_string(),
            pci_address: get_pci_address(name),
            mac_address: get_mac_address(name),
        };

        interfaces.push(interface)
    }

    Ok(interfaces)
}

pub fn check_info() -> HostInfo {
    let mut host: HostInfo = HostInfo::default();
    match sysconf(SysconfVar::_NPROCESSORS_ONLN) {
        Ok(Some(num_cores)) => match u64::try_from(num_cores) {
            Ok(num_cores) => host.num_cores = num_cores,
            Err(err) => info!("Error getting number of CPU cores: {err}"),
        },
        Ok(None) => (),
        Err(err) => info!("Error getting number of CPU cores: {err}"),
    }

    match sysinfo() {
        Ok(info) => {
            host.uptime = info.uptime().as_secs();
            host.ram_total = info.ram_total();
            host.ram_unused = info.ram_unused();
        }
        Err(err) => info!("Error getting sysinfo: {err}"),
    }

    match get_interfaces() {
        Ok(ifs) => {
            host.net_interfaces = ifs;
        }
        Err(err) => info!("Error getting network interfaces: {err}"),
    }

    host
}

pub async fn is_running_on_vm() -> Result<bool, Box<dyn std::error::Error>> {
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
