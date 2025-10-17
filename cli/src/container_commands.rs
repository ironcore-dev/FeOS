// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use feos_proto::container_service::{
    container_service_client::ContainerServiceClient, ContainerConfig, ContainerState,
    CreateContainerRequest, DeleteContainerRequest, GetContainerRequest, ListContainersRequest,
    StartContainerRequest, StopContainerRequest,
};
use tonic::transport::Channel;

#[derive(Args, Debug)]
pub struct ContainerArgs {
    #[arg(
        short,
        long,
        global = true,
        env = "FEOS_ADDRESS",
        default_value = "http://[::1]:1337"
    )]
    pub address: String,

    #[command(subcommand)]
    command: ContainerCommand,
}

#[derive(Subcommand, Debug)]
pub enum ContainerCommand {
    /// Create a new container
    Create {
        #[arg(
            long,
            required = true,
            help = "Container image reference (e.g., docker.io/library/alpine:latest)"
        )]
        image_ref: String,

        #[arg(long, help = "Optional custom container identifier (UUID)")]
        id: Option<String>,

        #[arg(
            long,
            help = "Override the default command of the image",
            num_args = 1..,
            value_delimiter = ' '
        )]
        cmd: Vec<String>,

        #[arg(
            long,
            help = "Set environment variables (e.g., --env KEY1=VALUE1 --env KEY2=VALUE2)",
            value_parser = parse_key_val
        )]
        env: Vec<(String, String)>,
    },
    /// Start a created container
    Start {
        #[arg(required = true, help = "Container identifier")]
        id: String,
    },
    /// Stop a running container
    Stop {
        #[arg(required = true, help = "Container identifier")]
        id: String,
    },
    /// Get detailed information about a container
    Info {
        #[arg(required = true, help = "Container identifier")]
        id: String,
    },
    /// List all containers
    List,
    /// Delete a container
    Delete {
        #[arg(required = true, help = "Container identifier")]
        id: String,
    },
}

fn parse_key_val(s: &str) -> Result<(String, String), String> {
    s.split_once('=')
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .ok_or_else(|| format!("invalid KEY=value format: {s}"))
}

pub async fn handle_container_command(args: ContainerArgs) -> Result<()> {
    let mut client = ContainerServiceClient::connect(args.address)
        .await
        .context("Failed to connect to container service")?;

    match args.command {
        ContainerCommand::Create {
            image_ref,
            id,
            cmd,
            env,
        } => create_container(&mut client, image_ref, id, cmd, env).await?,
        ContainerCommand::Start { id } => start_container(&mut client, id).await?,
        ContainerCommand::Stop { id } => stop_container(&mut client, id).await?,
        ContainerCommand::Info { id } => get_container_info(&mut client, id).await?,
        ContainerCommand::List => list_containers(&mut client).await?,
        ContainerCommand::Delete { id } => delete_container(&mut client, id).await?,
    }

    Ok(())
}

async fn create_container(
    client: &mut ContainerServiceClient<Channel>,
    image_ref: String,
    id: Option<String>,
    cmd: Vec<String>,
    env: Vec<(String, String)>,
) -> Result<()> {
    println!("Requesting container creation with image: {image_ref}...");

    let config = ContainerConfig {
        image_ref,
        command: cmd,
        env: env.into_iter().collect(),
    };

    let request = CreateContainerRequest {
        config: Some(config),
        container_id: id,
    };

    let response = client.create_container(request).await?.into_inner();
    println!(
        "Container creation initiated. Container ID: {}",
        response.container_id
    );
    println!(
        "Use 'feos-cli container list' to check its status and 'feos-cli container start {}' to run it.",
        response.container_id
    );

    Ok(())
}

async fn start_container(client: &mut ContainerServiceClient<Channel>, id: String) -> Result<()> {
    println!("Requesting to start container: {id}...");
    let request = StartContainerRequest {
        container_id: id.clone(),
    };
    client.start_container(request).await?;
    println!("Start request sent for container: {id}");
    Ok(())
}

async fn stop_container(client: &mut ContainerServiceClient<Channel>, id: String) -> Result<()> {
    println!("Requesting to stop container: {id}...");
    let request = StopContainerRequest {
        container_id: id.clone(),
        ..Default::default()
    };
    client.stop_container(request).await?;
    println!("Stop request sent for container: {id}");
    Ok(())
}

async fn get_container_info(
    client: &mut ContainerServiceClient<Channel>,
    id: String,
) -> Result<()> {
    let request = GetContainerRequest {
        container_id: id.clone(),
    };
    let response = client.get_container(request).await?.into_inner();

    println!("Container Info for: {id}");
    println!(
        "  State: {:?}",
        ContainerState::try_from(response.state).unwrap_or(ContainerState::Unspecified)
    );
    if let Some(pid) = response.pid {
        println!("  PID: {pid}");
    }
    if let Some(exit_code) = response.exit_code {
        println!("  Exit Code: {exit_code}");
    }
    if let Some(config) = response.config {
        println!("  Config:");
        println!("    Image Ref: {}", config.image_ref);
        if !config.command.is_empty() {
            println!("    Command: {:?}", config.command);
        }
        if !config.env.is_empty() {
            println!("    Env: {:?}", config.env);
        }
    }

    Ok(())
}

async fn list_containers(client: &mut ContainerServiceClient<Channel>) -> Result<()> {
    let request = ListContainersRequest {};
    let response = client.list_containers(request).await?.into_inner();

    if response.containers.is_empty() {
        println!("No containers found.");
        return Ok(());
    }

    println!("{:<38} {:<15} IMAGE_REF", "CONTAINER_ID", "STATE");
    println!("{:-<38} {:-<15} {:-<40}", "", "", "");
    for container in response.containers {
        let state =
            ContainerState::try_from(container.state).unwrap_or(ContainerState::Unspecified);
        let image_ref = container
            .config
            .map(|c| c.image_ref)
            .unwrap_or_else(|| "N/A".to_string());
        println!(
            "{:<38} {:<15} {}",
            container.container_id,
            format!("{:?}", state),
            image_ref
        );
    }
    Ok(())
}

async fn delete_container(client: &mut ContainerServiceClient<Channel>, id: String) -> Result<()> {
    println!("Requesting to delete container: {id}...");
    let request = DeleteContainerRequest {
        container_id: id.clone(),
    };
    client.delete_container(request).await?;
    println!("Successfully deleted container: {id}");
    Ok(())
}
