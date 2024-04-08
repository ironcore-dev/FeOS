use log::debug;
#[cfg(target_os = "linux")]
use nix::mount::{mount, MsFlags};

#[cfg(not(target_os = "linux"))]
pub fn mount_virtual_filesystems() {
    debug!("Skip mount if other than linux");
}

#[cfg(target_os = "linux")]
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
}