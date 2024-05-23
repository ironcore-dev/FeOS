use log::info;
use nix::errno::Errno;
use nix::ifaddrs::getifaddrs;
use nix::sys::sysinfo::sysinfo;
use nix::unistd::sysconf;
use nix::unistd::SysconfVar;
use std::fs;

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
    let path = format!("/sys/class/net/{}/device", interface_name);
    if let Ok(device_path) = fs::read_link(path) {
        info!("device path: {:?}", device_path);
        let pci_address = device_path.file_name()?.to_str()?.to_string();
        return Some(pci_address);
    }
    None
}

fn get_mac_address(interface_name: &str) -> Option<String> {
    let path = format!("/sys/class/net/{}/address", interface_name);
    if let Ok(mac) = fs::read_to_string(path) {
        return Some(mac.trim().to_string());
    } else {
        None
    }
}

fn get_interfaces() -> Result<Vec<Interface>, Errno> {
    let mut interfaces = Vec::new();

    let ifaddrs = getifaddrs()?;
    for ifaddr in ifaddrs {
        info!("Got host info request {:?}", ifaddr);
        let interface = Interface {
            name: ifaddr.interface_name.clone(),
            pci_address: get_pci_address(&ifaddr.interface_name),
            mac_address: get_mac_address(&ifaddr.interface_name),
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
            Err(err) => info!("Error getting number of CPU cores: {}", err),
        },
        Ok(None) => (),
        Err(err) => info!("Error getting number of CPU cores: {}", err),
    }

    match sysinfo() {
        Ok(info) => {
            host.uptime = info.uptime().as_secs();
            host.ram_total = info.ram_total();
            host.ram_unused = info.ram_unused();
        }
        Err(err) => info!("Error getting sysinfo: {}", err),
    }

    match get_interfaces() {
        Ok(ifs) => {
            host.net_interfaces = ifs;
        }
        Err(err) => info!("Error getting network interfaces: {}", err),
    }

    host
}
