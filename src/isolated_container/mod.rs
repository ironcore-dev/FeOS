use crate::container::container_service::container_service_client::ContainerServiceClient;
use crate::container::container_service::{CreateContainerRequest, RunContainerRequest};
use crate::daemon::FeOSAPI;
use crate::feos_grpc::Empty;
use crate::ringbuffer::RingBuffer;
use crate::vm::{Error, NetworkMode};
use crate::{container, feos_grpc, network, vm};
use flate2::read::GzDecoder;
use futures::channel;
use hyper_util::rt::TokioIo;
use isolated_container_service::isolated_container_service_server::IsolatedContainerService;
use libcontainer::container::builder::ContainerBuilder;
use libcontainer::container::Container;
use libcontainer::oci_spec::runtime::{LinuxNamespace, Mount, Spec};
use libcontainer::signal::Signal;
use libcontainer::syscall::syscall::SyscallType;
use libcontainer::workload::default::DefaultExecutor;
use log::{debug, error, info};
use rtnetlink::new_connection;
use serde_json::to_writer_pretty;
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Arc;
use std::thread::sleep;
use std::{collections::HashMap, num::TryFromIntError, sync::Mutex};
use std::{fmt::Debug, io, path::PathBuf, time};
use tar::Archive;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::spawn;
use tokio::sync::mpsc;
use tonic::transport::{Channel, Endpoint, Uri};
use tonic::{Request, Response, Status};
use tower::service_fn;
use uuid::Uuid;

pub mod isolated_container_service {
    tonic::include_proto!("isolated_container");
}

#[derive(Debug, Default)]
pub struct IsolatedContainerAPI {
    vmm: Arc<vm::Manager>,
    network: Arc<network::Manager>,
    vm_to_container: Mutex<HashMap<Uuid, Uuid>>,
}

impl IsolatedContainerAPI {
    pub fn new(vmm: Arc<vm::Manager>, network: Arc<network::Manager>) -> Self {
        IsolatedContainerAPI {
            vmm,
            network,
            vm_to_container: Mutex::new(HashMap::new()),
        }
    }
}

fn handle_error(e: vm::Error) -> tonic::Status {
    match e {
        vm::Error::AlreadyExists => Status::new(tonic::Code::AlreadyExists, "vm already exists"),
        vm::Error::NotFound => Status::new(tonic::Code::NotFound, "vm not found"),
        vm::Error::SocketFailure(e) => {
            info!("socket error: {:?}", e);
            Status::new(tonic::Code::Internal, "failed to ")
        }
        vm::Error::InvalidInput(e) => {
            info!("invalid input error: {:?}", e);
            Status::new(tonic::Code::Internal, "invalid input")
        }
        vm::Error::CHCommandFailure(e) => {
            info!("failed to connect to cloud hypervisor: {:?}", e);
            Status::new(
                tonic::Code::Internal,
                "failed to connect to cloud hypervisor",
            )
        }
        vm::Error::CHApiFailure(e) => {
            info!("failed to connect to cloud hypervisor api: {:?}", e);
            Status::new(
                tonic::Code::Internal,
                "failed to connect to cloud hypervisor api",
            )
        }
        vm::Error::Failed => Status::new(tonic::Code::AlreadyExists, "vm already exists"),
    }
}

async fn retry_get_channel(path: String) -> Result<Channel, Error> {
    async fn get_channel(path: String) -> Result<Channel, Error> {
        let channel = Endpoint::try_from("http://[::]:50051")
            .map_err(|e| Error::Failed)?
            .connect_with_connector(service_fn(move |_: Uri| {
                let path = path.clone();
                async move {
                    let mut stream = UnixStream::connect(&path).await.map_err(|e| {
                        io::Error::new(
                            io::ErrorKind::Other,
                            format!("UnixStream connect error: {}", e),
                        )
                    })?;
                    let connect_cmd = format!("CONNECT {}\n", 1337);
                    stream
                        .write_all(connect_cmd.as_bytes())
                        .await
                        .map_err(|e| {
                            io::Error::new(io::ErrorKind::Other, format!("Write error: {}", e))
                        })?;

                    let mut buffer = [0u8; 128];
                    let n = stream.read(&mut buffer).await.map_err(|e| {
                        io::Error::new(io::ErrorKind::Other, format!("Read error: {}", e))
                    })?;
                    let response = String::from_utf8_lossy(&buffer[..n]);
                    // Parse the response
                    if !response.starts_with("OK") {
                        return Err(io::Error::new(
                            io::ErrorKind::Other,
                            format!("Failed to connect to vsock: {}", response.trim()),
                        ));
                    }
                    info!("Connected to vsock: {}", response.trim());
                    // Connect to an Uds socket
                    Ok::<_, io::Error>(TokioIo::new(stream))
                }
            }))
            .await
            .map_err(|e| Error::Failed)?;
        Ok(channel)
    }

    const RETRIES: u8 = 20;
    const DELAY: tokio::time::Duration = time::Duration::from_millis(2000);
    for attempt in 0..RETRIES {
        match get_channel(path.clone()).await {
            Ok(channel) => return Ok(channel),
            Err(e) => {
                info!("Attempt {} failed: {:?}", attempt + 1, e);
                if attempt < RETRIES - 1 {
                    info!("Retrying in {:?}", DELAY);
                    tokio::time::sleep(DELAY).await;
                }
            }
        }
    }
    Err(Error::Failed) // Or another appropriate error
}

