use super::{broadcast_state_change_event, Hypervisor, VmmError};
use crate::{VmEventWrapper, IMAGE_DIR, VM_API_SOCKET_DIR, VM_CONSOLE_DIR};
use cloud_hypervisor_client::{
    apis::{configuration::Configuration, DefaultApi, DefaultApiClient},
    models::{
        self, console_config::Mode as ConsoleMode, vm_info::State as ChVmState,
        VmmPingResponse as ChPingResponse,
    },
};
use feos_proto::vm_service::{
    AttachDiskRequest, AttachDiskResponse, CreateVmRequest, DeleteVmRequest, DeleteVmResponse,
    GetVmRequest, PauseVmRequest, PauseVmResponse, PingVmRequest, PingVmResponse,
    RemoveDiskRequest, RemoveDiskResponse, ResumeVmRequest, ResumeVmResponse, ShutdownVmRequest,
    ShutdownVmResponse, StartVmRequest, StartVmResponse, StreamVmEventsRequest, VmEvent, VmInfo,
    VmState, VmStateChangedEvent,
};
use hyper_util::client::legacy::Client;
use hyperlocal::{UnixClientExt, UnixConnector, Uri as HyperlocalUri};
use log::{error, info, warn};
use nix::sys::signal::{kill, Signal};
use nix::unistd::{self, Pid};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::process::Command as TokioCommand;
use tokio::sync::{broadcast, mpsc};
use tokio::time::{timeout, Duration};

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

    async fn cleanup_socket_file(&self, vm_id: &str, socket_path: &Path, socket_type: &str) {
        if let Err(e) = tokio::fs::remove_file(socket_path).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                warn!(
                    "CH_ADAPTER ({}): Failed to remove {} socket {}: {}",
                    vm_id,
                    socket_type,
                    socket_path.display(),
                    e
                );
            }
        } else {
            info!(
                "CH_ADAPTER ({}): Successfully removed {} socket {}",
                vm_id,
                socket_type,
                socket_path.display()
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
        broadcast_tx: broadcast::Sender<VmEventWrapper>,
    ) -> Result<(), VmmError> {
        info!("CH_ADAPTER: Creating VM with provided ID: {vm_id}");

        broadcast_state_change_event(
            &broadcast_tx,
            vm_id,
            "vm-service",
            VmStateChangedEvent {
                new_state: VmState::Creating as i32,
                reason: "VM creation process started".to_string(),
            },
            None,
        )
        .await;

        let config = req
            .config
            .ok_or_else(|| VmmError::InvalidConfig("VmConfig is required".to_string()))?;

        let api_socket_path = PathBuf::from(VM_API_SOCKET_DIR).join(vm_id);

        info!(
            "CH_ADAPTER ({}): Spawning cloud-hypervisor process...",
            vm_id
        );
        let mut child = unsafe {
            TokioCommand::new(&self.ch_binary_path)
                .arg("--api-socket")
                .arg(&api_socket_path)
                .pre_exec(|| unistd::setsid().map(|_pid| ()).map_err(io::Error::other))
                .spawn()
        }
        .map_err(|e| VmmError::ProcessSpawnFailed(e.to_string()))?;
        let pid = child.id().map(|id| id as i64);

        let vm_id_clone = vm_id.to_string();
        let broadcast_tx_clone = broadcast_tx.clone();
        tokio::spawn(async move {
            match child.wait().await {
                Ok(status) => {
                    warn!(
                        "CH_ADAPTER ({}): Process exited with status: {}",
                        &vm_id_clone, status
                    );
                    broadcast_state_change_event(
                        &broadcast_tx_clone,
                        &vm_id_clone,
                        "vm-process",
                        VmStateChangedEvent {
                            new_state: VmState::Crashed as i32,
                            reason: format!(
                                "Process exited with code {}",
                                status.code().unwrap_or(-1)
                            ),
                        },
                        None,
                    )
                    .await;
                }
                Err(e) => {
                    error!(
                        "CH_ADAPTER ({}): Failed to wait for child process: {}",
                        &vm_id_clone, e
                    );
                }
            }
        });

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
        info!("CH_ADAPTER ({}): API socket is available.", vm_id);

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
                ..Default::default()
            });
        }

        if let Some(ignition_data) = config.ignition {
            if !ignition_data.is_empty() {
                ch_vm_config.platform = Some(models::PlatformConfig {
                    oem_strings: Some(vec![ignition_data]),
                    ..Default::default()
                });
            }
        }

        client
            .create_vm(ch_vm_config)
            .await
            .map_err(|e| VmmError::ApiOperationFailed(format!("vm.create API call failed: {e}")))?;

        info!("CH_ADAPTER ({}): vm.create API call successful.", vm_id);

        broadcast_state_change_event(
            &broadcast_tx,
            vm_id,
            "vm-service",
            VmStateChangedEvent {
                new_state: VmState::Created as i32,
                reason: "Hypervisor process started and VM configured".to_string(),
            },
            pid,
        )
        .await;

        Ok(())
    }

    async fn start_vm(
        &self,
        req: StartVmRequest,
        broadcast_tx: broadcast::Sender<VmEventWrapper>,
    ) -> Result<StartVmResponse, VmmError> {
        let api_client = self.get_ch_api_client(&req.vm_id)?;
        api_client
            .boot_vm()
            .await
            .map_err(|e| VmmError::ApiOperationFailed(e.to_string()))?;

        broadcast_state_change_event(
            &broadcast_tx,
            &req.vm_id,
            "vm-service",
            VmStateChangedEvent {
                new_state: VmState::Running as i32,
                reason: "Start command successful".to_string(),
            },
            None,
        )
        .await;

        Ok(StartVmResponse {})
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
        _broadcast_tx: broadcast::Sender<VmEventWrapper>,
    ) -> Result<DeleteVmResponse, VmmError> {
        if let Ok(api_client) = self.get_ch_api_client(&req.vm_id) {
            if let Err(e) = api_client.delete_vm().await {
                warn!(
                    "CH_ADAPTER ({}): API call to delete VM failed: {}. This might happen if the process is already gone. Continuing cleanup.",
                    req.vm_id, e
                );
            } else {
                info!(
                    "CH_ADAPTER ({}): Successfully deleted hypervisor process via API.",
                    req.vm_id
                );
            }
        }

        if let Some(pid_val) = process_id {
            info!(
                "CH_ADAPTER ({}): Attempting to kill process with PID: {}",
                req.vm_id, pid_val
            );
            let pid = Pid::from_raw(pid_val as i32);
            match kill(pid, Signal::SIGKILL) {
                Ok(_) => info!(
                    "CH_ADAPTER ({}): Successfully sent SIGKILL to process {}.",
                    req.vm_id, pid_val
                ),
                Err(nix::Error::ESRCH) => info!(
                    "CH_ADAPTER ({}): Process {} already exited.",
                    req.vm_id, pid_val
                ),
                Err(e) => warn!(
                    "CH_ADAPTER ({}): Failed to kill process {}: {}. It might already be gone.",
                    req.vm_id, pid_val, e
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

    async fn stream_vm_events(
        &self,
        req: StreamVmEventsRequest,
        broadcast_tx: broadcast::Sender<VmEventWrapper>,
    ) -> Result<mpsc::Receiver<Result<VmEvent, VmmError>>, VmmError> {
        let (tx, rx) = mpsc::channel(32);
        let mut broadcast_rx = broadcast_tx.subscribe();
        let vm_id_to_watch = req.vm_id;

        tokio::spawn(async move {
            let watcher_desc = vm_id_to_watch
                .clone()
                .unwrap_or_else(|| "all VMs".to_string());
            loop {
                match broadcast_rx.recv().await {
                    Ok(VmEventWrapper { event, .. }) => {
                        if vm_id_to_watch.as_ref().is_none_or(|id| event.vm_id == *id)
                            && tx.send(Ok(event)).await.is_err()
                        {
                            info!("gRPC event stream for '{watcher_desc}' disconnected.");
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("Event stream for '{watcher_desc}' lagged by {n} messages.");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!(
                            "Broadcast channel closed. Shutting down stream for '{watcher_desc}'."
                        );
                        break;
                    }
                }
            }
        });

        Ok(rx)
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

    async fn shutdown_vm(
        &self,
        req: ShutdownVmRequest,
        broadcast_tx: broadcast::Sender<VmEventWrapper>,
    ) -> Result<ShutdownVmResponse, VmmError> {
        let api_client = self.get_ch_api_client(&req.vm_id)?;
        api_client
            .shutdown_vm()
            .await
            .map_err(|e| VmmError::ApiOperationFailed(e.to_string()))?;
        broadcast_state_change_event(
            &broadcast_tx,
            &req.vm_id,
            "vm-service",
            VmStateChangedEvent {
                new_state: VmState::Stopped as i32,
                reason: "Shutdown command successful".to_string(),
            },
            None,
        )
        .await;
        Ok(ShutdownVmResponse {})
    }

    async fn pause_vm(
        &self,
        req: PauseVmRequest,
        broadcast_tx: broadcast::Sender<VmEventWrapper>,
    ) -> Result<PauseVmResponse, VmmError> {
        let api_client = self.get_ch_api_client(&req.vm_id)?;
        api_client
            .pause_vm()
            .await
            .map_err(|e| VmmError::ApiOperationFailed(e.to_string()))?;
        broadcast_state_change_event(
            &broadcast_tx,
            &req.vm_id,
            "vm-service",
            VmStateChangedEvent {
                new_state: VmState::Paused as i32,
                reason: "Pause command successful".to_string(),
            },
            None,
        )
        .await;
        Ok(PauseVmResponse {})
    }

    async fn resume_vm(
        &self,
        req: ResumeVmRequest,
        broadcast_tx: broadcast::Sender<VmEventWrapper>,
    ) -> Result<ResumeVmResponse, VmmError> {
        let api_client = self.get_ch_api_client(&req.vm_id)?;
        api_client
            .resume_vm()
            .await
            .map_err(|e| VmmError::ApiOperationFailed(e.to_string()))?;
        broadcast_state_change_event(
            &broadcast_tx,
            &req.vm_id,
            "vm-service",
            VmStateChangedEvent {
                new_state: VmState::Running as i32,
                reason: "Resume command successful".to_string(),
            },
            None,
        )
        .await;
        Ok(ResumeVmResponse {})
    }

    async fn attach_disk(&self, _req: AttachDiskRequest) -> Result<AttachDiskResponse, VmmError> {
        Err(VmmError::Internal(
            "AttachDisk not implemented for CloudHypervisorAdapter".to_string(),
        ))
    }

    async fn remove_disk(&self, _req: RemoveDiskRequest) -> Result<RemoveDiskResponse, VmmError> {
        Err(VmmError::Internal(
            "RemoveDisk not implemented for CloudHypervisorAdapter".to_string(),
        ))
    }
}
