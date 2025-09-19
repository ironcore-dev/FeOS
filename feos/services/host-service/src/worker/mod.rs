// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

pub mod info;
pub mod ops;
pub mod power;

pub use info::{
    handle_get_cpu_info, handle_get_memory, handle_get_network_info, handle_get_version_info,
    handle_hostname,
};
pub use ops::{handle_stream_feos_logs, handle_stream_kernel_logs, handle_upgrade};
pub use power::{handle_reboot, handle_shutdown};
