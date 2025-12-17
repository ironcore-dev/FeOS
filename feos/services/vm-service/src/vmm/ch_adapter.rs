// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use super::{Hypervisor, VmmError};
use crate::{VmEventWrapper, IMAGE_DIR, VM_API_SOCKET_DIR, VM_CONSOLE_DIR};
use cloud_hypervisor_client::{
    apis::{configuration::Configuration, DefaultApi, DefaultApiClient},
    models::{
        self, console_config::Mode as ConsoleMode, vm_info::State as ChVmState,
        VmmPingResponse as ChPingResponse,
    },
};
use feos_proto::vm_service::{
    net_config, AttachDiskRequest, AttachDiskResponse, AttachNicRequest, AttachNicResponse,
    CreateVmRequest, DeleteVmRequest, DeleteVmResponse, DetachDiskRequest, DetachDiskResponse,
    DetachNicRequest, DetachNicResponse, GetVmRequest, PauseVmRequest, PauseVmResponse,
    PingVmRequest, PingVmResponse, ResumeVmRequest, ResumeVmResponse, ShutdownVmRequest,
    ShutdownVmResponse, StartVmRequest, StartVmResponse, VmConfig, VmInfo, VmState,
};
use hyper_util::client::legacy::Client;
use hyperlocal::{UnixClientExt, UnixConnector, Uri as HyperlocalUri};
use log::{error, info, warn};
use nix::sys::signal::{kill, Signal};
use nix::unistd::{self, Pid};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::process::Command as TokioCommand;
use tokio::sync::{broadcast, mpsc};
use tokio::time::{self, timeout, Duration};
use uuid::Uuid;

#[derive(Debug)]
pub enum ChNetworkDevice {
    Net(Box<models::NetConfig>),
    Device(models::DeviceConfig),
}

impl ChNetworkDevice {
    pub fn id(&self) -> Option<String> {
        match self {
            ChNetworkDevice::Net(config) => config.id.clone(),
            ChNetworkDevice::Device(config) => config.id.clone(),
        }
    }
}

fn convert_net_config_to_ch(
    nic: &feos_proto::vm_service::NetConfig,
) -> Result<ChNetworkDevice, VmmError> {
    match &nic.backend {
        Some(net_config::Backend::Tap(tap)) => {
            let id = if !nic.device_id.is_empty() {
                Some(nic.device_id.clone())
            } else {
                Some(tap.tap_name.clone())
            };

            let mac = if nic.mac_address.is_empty() {
                None
            } else {
                Some(nic.mac_address.clone())
            };

            let ch_net_config = models::NetConfig {
                tap: Some(tap.tap_name.clone()),
                mac,
                id,
                ..Default::default()
            };
            Ok(ChNetworkDevice::Net(Box::new(ch_net_config)))
        }
        Some(net_config::Backend::VfioPci(vfio_pci)) => {
            let device_path = format!("/sys/bus/pci/devices/{}", vfio_pci.bdf);
            let id = if !nic.device_id.is_empty() {
                Some(nic.device_id.clone())
            } else {
                Some(device_path.clone())
            };

            let ch_device_config = models::DeviceConfig {
                path: device_path,
                id,
                ..Default::default()
            };
            Ok(ChNetworkDevice::Device(ch_device_config))
        }
        None => Err(VmmError::InvalidConfig(
            "NetConfig backend (tap or vfio_pci) is required".to_string(),
        )),
    }
}

pub struct CloudHypervisorAdapter {
    ch_binary_path: PathBuf,
}

impl CloudHypervisorAdapter {
    pub fn new(ch_binary_path: PathBuf) -> Self {
        Self { ch_binary_path }
    }

