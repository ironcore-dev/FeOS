// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

mod fsmount;
mod mount;
mod r#move;

pub use mount::mount_virtual_filesystems;
pub use r#move::{get_root_fstype, move_root};
