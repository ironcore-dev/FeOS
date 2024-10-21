use crate::container::container_service::container_service_client::ContainerServiceClient;
use crate::container::container_service::{
    CreateContainerRequest, RunContainerRequest, StateContainerRequest,
};
use crate::vm::NetworkMode;
use crate::{network, vm};
use hyper_util::rt::TokioIo;
use isolated_container_service::isolated_container_service_server::IsolatedContainerService;
use log::info;
use std::sync::Arc;
use std::{collections::HashMap, sync::Mutex};
use std::{fmt::Debug, io, path::PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tonic::transport::{Channel, Endpoint, Uri};
use tonic::{transport, Request, Response, Status};
use tower::service_fn;
use uuid::Uuid;

pub mod isolated_container_service {
    tonic::include_proto!("isolated_container");
}

#[derive(Debug, Default)]
pub struct IsolatedContainerAPI {
    vmm: Arc<vm::Manager>,
    network: Arc<network::Manager>,
    vm_to_container: Mutex<HashMap<Uuid, IsolatedContainerInfo>>,
}

#[derive(Debug, Clone)]
struct IsolatedContainerInfo {
    pub container_id: Uuid,
    pub sock: Channel,
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

#[derive(Debug)]
pub enum Error {
    VMConnectionError(transport::Error),
    VMConnectionMaxRetriesError,
    VMError(vm::Error),
    NetworkingError(network::Error),
    InvalidID,
    Error(String),
}

async fn get_channel(path: String) -> Result<Channel, Error> {
    async fn establish_connection(path: String) -> Result<Channel, tonic::transport::Error> {
        let endpoint = Endpoint::try_from("http://[::]:50051")?;

        let connector = service_fn(move |_: Uri| {
            let path = path.clone();
            async move {
                let mut stream = UnixStream::connect(&path).await?;

                let connect_cmd = format!("CONNECT {}\n", 1337);
                stream
                    .write_all(connect_cmd.as_bytes())
                    .await
                    .map_err(|e| {
                        io::Error::new(io::ErrorKind::Other, format!("Write error: {}", e))
                    })?;

                let mut buffer = [0u8; 128];
                let n = stream.read(&mut buffer).await?;

                let response = String::from_utf8_lossy(&buffer[..n]);
                if !response.starts_with("OK") {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("Failed to connect to vsock: {}", response.trim()),
                    ));
                }

                info!("Connected to vsock: {}", response.trim());
                Ok::<_, io::Error>(TokioIo::new(stream))
            }
        });

        endpoint.connect_with_connector(connector).await
    }

    const RETRIES: u8 = 20;
    const DELAY: tokio::time::Duration = tokio::time::Duration::from_millis(2000);

    for attempt in 0..RETRIES {
        match establish_connection(path.clone()).await {
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

    Err(Error::VMConnectionMaxRetriesError)
}

impl IsolatedContainerAPI {
    fn prepare_vm(&self, id: uuid::Uuid) -> Result<(), Error> {
        self.vmm.init_vmm(id, true).map_err(Error::VMError)?;
        self.vmm
            .create_vm(
                id,
                2,
                4294967296,
                vm::BootMode::KernelBoot(vm::KernelBootMode {
                    kernel: PathBuf::from("/usr/share/feos/vmlinuz"),
                    initramfs: PathBuf::from("/usr/share/feos/initramfs"),
                    // TODO
                    cmdline: "console=tty0 console=ttyS0,115200 intel_iommu=on iommu=pt"
                        .to_string(),
                }),
                None,
            )
            .map_err(Error::VMError)?;

        self.vmm.boot_vm(id).map_err(Error::VMError)?;

        self.vmm
            .add_net_device(
                id,
                NetworkMode::TAPDeviceName(network::Manager::device_name(&id)),
            )
            .map_err(Error::VMError)?;

        Ok(())
    }

    fn handle_error(&self, e: Error) -> tonic::Status {
        match e {
            Error::VMConnectionError(e) => Status::new(
                tonic::Code::Internal,
                format!("failed to connect to vm: {}", e),
            ),
            Error::VMConnectionMaxRetriesError => Status::new(
                tonic::Code::Internal,
                format!("failed to connect to vm: mac retries reached: {:?}", e),
            ),
            Error::VMError(e) => Status::new(
                tonic::Code::Internal,
                format!("failed to prepare vm: {:?}", e),
            ),
            Error::NetworkingError(e) => Status::new(
                tonic::Code::Internal,
                format!("failed to prepare network: {:?}", e),
            ),
            Error::InvalidID => Status::invalid_argument("failed to parse uuid"),
            Error::Error(m) => Status::internal(m),
        }
    }

    fn get_container_info(&self, vm_id: Uuid) -> Result<IsolatedContainerInfo, Error> {
        let container_id = {
            let vm_to_container = self
                .vm_to_container
                .lock()
                .map_err(|_| Error::Error("Failed to lock mutex".to_string()))?;
            vm_to_container
                .get(&vm_id)
                .cloned()
                .ok_or_else(|| Error::Error(format!("VM with ID '{}' not found", vm_id)))?
        };

        Ok(container_id)
    }
}

#[tonic::async_trait]
impl IsolatedContainerService for IsolatedContainerAPI {
    async fn create_container(
        &self,
        request: Request<isolated_container_service::CreateContainerRequest>,
    ) -> Result<Response<isolated_container_service::CreateContainerResponse>, Status> {
        info!("Got create_container request");

        let id = Uuid::new_v4();

        self.prepare_vm(id).map_err(|e| self.handle_error(e))?;

        self.network
            .start_dhcp(id)
            .await
            .map_err(Error::NetworkingError)
            .map_err(|e| self.handle_error(e))?;

        let path = format!("vsock{}.sock", network::Manager::device_name(&id));
        let channel = get_channel(path).await.map_err(|e| self.handle_error(e))?;

        let mut client = ContainerServiceClient::new(channel.clone());
        let request = tonic::Request::new(CreateContainerRequest {
            image: request.get_ref().image.to_string(),
            command: request.get_ref().command.clone(),
        });
        let response = client.create_container(request).await?;
        println!("{}", response.get_ref().uuid);

        let container_id = Uuid::parse_str(&response.get_ref().uuid)
            .map_err(|_| Error::InvalidID)
            .map_err(|e| self.handle_error(e))?;

        let mut vm_to_container = self.vm_to_container.lock().unwrap();
        vm_to_container.insert(
            id,
            IsolatedContainerInfo {
                container_id,
                sock: channel,
            },
        );

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
            .map_err(|_| Error::InvalidID)
            .map_err(|e| self.handle_error(e))?;

        let container = self
            .get_container_info(vm_id)
            .map_err(|e| self.handle_error(e))?;

        let mut client = ContainerServiceClient::new(container.sock.clone());
        let request = tonic::Request::new(RunContainerRequest {
            uuid: container.container_id.to_string(),
        });
        client.run_container(request).await?;

        Ok(Response::new(
            isolated_container_service::RunContainerResponse {},
        ))
    }

    async fn stop_container(
        &self,
        request: Request<isolated_container_service::StopContainerRequest>,
    ) -> Result<Response<isolated_container_service::StopContainerResponse>, Status> {
        info!("Got stop_container request");

        let vm_id: String = request.get_ref().uuid.clone();
        let vm_id = Uuid::parse_str(&vm_id)
            .map_err(|_| Error::InvalidID)
            .map_err(|e| self.handle_error(e))?;

        self.network
            .stop_dhcp(vm_id)
            .await
            .map_err(Error::NetworkingError)
            .map_err(|e| self.handle_error(e))?;

        self.vmm
            .kill_vm(vm_id)
            .map_err(Error::VMError)
            .map_err(|e| self.handle_error(e))?;

        let mut vm_to_container = self.vm_to_container.lock().unwrap();
        vm_to_container.remove(&vm_id);

        Ok(Response::new(
            isolated_container_service::StopContainerResponse {},
        ))
    }

    async fn state_container(
        &self,
        request: Request<isolated_container_service::StateContainerRequest>,
    ) -> Result<Response<isolated_container_service::StateContainerResponse>, Status> {
        info!("Got state_container request");

        let vm_id: String = request.get_ref().uuid.clone();
        let vm_id = Uuid::parse_str(&vm_id)
            .map_err(|_| Error::InvalidID)
            .map_err(|e| self.handle_error(e))?;

        let container = self
            .get_container_info(vm_id)
            .map_err(|e| self.handle_error(e))?;

        let mut client = ContainerServiceClient::new(container.sock.clone());
        let request = tonic::Request::new(StateContainerRequest {
            uuid: container.container_id.to_string(),
        });
        let response = client.state_container(request).await?;

        Ok(Response::new(
            isolated_container_service::StateContainerResponse {
                state: response.get_ref().state.to_string(),
                pid: response.get_ref().pid,
            },
        ))
    }
}