    fn get_ch_api_client(&self, vm_id: &str) -> Result<DefaultApiClient<UnixConnector>, VmmError> {
        let socket_path = PathBuf::from(VM_API_SOCKET_DIR).join(vm_id);
        if !socket_path.exists() {
            return Err(VmmError::VmNotFound(vm_id.to_string()));
        }

        let uri: hyper::Uri = HyperlocalUri::new(socket_path, "/api/v1").into();
        let client = Client::unix();

        let configuration = Configuration {
            base_path: uri.to_string(),
            client,
            user_agent: Some("FeOS-vm-service/1.0".to_string()),
            basic_auth: None,
            oauth_access_token: None,
            api_key: None,
        };

        Ok(DefaultApiClient::new(Arc::new(configuration)))
    }
    async fn perform_vm_creation(
        &self,
        vm_id: &str,
        config: VmConfig,
        image_uuid: String,
        api_socket_path: &Path,
    ) -> Result<(), VmmError> {
        let wait_for_socket = async {
            while !api_socket_path.exists() {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        };

        if timeout(Duration::from_secs(5), wait_for_socket)
            .await
            .is_err()
        {
            return Err(VmmError::ApiConnectionFailed(
                "Timed out waiting for API socket".to_string(),
            ));
        }
        info!("CloudHypervisorAdapter ({vm_id}): API socket is available.");

        let client = self.get_ch_api_client(vm_id)?;
        tokio::fs::create_dir_all(VM_CONSOLE_DIR)
            .await
            .map_err(|e| VmmError::Internal(format!("Failed to create console dir: {e}")))?;

        let rootfs_path_str = format!("{IMAGE_DIR}/{image_uuid}/disk.image");
        let console_socket_path = format!("{VM_CONSOLE_DIR}/{vm_id}.console");

        let mut ch_vm_config = models::VmConfig {
            payload: models::PayloadConfig {
                firmware: Some("/usr/share/cloud-hypervisor/hypervisor-fw".to_string()),
                ..Default::default()
            },
            disks: Some(vec![models::DiskConfig {
                path: Some(rootfs_path_str),
                ..Default::default()
            }]),
            serial: Some(models::ConsoleConfig {
                socket: Some(console_socket_path),
                mode: ConsoleMode::Socket,
                ..Default::default()
            }),
            console: Some(models::ConsoleConfig {
                mode: ConsoleMode::Off,
                ..Default::default()
            }),
            ..Default::default()
        };

        if let Some(cpus) = config.cpus {
            ch_vm_config.cpus = Some(models::CpusConfig {
                boot_vcpus: cpus.boot_vcpus as i32,
                max_vcpus: cpus.max_vcpus as i32,
                ..Default::default()
            });
        }

        if let Some(mem) = config.memory {
            ch_vm_config.memory = Some(models::MemoryConfig {
                size: mem.size_mib as i64 * 1024 * 1024,
                shared: Some(true),
                hugepages: Some(mem.hugepages),
                ..Default::default()
            });
        }

        let mut ch_net_configs: Vec<models::NetConfig> = Vec::new();
        let mut ch_device_configs: Vec<models::DeviceConfig> = Vec::new();

        for nc in config.net {
            match convert_net_config_to_ch(&nc)? {
                ChNetworkDevice::Net(net_config) => {
                    ch_net_configs.push(*net_config);
                }
                ChNetworkDevice::Device(device_config) => {
                    ch_device_configs.push(device_config);
                }
            }
        }

        if !ch_net_configs.is_empty() {
            ch_vm_config.net = Some(ch_net_configs);
        }

        if !ch_device_configs.is_empty() {
            ch_vm_config.devices = Some(ch_device_configs);
        }

        if let Some(ignition_data) = config.ignition {
            if !ignition_data.is_empty() {
                ch_vm_config.platform = Some(models::PlatformConfig {
                    num_pci_segments: Some(1),
                    oem_strings: Some(vec![ignition_data]),
                    ..Default::default()
                });
            }
        }

        client
            .create_vm(ch_vm_config)
            .await
            .map_err(|e| VmmError::ApiOperationFailed(format!("vm.create API call failed: {e}")))?;

        info!("CloudHypervisorAdapter ({vm_id}): vm.create API call successful.");

        Ok::<(), VmmError>(())
    }

    async fn cleanup_socket_file(&self, vm_id: &str, socket_path: &Path, socket_type: &str) {
        if let Err(e) = tokio::fs::remove_file(socket_path).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                warn!(
                    "CloudHypervisorAdapter ({vm_id}): Failed to remove {socket_type} socket {path}: {e}",
                    path = socket_path.display()
                );
            }
        } else {
            info!(
                "CloudHypervisorAdapter ({vm_id}): Successfully removed {socket_type} socket {path}",
                path = socket_path.display()
            );
        }
    }
}

