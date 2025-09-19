// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;
use feos_utils::filesystem::{get_root_fstype, move_root};
use main_server::run_server;
use nix::unistd::execv;
use std::env;
use std::ffi::CString;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct ServerArgs {
    #[arg(long, hide = true)]
    restarted_after_upgrade: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = ServerArgs::parse();

    if std::process::id() == 1 {
        let root_fstype = get_root_fstype().unwrap_or_else(|e| {
            eprintln!("[feos] Failed to get root fstype: {e}");
            String::new()
        });

        if root_fstype == "rootfs" {
            move_root().map_err(|e| anyhow::anyhow!("[feos] move_root failed: {}", e))?;

            let argv: Vec<CString> = env::args()
                .map(|arg| CString::new(arg).unwrap_or_default())
                .collect();
            let _ = execv(&argv[0], &argv)
                .map_err(|e| anyhow::anyhow!("[feos] execv failed: {}", e))?;

            return Err(anyhow::anyhow!("execv failed to replace process"));
        }
    }

    run_server(args.restarted_after_upgrade).await
}
