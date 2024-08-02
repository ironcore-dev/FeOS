use log::{error, info};
use std::path::PathBuf;
use tonic::{transport::Server, Request, Response, Status};

use crate::container;
use crate::host;
use crate::ringbuffer::*;
use crate::vm::image;
use feos_grpc::feos_grpc_server::{FeosGrpc, FeosGrpcServer};
use hyper_util::rt::TokioIo;
use nix::libc::VMADDR_CID_ANY;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{sleep, Duration};
use tokio_stream::wrappers::ReceiverStream;
use tokio_vsock::{VsockAddr, VsockListener};
use tonic::transport::{Endpoint, Uri};
use tower::service_fn;
use uuid::Uuid;

use self::feos_grpc::{
    AttachNicVmRequest, AttachNicVmResponse, BootVmRequest, BootVmResponse, ConsoleVmResponse,
    CreateVmRequest, CreateVmResponse, Empty, FetchImageRequest, FetchImageResponse,
    GetFeOsKernelLogRequest, GetFeOsKernelLogResponse, GetFeOsLogRequest, GetFeOsLogResponse,
    GetVmRequest, GetVmResponse, HostInfoRequest, HostInfoResponse, NetInterface, RebootRequest,
    RebootResponse, ShutdownRequest, ShutdownResponse,
};
use crate::vm::{self};

pub mod feos_grpc {
    tonic::include_proto!("feos_grpc"); // The string specified here must match the proto package name
}

#[derive(Debug)]
pub struct FeOSAPI {
    vmm: Arc<vm::Manager>,
    buffer: Arc<RingBuffer>,
    log_receiver: Arc<Mutex<mpsc::Receiver<String>>>,
}

