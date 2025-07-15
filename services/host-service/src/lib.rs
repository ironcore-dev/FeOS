use proto_definitions::host_service::{HostnameResponse, UpgradeRequest, UpgradeResponse};
use std::path::PathBuf;
use tokio::sync::oneshot;
use tonic::{Status, Streaming};

pub mod api;
pub mod dispatcher;
pub mod worker;

#[derive(Debug)]
pub enum Command {
    GetHostname(oneshot::Sender<Result<HostnameResponse, Status>>),
    UpgradeFeosBinary(
        Box<Streaming<UpgradeRequest>>,
        oneshot::Sender<Result<UpgradeResponse, Status>>,
    ),
}

#[derive(Debug)]
pub struct RestartSignal(pub PathBuf);
