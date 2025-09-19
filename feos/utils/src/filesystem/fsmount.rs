// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use libc::{syscall, SYS_fsconfig, SYS_fsmount, SYS_fsopen};
use nix::{errno::Errno, Result};
use std::{
    ffi::CString,
    os::fd::{AsRawFd, BorrowedFd, RawFd},
    ptr::null,
};

#[allow(unsafe_code)]
pub fn fsopen(fsname: &str, flags: u32) -> Result<RawFd> {
    let fsname_cstring = CString::new(fsname).unwrap_or_default();
    let ret = unsafe { syscall(SYS_fsopen, fsname_cstring.as_ptr(), flags) };
    Errno::result(ret.try_into().unwrap())
}

#[allow(unsafe_code)]
pub fn fsconfig(
    fd: BorrowedFd,
    cmd: u32,
    key: Option<&str>,
    value: Option<&str>,
    aux: i32,
) -> Result<RawFd> {
    let key_cstring = key.map(|key| CString::new(key).unwrap_or_default());
    let value_cstring = value.map(|value| CString::new(value).unwrap_or_default());

    let key_ptr = match key_cstring.as_ref() {
        Some(key) => key.as_ptr(),
        None => null(),
    };
    let value_ptr = match value_cstring.as_ref() {
        Some(value) => value.as_ptr(),
        None => null(),
    };

    let ret = unsafe { syscall(SYS_fsconfig, fd.as_raw_fd(), cmd, key_ptr, value_ptr, aux) };

    Errno::result(ret.try_into().unwrap())
}

pub const FSCONFIG_SET_STRING: u32 = 1;
pub const FSCONFIG_CMD_CREATE: u32 = 6;

#[allow(unsafe_code)]
pub fn fsmount(fd: BorrowedFd, flags: u32, mount_attr: u32) -> Result<RawFd> {
    let ret = unsafe { syscall(SYS_fsmount, fd.as_raw_fd(), flags, mount_attr) };
    Errno::result(ret.try_into().unwrap())
}