impl FeOSAPI {
    pub fn new(
        vmm: vm::Manager,
        buffer: Arc<RingBuffer>,
        log_receiver: Arc<Mutex<mpsc::Receiver<String>>>,
    ) -> Self {
        FeOSAPI {
            vmm: Arc::new(vmm),
            buffer,
            log_receiver,
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

#[tonic::async_trait]
impl FeosGrpc for FeOSAPI {
    type GetFeOSKernelLogsStream = ReceiverStream<Result<GetFeOsKernelLogResponse, Status>>;
    type GetFeOSLogsStream = ReceiverStream<Result<GetFeOsLogResponse, Status>>;
    type ConsoleVMStream = ReceiverStream<Result<feos_grpc::ConsoleVmResponse, Status>>;

    async fn get_fe_os_kernel_logs(
        &self,
        _: Request<GetFeOsKernelLogRequest>,
    ) -> Result<Response<Self::GetFeOSKernelLogsStream>, Status> {
        let (tx, rx) = mpsc::channel(4);
        let tx = tx.clone();

        tokio::spawn(async move {
            let file = File::open("/dev/kmsg")
                .await
                .expect("Failed to open /dev/kmsg");
            let reader = BufReader::new(file);
            let mut lines = reader.lines();

            while let Some(line) = lines.next_line().await.unwrap() {
                let response = GetFeOsKernelLogResponse { message: line };
                if tx.send(Ok(response)).await.is_err() {
                    break;
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn console_vm(
        &self,
        request: Request<tonic::Streaming<feos_grpc::ConsoleVmRequest>>,
    ) -> Result<Response<Self::ConsoleVMStream>, Status> {
        let mut input_stream = request.into_inner();
        let (tx, rx) = mpsc::channel(4);

        info!("Got console_vm request");
        let initial_request = match input_stream.message().await {
            Ok(Some(request)) => request,
            Ok(None) => {
                return Err(Status::new(
                    tonic::Code::InvalidArgument,
                    "No initial request received",
                ))
            }
            Err(status) => return Err(status),
        };
        let id = Uuid::parse_str(&initial_request.uuid)
            .map_err(|_| Status::invalid_argument("failed to parse uuid"))?;
        let socket_path = self.vmm.get_vm_console_path(id).map_err(handle_error)?;

        tokio::spawn(async move {
            let stream = match UnixStream::connect(&socket_path).await {
                Ok(stream) => stream,
                Err(e) => {
                    error!("Failed to connect to Unix socket: {:?}", e);
                    return;
                }
            };

            let (reader, writer) = stream.into_split();

            tokio::spawn(async move {
                let mut writer = writer;
                while let Ok(Some(req)) = input_stream.message().await {
                    let input_with_newline = format!("{}\n", req.input);
                    if let Err(e) = writer.write_all(input_with_newline.as_bytes()).await {
                        error!("Failed to write to console: {:?}", e);
                        break;
                    }
                }
            });

            let mut reader = reader;
            let mut buffer = vec![0; 1024];
            loop {
                match reader.read(&mut buffer).await {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        let message = String::from_utf8_lossy(&buffer[..n]).to_string();
                        let response = ConsoleVmResponse { message };
                        if tx.send(Ok(response)).await.is_err() {
                            error!("Failed to send response through channel");
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Failed to read from stream: {:?}", e);
                        break;
                    }
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn get_fe_os_logs(
        &self,
        _: Request<GetFeOsLogRequest>,
    ) -> Result<Response<ReceiverStream<Result<GetFeOsLogResponse, Status>>>, Status> {
        let (tx, rx) = mpsc::channel(4);
        let buffer = self.buffer.clone();
        let log_receiver = self.log_receiver.clone();

        tokio::spawn(async move {
            let logs = buffer.get_lines().await;
            for log in logs {
                let response = GetFeOsLogResponse { message: log };
                if tx.send(Ok(response)).await.is_err() {
                    break;
                }
            }

            let mut log_receiver = log_receiver.lock().await;
            while let Some(log_entry) = log_receiver.recv().await {
                let response = GetFeOsLogResponse { message: log_entry };
                if tx.send(Ok(response)).await.is_err() {
                    break;
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn ping(
        &self,
        request: Request<Empty>, // Accept request of type HelloRequest
    ) -> Result<Response<Empty>, Status> {
        // Return an instance of type HelloReply
        info!("Got a request: {:?}", request);

        if !self.vmm.is_nested {
            let channel = Endpoint::try_from("http://[::]:50051")
                .map_err(|e| Status::internal(format!("Failed to create endpoint: {}", e)))?
                .connect_with_connector(service_fn(|_: Uri| async {
                    let path = "vsock.sock";

                    let mut stream = UnixStream::connect(path).await?;
                    let connect_cmd = format!("CONNECT {}\n", 1337);
                    stream.write_all(connect_cmd.as_bytes()).await?;

                    let mut buffer = [0u8; 128];
                    let n = stream.read(&mut buffer).await?;
                    let response = String::from_utf8_lossy(&buffer[..n]);
                    // Parse the response
                    if !response.starts_with("OK") {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            format!("Failed to connect to vsock: {}", response.trim()),
                        ));
                    }
                    info!("Connected to vsock: {}", response.trim());
                    // Connect to an Uds socket
                    Ok::<_, std::io::Error>(TokioIo::new(stream))
                }))
                .await
                .map_err(|e| Status::internal(format!("Failed to connect: {}", e)))?;

            let mut client = feos_grpc::feos_grpc_client::FeosGrpcClient::new(channel);
            let request = tonic::Request::new(Empty {});
            let _response = client.ping(request).await?;
        }

        let reply = feos_grpc::Empty {};

        Ok(Response::new(reply)) // Send back our formatted greeting
    }

    async fn fetch_image(
        &self,
        request: Request<FetchImageRequest>,
    ) -> Result<Response<FetchImageResponse>, Status> {
        info!("Got fetch_image request");

        let id = Uuid::new_v4();
        let path: PathBuf = PathBuf::from(format!("./images/{}", id.clone()));
        tokio::spawn(async move {
            match vm::image::fetch_image(request.get_ref().image.clone(), path).await {
                Ok(_) => info!("image pulled"),
                Err(image::ImageError::IOError(e)) => {
                    info!("failed to pull image: io error: {:?}", e)
                }
                Err(image::ImageError::PullError(e)) => info!("failed to pull image: {:?}", e),
                Err(image::ImageError::InvalidReference(e)) => {
                    info!("failed to pull image: invalid reference: {:?}", e)
                }
                Err(image::ImageError::MissingLayer(e)) => {
                    info!("failed to pull image: missing layer: {:?}", e)
                }
            }
        });

        Ok(Response::new(feos_grpc::FetchImageResponse {
            uuid: id.to_string(),
        }))
    }

    async fn reboot(&self, _: Request<RebootRequest>) -> Result<Response<RebootResponse>, Status> {
        info!("Got reboot request");
        tokio::spawn(async {
            sleep(Duration::from_secs(1)).await;
            match host::power::reboot() {
                Ok(_) => info!("reboot"),
                Err(e) => info!("failed to reboot: {:?}", e),
            }
        });
        Ok(Response::new(feos_grpc::RebootResponse {}))
    }

    async fn shutdown(
        &self,
        _: Request<ShutdownRequest>,
    ) -> Result<Response<ShutdownResponse>, Status> {
        info!("Got shutdown request");
        tokio::spawn(async {
            sleep(Duration::from_secs(1)).await;
            match host::power::shutdown() {
                Ok(_) => info!("shutdown"),
                Err(e) => info!("failed to shutdown: {:?}", e),
            }
        });
        Ok(Response::new(feos_grpc::ShutdownResponse {}))
    }

    async fn attach_nic_vm(
        &self,
        request: Request<AttachNicVmRequest>,
    ) -> Result<Response<AttachNicVmResponse>, Status> {
        info!("Got attach_nic_vm request");

        let id = request.get_ref().uuid.to_owned();
        let id =
            Uuid::parse_str(&id).map_err(|_| Status::invalid_argument("failed to parse uuid"))?;

        let mac_address = if request.get_ref().mac_address.is_empty() {
            None
        } else {
            Some(request.get_ref().mac_address.clone())
        };
        let pci_address = if request.get_ref().pci_address.is_empty() {
            None
        } else {
            Some(request.get_ref().pci_address.clone())
        };

        self.vmm
            ._add_net_device(id, mac_address, pci_address)
            .map_err(handle_error)?;

        Ok(Response::new(feos_grpc::AttachNicVmResponse {}))
    }

    async fn host_info(
        &self,
        _: Request<HostInfoRequest>,
    ) -> Result<Response<HostInfoResponse>, Status> {
        info!("Got host info request");

        let host = host::info::check_info();

        let mut interfaces = Vec::new();
        for interface in host.net_interfaces {
            interfaces.push(NetInterface {
                name: interface.name,
                pci_address: interface.pci_address.unwrap_or_default(),
                mac_address: interface.mac_address.unwrap_or_default(),
            })
        }

        Ok(Response::new(feos_grpc::HostInfoResponse {
            uptime: host.uptime,
            ram_total: host.ram_total,
            ram_unused: host.ram_unused,
            num_cores: host.num_cores,
            net_interfaces: interfaces,
        }))
    }

    async fn create_vm(
        &self,
        request: Request<CreateVmRequest>,
    ) -> Result<Response<CreateVmResponse>, Status> {
        info!("Got create_vm request");

        let id = Uuid::new_v4();
        self.vmm.init_vmm(id, true).map_err(handle_error)?;

        let root_fs = PathBuf::from(format!(
            "./images/{}/application.vnd.ironcore.image.rootfs.v1alpha1.rootfs",
            request.get_ref().image_uuid
        ));
        self.vmm
            .create_vm(
                id,
                request.get_ref().cpu,
                request.get_ref().memory_bytes,
                root_fs,
                request.get_ref().ignition.clone(),
            )
            .map_err(handle_error)?;

        Ok(Response::new(feos_grpc::CreateVmResponse {
            uuid: id.to_string(),
        }))
    }

    async fn get_vm(
        &self,
        request: Request<GetVmRequest>,
    ) -> Result<Response<GetVmResponse>, Status> {
        info!("Got get_vm request");

        let id = request.get_ref().uuid.to_owned();
        let id =
            Uuid::parse_str(&id).map_err(|_| Status::invalid_argument("failed to parse uuid"))?;
        self.vmm.ping_vmm(id).map_err(handle_error)?;
        let vm_status = self.vmm.get_vm(id).map_err(handle_error)?;

        Ok(Response::new(feos_grpc::GetVmResponse { info: vm_status }))
    }

    async fn boot_vm(
        &self,
        request: Request<BootVmRequest>,
    ) -> Result<Response<BootVmResponse>, Status> {
        info!("Got boot_vm request");

        let id = request.get_ref().uuid.to_owned();
        let id =
            Uuid::parse_str(&id).map_err(|_| Status::invalid_argument("failed to parse uuid"))?;
        self.vmm.boot_vm(id).map_err(handle_error)?;

        Ok(Response::new(feos_grpc::BootVmResponse {}))
    }
}

pub async fn daemon_start(
    vmm: vm::Manager,
    buffer: Arc<RingBuffer>,
    log_receiver: Arc<Mutex<mpsc::Receiver<String>>>,
    is_nested: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let api = FeOSAPI::new(vmm, buffer, log_receiver);

    if is_nested {
        let sockaddr = VsockAddr::new(VMADDR_CID_ANY, 1337);
        let vsock_listener = VsockListener::bind(sockaddr)?;
        Server::builder()
            .add_service(FeosGrpcServer::new(api))
            .add_service(
                container::container_service::container_service_server::ContainerServiceServer::new(
                    container::ContainerAPI {},
                ),
            )
            .serve_with_incoming(vsock_listener.incoming())
            .await?;
    } else {
        let addr = "[::]:1337".parse()?;
        Server::builder()
            .timeout(Duration::from_secs(30))
            .add_service(FeosGrpcServer::new(api))
            .add_service(
                container::container_service::container_service_server::ContainerServiceServer::new(
                    container::ContainerAPI {},
                ),
            )
            .serve(addr)
            .await?;
    }

    Ok(())
}
