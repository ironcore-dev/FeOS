// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use feos_proto::task_service::{
    task_service_client::TaskServiceClient, CreateRequest, DeleteRequest, KillRequest, StartRequest,
};
use hyper_util::rt::TokioIo;
use log::info;
use serde::{Deserialize, Serialize};
use std::path::Path;
use task_service::TASK_SERVICE_SOCKET;
use tokio::fs;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("I/O error")]
    Io(#[from] std::io::Error),
    #[error("Task service communication failed: {0}")]
    TaskService(#[from] tonic::Status),
}

#[derive(Serialize, Deserialize, Debug)]
struct OciImageSpec {
    config: OciImageConfig,
}

#[derive(Serialize, Deserialize, Debug)]
struct OciImageConfig {
    #[serde(rename = "Entrypoint")]
    entrypoint: Option<Vec<String>>,
    #[serde(rename = "Cmd")]
    cmd: Option<Vec<String>>,
    #[serde(rename = "Env")]
    env: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct OciRuntimeSpec {
    oci_version: String,
    process: OciProcess,
    root: OciRoot,
    mounts: Vec<OciMount>,
    linux: OciLinux,
}

#[derive(Serialize, Deserialize, Debug)]
struct OciProcess {
    terminal: bool,
    user: OciUser,
    args: Vec<String>,
    env: Vec<String>,
    cwd: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct OciUser {
    uid: u32,
    gid: u32,
}

#[derive(Serialize, Deserialize, Debug)]
struct OciRoot {
    path: String,
    readonly: bool,
}

#[derive(Serialize, Deserialize, Debug)]
struct OciMount {
    destination: String,
    #[serde(rename = "type")]
    typ: String,
    source: String,
    options: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct OciLinux {
    namespaces: Vec<OciLinuxNamespace>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct OciLinuxNamespace {
    #[serde(rename = "type")]
    typ: String,
}

pub struct ContainerAdapter;

impl Default for ContainerAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ContainerAdapter {
    pub fn new() -> Self {
        Self
    }

    async fn get_task_service_client() -> Result<TaskServiceClient<Channel>, AdapterError> {
        let socket_path = Path::new(TASK_SERVICE_SOCKET).to_path_buf();
        Endpoint::try_from("http://[::1]:50051")
            .unwrap()
            .connect_with_connector(service_fn(move |_: Uri| {
                let socket_path = socket_path.clone();
                async move {
                    tokio::net::UnixStream::connect(socket_path)
                        .await
                        .map(TokioIo::new)
                }
            }))
            .await
            .map(TaskServiceClient::new)
            .map_err(|e| AdapterError::TaskService(tonic::Status::unavailable(e.to_string())))
    }

    async fn generate_runtime_spec(bundle_path: &Path) -> Result<(), AdapterError> {
        let image_config_path = bundle_path.join("config.json");
        let image_spec_json = fs::read_to_string(&image_config_path).await?;
        let image_spec: OciImageSpec = serde_json::from_str(&image_spec_json)
            .map_err(|e| AdapterError::Internal(e.to_string()))?;

        let mut args = Vec::new();
        if let Some(entrypoint) = image_spec.config.entrypoint {
            args.extend(entrypoint);
        }
        if let Some(cmd) = image_spec.config.cmd {
            args.extend(cmd);
        }

        let runtime_spec = OciRuntimeSpec {
            oci_version: "1.0.2".to_string(),
            process: OciProcess {
                terminal: false,
                user: OciUser { uid: 0, gid: 0 },
                args,
                env: image_spec.config.env.unwrap_or_default(),
                cwd: "/".to_string(),
            },
            root: OciRoot {
                path: "rootfs".to_string(),
                readonly: false,
            },
            mounts: vec![
                OciMount {
                    destination: "/proc".to_string(),
                    typ: "proc".to_string(),
                    source: "proc".to_string(),
                    options: vec![],
                },
                OciMount {
                    destination: "/dev".to_string(),
                    typ: "tmpfs".to_string(),
                    source: "tmpfs".to_string(),
                    options: vec![
                        "nosuid".to_string(),
                        "strictatime".to_string(),
                        "mode=755".to_string(),
                        "size=65536k".to_string(),
                    ],
                },
                OciMount {
                    destination: "/dev/pts".to_string(),
                    typ: "devpts".to_string(),
                    source: "devpts".to_string(),
                    options: vec![
                        "nosuid".to_string(),
                        "noexec".to_string(),
                        "newinstance".to_string(),
                        "ptmxmode=0666".to_string(),
                        "mode=0620".to_string(),
                    ],
                },
                OciMount {
                    destination: "/sys".to_string(),
                    typ: "sysfs".to_string(),
                    source: "sysfs".to_string(),
                    options: vec![
                        "nosuid".to_string(),
                        "noexec".to_string(),
                        "nodev".to_string(),
                        "ro".to_string(),
                    ],
                },
            ],
            linux: OciLinux {
                namespaces: vec![
                    OciLinuxNamespace {
                        typ: "pid".to_string(),
                    },
                    OciLinuxNamespace {
                        typ: "ipc".to_string(),
                    },
                    OciLinuxNamespace {
                        typ: "uts".to_string(),
                    },
                    OciLinuxNamespace {
                        typ: "mount".to_string(),
                    },
                    // OciLinuxNamespace {
                    //     typ: "network".to_string(),
                    // },
                ],
            },
        };

        let runtime_spec_json = serde_json::to_string(&runtime_spec)
            .map_err(|e| AdapterError::Internal(e.to_string()))?;
        fs::write(&image_config_path, runtime_spec_json).await?;
        info!("Generated and overwrote runtime config.json in bundle");

        Ok(())
    }

    pub async fn create_container(
        &self,
        container_id: &str,
        bundle_path: &Path,
    ) -> Result<i64, AdapterError> {
        info!("Adapter: Rewriting OCI spec for container {container_id}");
        Self::generate_runtime_spec(bundle_path).await?;

        info!("Adapter: Connecting to TaskService for container {container_id}");
        let mut task_client = Self::get_task_service_client().await?;

        let bundle_path_str = bundle_path
            .to_str()
            .ok_or_else(|| AdapterError::Internal("Bundle path is not valid UTF-8".to_string()))?
            .to_string();

        info!("Adapter: Calling Create RPC on TaskService for container {container_id}");
        let request = CreateRequest {
            container_id: container_id.to_string(),
            bundle_path: bundle_path_str,
            stdin_path: "".to_string(),
            stdout_path: "".to_string(),
            stderr_path: "".to_string(),
        };

        let response = task_client.create(request).await?;

        let pid = response.into_inner().pid;
        info!("Adapter: TaskService created container {container_id} with PID {pid}");

        Ok(pid)
    }

    pub async fn start_container(&self, container_id: &str) -> Result<(), AdapterError> {
        let mut task_client = Self::get_task_service_client().await?;
        let request = StartRequest {
            container_id: container_id.to_string(),
        };
        task_client.start(request).await?;
        Ok(())
    }

    pub async fn stop_container(
        &self,
        container_id: &str,
        signal: u32,
    ) -> Result<(), AdapterError> {
        let mut task_client = Self::get_task_service_client().await?;
        let request = KillRequest {
            container_id: container_id.to_string(),
            signal,
        };
        task_client.kill(request).await?;
        Ok(())
    }

    pub async fn delete_container(&self, container_id: &str) -> Result<(), AdapterError> {
        let mut task_client = Self::get_task_service_client().await?;
        let request = DeleteRequest {
            container_id: container_id.to_string(),
        };
        task_client.delete(request).await?;
        Ok(())
    }
}
