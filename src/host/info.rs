use log::info;
use nix::sys::sysinfo::sysinfo;
use nix::unistd::sysconf;
use nix::unistd::SysconfVar;

#[derive(Default)]
pub struct HostInfo {
    pub uptime: u64,
    pub ram_total: u64,
    pub ram_unused: u64,
    pub num_cores: u64,
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

    host
}
