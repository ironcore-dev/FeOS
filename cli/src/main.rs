// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::{Parser, Subcommand};

mod host_commands;
mod image_commands;
mod vm_commands;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    service: Service,
}

#[derive(Subcommand, Debug)]
enum Service {
    Vm(vm_commands::VmArgs),
    Host(host_commands::HostArgs),
    Image(image_commands::ImageArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Warn)
        .parse_default_env()
        .init();

    let cli = Cli::parse();

    match cli.service {
        Service::Vm(args) => vm_commands::handle_vm_command(args).await?,
        Service::Host(args) => host_commands::handle_host_command(args).await?,
        Service::Image(args) => image_commands::handle_image_command(args).await?,
    }

    Ok(())
}
