// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use super::fsmount::{fsconfig, fsmount, fsopen, FSCONFIG_CMD_CREATE, FSCONFIG_SET_STRING};
use nix::{
    fcntl::{openat, OFlag},
    mount::{mount, MsFlags},
    sys::stat::{mknod, stat, Mode, SFlag},
    unistd::{chroot, fchdir},
};
use std::{
    fs::{copy, create_dir, read_dir, read_link, remove_dir, remove_file, File},
    io::{self, BufRead, BufReader},
    os::{
        fd::{AsFd, AsRawFd, FromRawFd, OwnedFd},
        unix::fs::{symlink, FileTypeExt},
    },
    path::Path,
};

#[allow(unsafe_code)]
pub fn get_root_fstype() -> Result<String, Box<dyn std::error::Error>> {
    let proc_fs_raw = fsopen("proc", 0)?;
    let proc_fs = unsafe { OwnedFd::from_raw_fd(proc_fs_raw) };

    fsconfig(proc_fs.as_fd(), FSCONFIG_CMD_CREATE, None, None, 0)?;

    let proc_dir_raw = fsmount(proc_fs.as_fd(), 0, 0)?;
    let proc_dir = unsafe { OwnedFd::from_raw_fd(proc_dir_raw) };

    let mounts_raw = openat(
        Some(proc_dir.as_raw_fd()),
        "mounts",
        OFlag::O_RDONLY,
        Mode::empty(),
    )?;
    let mounts = unsafe { File::from_raw_fd(mounts_raw) };

    let reader = BufReader::new(mounts);

    for line in reader.lines() {
        let line = line?;
        let fields: Vec<&str> = line.split_whitespace().collect();

        if fields.len() >= 3 && fields[1] == "/" {
            return Ok(fields[2].to_string());
        }
    }

    Err("could not determine root fstype".into())
}

#[allow(unsafe_code)]
pub fn move_root() -> Result<(), Box<dyn std::error::Error>> {
    let tmp_fs_raw = fsopen("tmpfs", 0)?;
    let tmp_fs = unsafe { OwnedFd::from_raw_fd(tmp_fs_raw) };

    fsconfig(
        tmp_fs.as_fd(),
        FSCONFIG_SET_STRING,
        Some("mode"),
        Some("0755"),
        0,
    )?;
    fsconfig(tmp_fs.as_fd(), FSCONFIG_CMD_CREATE, None, None, 0)?;

    let tmp_dir_raw = fsmount(tmp_fs.as_fd(), 0, 0)?;
    let tmp_dir = unsafe { OwnedFd::from_raw_fd(tmp_dir_raw) };

    fchdir(tmp_dir.as_raw_fd())?;
    move_recursively(Path::new("/"), Path::new("."))?;

    mount(
        Some("." as &str),
        "/" as &str,
        None::<&str>,
        MsFlags::MS_MOVE,
        None::<&str>,
    )?;
    chroot(".")?;

    Ok(())
}

fn move_recursively(source: impl AsRef<Path>, destination: impl AsRef<Path>) -> io::Result<()> {
    for entry in read_dir(source)? {
        let entry = entry?;
        let filetype = entry.file_type()?;

        let source_path = entry.path();
        let destination_path = destination.as_ref().join(entry.file_name());

        if filetype.is_dir() {
            create_dir(destination_path.as_path())?;
            move_recursively(source_path.as_path(), destination_path.as_path())?;
            remove_dir(source_path.as_path())?;
        } else {
            if filetype.is_file() {
                copy(source_path.as_path(), destination_path.as_path())?;
            } else if filetype.is_symlink() {
                let target = read_link(source_path.as_path())?;
                symlink(target, destination_path.as_path())?;
            } else if filetype.is_char_device() || filetype.is_block_device() {
                let source_stat = stat(source_path.as_path())?;
                mknod(
                    destination_path.as_path(),
                    SFlag::from_bits_truncate(source_stat.st_mode),
                    Mode::from_bits_truncate(source_stat.st_mode),
                    source_stat.st_rdev,
                )?;
            }
            remove_file(source_path)?;
        }
    }

    Ok(())
}