#[tonic::async_trait]
impl IsolatedContainerService for IsolatedContainerAPI {
    async fn create_container(
        &self,
        request: Request<isolated_container_service::CreateContainerRequest>,
    ) -> Result<Response<isolated_container_service::CreateContainerResponse>, Status> {
        info!("Got create_container request");

        let id = Uuid::new_v4();

        self.vmm.init_vmm(id, true).map_err(handle_error)?;
        self.vmm
            .create_vm(
                id,
                2,
                4294967296,
                vm::BootMode::KernelBoot(vm::KernelBootMode {
                    // kernel:   PathBuf::from("/usr/share/feos/vmlinuz"),
                    kernel: PathBuf::from("/home/lukasfrank/dev/FeOS/target/kernel/vmlinuz"),
                    initramfs: PathBuf::from("/home/lukasfrank/dev/FeOS/target/initramfs.zst"),
                    // initramfs: PathBuf::from("/usr/share/feos/initramfs"),
                    // TODO
                    cmdline: "console=tty0 console=ttyS0,115200 intel_iommu=on iommu=pt"
                        .to_string(),
                }),
                None,
            )
            .map_err(handle_error)?;

        self.vmm.boot_vm(id).map_err(handle_error)?;

        self.vmm
            .add_net_device(
                id,
                NetworkMode::TAPDeviceName(network::Manager::device_name(&id)),
            )
            .expect("failed to add tap device");

        self.network
            .start_dhcp(id)
            .await
            .expect("failed to start network");

        let path = format!("vsock{}.sock", network::Manager::device_name(&id));
        let channel = retry_get_channel(path).await.expect("abc");

        let mut client = ContainerServiceClient::new(channel);
        let request = tonic::Request::new(CreateContainerRequest {
            image: request.get_ref().image.to_string(),
            command: request.get_ref().command.clone(),
        });
        let response = client.create_container(request).await?;
        println!("{}", response.get_ref().uuid);

        let container_id = Uuid::parse_str(&response.get_ref().uuid)
            .map_err(|_| Status::invalid_argument("failed to parse uuid"))?;

        let mut vm_to_container = self.vm_to_container.lock().unwrap();
        vm_to_container.insert(id, container_id);

        Ok(Response::new(
            isolated_container_service::CreateContainerResponse {
                uuid: id.to_string(),
            },
        ))
    }

    async fn run_container(
        &self,
        request: Request<isolated_container_service::RunContainerRequest>,
    ) -> Result<Response<isolated_container_service::RunContainerResponse>, Status> {
        info!("Got run_container request");

        let vm_id: String = request.get_ref().uuid.clone();
        let vm_id = Uuid::parse_str(&vm_id)
            .map_err(|_| Status::invalid_argument("failed to parse uuid"))?;

        let container_id = {
            let vm_to_container = self
                .vm_to_container
                .lock()
                .map_err(|_| Status::internal("Failed to lock mutex"))?;
            *vm_to_container
                .get(&vm_id)
                .ok_or_else(|| Status::not_found(format!("VM with ID '{}' not found", vm_id)))?
        };

        let path = format!("vsock{}.sock", network::Manager::device_name(&vm_id));
        let channel = retry_get_channel(path).await.expect("abc");

        let mut client = ContainerServiceClient::new(channel);
        let request = tonic::Request::new(RunContainerRequest {
            uuid: container_id.to_string(),
        });
        let response = client.run_container(request).await?;

        Ok(Response::new(
            isolated_container_service::RunContainerResponse {},
        ))
    }

    async fn stop_container(
        &self,
        request: Request<isolated_container_service::StopContainerRequest>,
    ) -> Result<Response<isolated_container_service::StopContainerResponse>, Status> {
        info!("Got stop_container request");

        let container_id: String = request.get_ref().uuid.clone();

        Ok(Response::new(
            isolated_container_service::StopContainerResponse {},
        ))
    }

    async fn state_container(
        &self,
        request: Request<isolated_container_service::StateContainerRequest>,
    ) -> Result<Response<isolated_container_service::StateContainerResponse>, Status> {
        info!("Got state_container request");

        let container_id: String = request.get_ref().uuid.clone();

        Ok(Response::new(
            isolated_container_service::StateContainerResponse {
                state: "".to_string(),
                pid: None,
            },
        ))
    }
}
