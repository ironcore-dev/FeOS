use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use digest::Digest;
use proto_definitions::host_service::{
    host_service_client::HostServiceClient, upgrade_request, Empty, UpgradeMetadata, UpgradeRequest,
};
use sha2::Sha256;
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel;

#[derive(Args, Debug)]
pub struct HostArgs {
    #[arg(
        short,
        long,
        global = true,
        env = "FEOS_ADDRESS",
        default_value = "http://[::1]:1337"
    )]
    pub address: String,

    #[command(subcommand)]
    command: HostCommand,
}

#[derive(Subcommand, Debug)]
pub enum HostCommand {
    Hostname,
    Upgrade {
        #[arg(required = true)]
        binary_path: PathBuf,
    },
}

pub async fn handle_host_command(args: HostArgs) -> Result<()> {
    let mut client = HostServiceClient::connect(args.address)
        .await
        .context("Failed to connect to host service")?;

    match args.command {
        HostCommand::Hostname => get_hostname(&mut client).await?,
        HostCommand::Upgrade { binary_path } => upgrade_feos(&mut client, binary_path).await?,
    }

    Ok(())
}

async fn get_hostname(client: &mut HostServiceClient<Channel>) -> Result<()> {
    let request = Empty {};
    let response = client.hostname(request).await?.into_inner();
    println!("{}", response.hostname);
    Ok(())
}

async fn upgrade_feos(client: &mut HostServiceClient<Channel>, binary_path: PathBuf) -> Result<()> {
    if !binary_path.exists() {
        anyhow::bail!("Binary file not found at: {}", binary_path.display());
    }

    println!("Calculating checksum for {}...", binary_path.display());
    let mut file_for_hash = File::open(&binary_path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 8192];
    while let Ok(n) = file_for_hash.read(&mut buffer).await {
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    let checksum = hex::encode(hasher.finalize());
    println!("Checksum (SHA256): {checksum}");

    let (tx, rx) = mpsc::channel(4);
    let request_stream = ReceiverStream::new(rx);

    let upload_task = tokio::spawn(async move {
        let metadata = UpgradeMetadata {
            sha256_sum: checksum,
        };
        let metadata_req = UpgradeRequest {
            payload: Some(upgrade_request::Payload::Metadata(metadata)),
        };
        if tx.send(metadata_req).await.is_err() {
            return;
        }

        let mut file = match File::open(&binary_path).await {
            Ok(file) => file,
            Err(e) => {
                eprintln!("Failed to open file for upload: {e}");
                return;
            }
        };

        loop {
            let mut chunk_buf = vec![0; 1024 * 64];
            match file.read(&mut chunk_buf).await {
                Ok(0) => break,
                Ok(n) => {
                    chunk_buf.truncate(n);
                    let chunk_req = UpgradeRequest {
                        payload: Some(upgrade_request::Payload::Chunk(chunk_buf)),
                    };
                    if tx.send(chunk_req).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("Error reading from file: {e}");
                    break;
                }
            }
        }
    });

    println!("Uploading new binary to host...");
    let response = client
        .upgrade_feos_binary(request_stream)
        .await?
        .into_inner();

    println!("Server response: {}", response.message);

    upload_task.await?;

    Ok(())
}