#[tonic::async_trait]
impl Hypervisor for CloudHypervisorAdapter {
    async fn create_vm(
        &self,
        vm_id: &str,
        req: CreateVmRequest,
        image_uuid: String,
    ) -> Result<Option<i64>, VmmError> {
        info!("CloudHypervisorAdapter: Creating VM with provided ID: {vm_id}");

        let config = req
            .config
            .ok_or_else(|| VmmError::InvalidConfig("VmConfig is required".to_string()))?;

        let api_socket_path = PathBuf::from(VM_API_SOCKET_DIR).join(vm_id);

        info!("CloudHypervisorAdapter ({vm_id}): Spawning cloud-hypervisor process...");
        let mut child = unsafe {
            TokioCommand::new(&self.ch_binary_path)
                .arg("--api-socket")
                .arg(&api_socket_path)
                .pre_exec(|| {
                    unistd::setsid()?;
                    Ok(())
                })
                .spawn()
        }
        .map_err(|e| VmmError::ProcessSpawnFailed(e.to_string()))?;
        let pid = child.id().map(|id| id as i64);

        let vm_creation = self.perform_vm_creation(vm_id, config, image_uuid, &api_socket_path);

        tokio::select! {
            biased;
            exit_status_res = child.wait() => {
                let status = exit_status_res.map_err(|e| VmmError::ProcessSpawnFailed(format!("Failed to wait for child process: {e}")))?;
                Err(VmmError::ProcessSpawnFailed(format!("Process exited prematurely with status: {status}")))
            }
            creation_result = vm_creation => {
                match creation_result {
                    Ok(_) => Ok(pid),
                    Err(e) => {
                        if let Err(kill_err) = child.kill().await {
                             warn!("CloudHypervisorAdapter ({vm_id}): Failed to kill child process after creation failure: {kill_err}");
                        }
                        let _ = child.wait().await;
                        Err(e)
                    }
                }
            }
        }
    }

    async fn start_vm(&self, req: StartVmRequest) -> Result<StartVmResponse, VmmError> {
        let api_client = self.get_ch_api_client(&req.vm_id)?;
        api_client
            .boot_vm()
            .await
            .map_err(|e| VmmError::ApiOperationFailed(e.to_string()))?;

        Ok(StartVmResponse {})
    }

