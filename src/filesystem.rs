use log::debug;
use nix::mount::{mount, MsFlags};

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
}
