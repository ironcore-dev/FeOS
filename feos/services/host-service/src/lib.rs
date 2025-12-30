// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use crate::error::HostError;
use feos_proto::host_service::{
    FeosLogEntry, GetCpuInfoResponse, GetKernelStatsResponse, GetNetworkInfoResponse,
    GetVersionInfoResponse, HostnameResponse, KernelLogEntry, MemoryResponse, RebootRequest,
    RebootResponse, ShutdownRequest, ShutdownResponse, UpgradeFeosBinaryRequest,
    UpgradeFeosBinaryResponse,
};
use std::path::PathBuf;
use tokio::sync::{mpsc, oneshot};
use tonic::Status;

pub mod api;
pub mod dispatcher;
pub mod error;
pub mod worker;

#[derive(Debug)]
pub enum Command {
    GetHostname(oneshot::Sender<Result<HostnameResponse, HostError>>),
    GetMemory(oneshot::Sender<Result<MemoryResponse, HostError>>),
    GetCPUInfo(oneshot::Sender<Result<GetCpuInfoResponse, HostError>>),
    GetKernelStats(oneshot::Sender<Result<GetKernelStatsResponse, HostError>>),
    GetNetworkInfo(oneshot::Sender<Result<GetNetworkInfoResponse, HostError>>),
    GetVersionInfo(oneshot::Sender<Result<GetVersionInfoResponse, HostError>>),
    UpgradeFeosBinary(
        UpgradeFeosBinaryRequest,
        oneshot::Sender<Result<UpgradeFeosBinaryResponse, Status>>,
    ),
    StreamKernelLogs(mpsc::Sender<Result<KernelLogEntry, Status>>),
    StreamFeOSLogs(mpsc::Sender<Result<FeosLogEntry, Status>>),
    Shutdown(
        ShutdownRequest,
        oneshot::Sender<Result<ShutdownResponse, HostError>>,
    ),
    Reboot(
        RebootRequest,
        oneshot::Sender<Result<RebootResponse, HostError>>,
    ),
}

#[derive(Debug)]
pub struct RestartSignal(pub PathBuf);
