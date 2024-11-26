use log::debug;
use nix::mount::{mount, MsFlags};
use std::fs::File;
use std::io::{Error, Write};

pub fn mount_virtual_filesystems() {
    const NONE: Option<&'static [u8]> = None;

    debug!("Mounting /proc");
    mount(
        Some(b"proc".as_ref()),
        "/proc",
        Some(b"proc".as_ref()),
        MsFlags::empty(),
        NONE,
    )
    .unwrap_or_else(|e| panic!("/proc mount failed: {e}"));

    debug!("Mounting /sys");
    mount(
        Some(b"sys".as_ref()),
        "/sys",
        Some(b"sysfs".as_ref()),
        MsFlags::empty(),
        NONE,
    )
    .unwrap_or_else(|e| panic!("/sys mount failed: {e}"));

    debug!("Mounting /dev");
    mount(
        Some(b"devtmpfs".as_ref()),
        "/dev",
        Some(b"devtmpfs".as_ref()),
        MsFlags::empty(),
        NONE,
    )
    .unwrap_or_else(|e| panic!("/dev mount failed: {e}"));

    debug!("Mounting /var/lib/feos");
    mount(
        Some(b"tmpfs".as_ref()),
        "/var/lib/feos",
        Some(b"tmpfs".as_ref()),
        MsFlags::empty(),
        NONE,
    )
    .unwrap_or_else(|e| panic!("/var/lib/feos mount failed: {e}"));

    debug!("Mounting /sys/fs/cgroup");
    mount(
        Some(b"cgroup2".as_ref()),
        "/sys/fs/cgroup",
        Some(b"cgroup2".as_ref()),
        MsFlags::empty(),
        NONE,
    )
    .unwrap_or_else(|e| panic!("/sys/fs/cgroup mount failed: {e}"));

    enable_ipv6_forwarding().unwrap_or_else(|e| panic!("Failed to enable ipv6 forwarding: {e}"));
}

fn enable_ipv6_forwarding() -> Result<(), Error> {
    let forwarding_paths = ["/proc/sys/net/ipv6/conf/all/forwarding"];

    for path in forwarding_paths {
        let mut file = File::create(path)?;
        file.write_all(b"1")?;
    }

    Ok(())
}
