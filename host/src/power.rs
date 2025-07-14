use nix::sys::reboot::RebootMode;
use std::convert::Infallible;

pub fn reboot() -> Result<Infallible, nix::errno::Errno> {
    nix::sys::reboot::reboot(RebootMode::RB_AUTOBOOT)
}

pub fn shutdown() -> Result<Infallible, nix::errno::Errno> {
    nix::sys::reboot::reboot(RebootMode::RB_POWER_OFF)
}