    async fn healthcheck_vm(
        &self,
        vm_id: String,
        broadcast_tx: mpsc::Sender<VmEventWrapper>,
        mut cancel_bus: broadcast::Receiver<Uuid>,
    ) {
        info!("CloudHypervisorAdapter ({vm_id}): Starting healthcheck monitoring.");
        let mut interval = time::interval(Duration::from_secs(10));
        let vm_id_uuid = match Uuid::parse_str(&vm_id) {
            Ok(id) => id,
            Err(e) => {
                error!("CloudHypervisorAdapter ({vm_id}): Invalid UUID format, cannot start healthcheck: {e}");
                return;
            }
        };

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    log::debug!("CloudHypervisorAdapter ({vm_id}): Performing healthcheck ping.");
                    let req = PingVmRequest {
                        vm_id: vm_id.clone(),
                    };

                    if let Err(e) = self.ping_vm(req).await {
                        warn!("CloudHypervisorAdapter ({vm_id}): Healthcheck failed: {e}. VM is considered unhealthy.");
                        super::broadcast_state_change_event(
                            &broadcast_tx,
                            &vm_id,
                            "vm-health-monitor",
                            feos_proto::vm_service::VmStateChangedEvent {
                                new_state: VmState::Crashed as i32,
                                reason: format!("Healthcheck failed: {e}"),
                            },
                            None,
                        )
                        .await;
                        break;
                    } else {
                        log::debug!("CloudHypervisorAdapter ({vm_id}): Healthcheck ping successful.");
                    }
                }
                Ok(cancelled_vm_id) = cancel_bus.recv() => {
                    if cancelled_vm_id == vm_id_uuid {
                        info!("CloudHypervisorAdapter ({vm_id}): Received cancellation signal. Stopping healthcheck.");
                        break;
                    }
                }
                else => {
                    info!("CloudHypervisorAdapter ({vm_id}): Healthcheck cancellation channel closed. Stopping healthcheck.");
                    break;
                }
            }
        }
        info!("CloudHypervisorAdapter ({vm_id}): Stopping healthcheck monitoring.");
    }

    async fn get_vm(&self, req: GetVmRequest) -> Result<VmInfo, VmmError> {
        let api_client = self.get_ch_api_client(&req.vm_id)?;
        let ch_info = api_client
            .vm_info_get()
            .await
            .map_err(|e| VmmError::ApiOperationFailed(e.to_string()))?;

        let state = match ch_info.state {
            ChVmState::Created => VmState::Created,
            ChVmState::Running => VmState::Running,
            ChVmState::Paused => VmState::Paused,
            ChVmState::Shutdown => VmState::Stopped,
        };

        Ok(VmInfo {
            vm_id: req.vm_id,
            state: state as i32,
            config: None,
        })
    }

    async fn delete_vm(
        &self,
        req: DeleteVmRequest,
        process_id: Option<i64>,
    ) -> Result<DeleteVmResponse, VmmError> {
        if let Ok(api_client) = self.get_ch_api_client(&req.vm_id) {
            if let Err(e) = api_client.delete_vm().await {
                warn!(
                    "CloudHypervisorAdapter ({vm_id}): API call to delete VM failed: {e}. This might happen if the process is already gone. Continuing cleanup.",
                    vm_id = req.vm_id
                );
            } else {
                info!(
                    "CloudHypervisorAdapter ({vm_id}): Successfully deleted hypervisor process via API.",
                    vm_id = req.vm_id
                );
            }
        }

        if let Some(pid_val) = process_id {
            info!(
                "CloudHypervisorAdapter ({vm_id}): Attempting to kill process with PID: {pid_val}",
                vm_id = req.vm_id
            );
            let pid = Pid::from_raw(pid_val as i32);
            match kill(pid, Signal::SIGKILL) {
                Ok(_) => info!(
                    "CloudHypervisorAdapter ({vm_id}): Successfully sent SIGKILL to process {pid_val}.",
                    vm_id = req.vm_id
                ),
                Err(nix::Error::ESRCH) => info!(
                    "CloudHypervisorAdapter ({vm_id}): Process {pid_val} already exited.",
                    vm_id = req.vm_id
                ),
                Err(e) => warn!(
                    "CloudHypervisorAdapter ({vm_id}): Failed to kill process {pid_val}: {e}. It might already be gone.",
                    vm_id = req.vm_id
                ),
            }
        }

        let api_socket_path = PathBuf::from(VM_API_SOCKET_DIR).join(&req.vm_id);
        self.cleanup_socket_file(&req.vm_id, &api_socket_path, "API")
            .await;

        let console_socket_path =
            PathBuf::from(VM_CONSOLE_DIR).join(format!("{}.console", req.vm_id));
        self.cleanup_socket_file(&req.vm_id, &console_socket_path, "console")
            .await;

        Ok(DeleteVmResponse {})
    }

    async fn get_console_socket_path(&self, vm_id: &str) -> Result<PathBuf, VmmError> {
        let socket_path = PathBuf::from(VM_CONSOLE_DIR).join(format!("{vm_id}.console"));
        if tokio::fs::try_exists(&socket_path)
            .await
            .map_err(|e| VmmError::Internal(e.to_string()))?
        {
            Ok(socket_path)
        } else {
            Err(VmmError::VmNotFound(vm_id.to_string()))
        }
    }

    async fn ping_vm(&self, req: PingVmRequest) -> Result<PingVmResponse, VmmError> {
        let api_client = self.get_ch_api_client(&req.vm_id)?;
        let ch_ping: ChPingResponse = api_client
            .vmm_ping_get()
            .await
            .map_err(|e| VmmError::ApiOperationFailed(e.to_string()))?;

        Ok(PingVmResponse {
            build_version: ch_ping.build_version.unwrap_or_default(),
            version: ch_ping.version,
            pid: ch_ping.pid.unwrap_or(0),
            features: ch_ping.features.unwrap_or_default(),
        })
    }

    async fn shutdown_vm(&self, req: ShutdownVmRequest) -> Result<ShutdownVmResponse, VmmError> {
        let api_client = self.get_ch_api_client(&req.vm_id)?;
        api_client
            .shutdown_vm()
            .await
            .map_err(|e| VmmError::ApiOperationFailed(e.to_string()))?;
        Ok(ShutdownVmResponse {})
    }

    async fn pause_vm(&self, req: PauseVmRequest) -> Result<PauseVmResponse, VmmError> {
        let api_client = self.get_ch_api_client(&req.vm_id)?;
        api_client
            .pause_vm()
            .await
            .map_err(|e| VmmError::ApiOperationFailed(e.to_string()))?;
        Ok(PauseVmResponse {})
    }

    async fn resume_vm(&self, req: ResumeVmRequest) -> Result<ResumeVmResponse, VmmError> {
        let api_client = self.get_ch_api_client(&req.vm_id)?;
        api_client
            .resume_vm()
            .await
            .map_err(|e| VmmError::ApiOperationFailed(e.to_string()))?;
        Ok(ResumeVmResponse {})
    }

    async fn attach_disk(&self, _req: AttachDiskRequest) -> Result<AttachDiskResponse, VmmError> {
        Err(VmmError::Internal(
            "AttachDisk not implemented for CloudHypervisorAdapter".to_string(),
        ))
    }

    async fn detach_disk(&self, _req: DetachDiskRequest) -> Result<DetachDiskResponse, VmmError> {
        Err(VmmError::Internal(
            "DetachDisk not implemented for CloudHypervisorAdapter".to_string(),
        ))
    }

    async fn attach_nic(&self, req: AttachNicRequest) -> Result<AttachNicResponse, VmmError> {
        let api_client = self.get_ch_api_client(&req.vm_id)?;
        let nic = req
            .nic
            .ok_or_else(|| VmmError::InvalidConfig("NetConfig is required".to_string()))?;

        let ch_device = convert_net_config_to_ch(&nic)?;
        let device_id = ch_device.id();

        match ch_device {
            ChNetworkDevice::Net(ch_net_config) => {
                api_client
                    .vm_add_net_put(*ch_net_config)
                    .await
                    .map_err(|e| VmmError::ApiOperationFailed(format!("vm.add-net failed: {e}")))?;
            }
            ChNetworkDevice::Device(ch_device_config) => {
                api_client
                    .vm_add_device_put(ch_device_config)
                    .await
                    .map_err(|e| {
                        VmmError::ApiOperationFailed(format!("vm.add-device failed: {e}"))
                    })?;
            }
        }

        Ok(AttachNicResponse {
            device_id: device_id.unwrap_or_default(),
        })
    }

    async fn detach_nic(&self, req: DetachNicRequest) -> Result<DetachNicResponse, VmmError> {
        let api_client = self.get_ch_api_client(&req.vm_id)?;
        let device_to_remove = models::VmRemoveDevice {
            id: Some(req.device_id),
        };
        api_client
            .vm_remove_device_put(device_to_remove)
            .await
            .map_err(|e| VmmError::ApiOperationFailed(format!("vm.remove-device failed: {e}")))?;
        Ok(DetachNicResponse {})
    }
}
