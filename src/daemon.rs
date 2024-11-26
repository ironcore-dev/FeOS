use log::{error, info, warn};
use std::net::Ipv6Addr;
use std::path::PathBuf;
use std::{env, io};
use tonic::{transport::Server, Request, Response, Status};

use crate::feos_grpc;
use crate::feos_grpc::feos_grpc_server::*;
use crate::feos_grpc::{
    attach_nic_vm_request::NicData, AttachNicVmRequest, AttachNicVmResponse, BootVmRequest,
    BootVmResponse, ConsoleVmResponse, CreateVmRequest, CreateVmResponse, Empty, FetchImageRequest,
    FetchImageResponse, GetFeOsKernelLogRequest, GetFeOsKernelLogResponse, GetFeOsLogRequest,
    GetFeOsLogResponse, GetVmRequest, GetVmResponse, HostInfoRequest, HostInfoResponse,
    NetInterface, NicType as ProtoNicType, PingVmRequest, PingVmResponse, RebootRequest,
    RebootResponse, ShutdownRequest, ShutdownResponse, ShutdownVmRequest, ShutdownVmResponse,
};
use crate::host;
use crate::ringbuffer::*;
use crate::vm::{self};
use crate::vm::{image, Manager};
use crate::{container, network};
use hyper_util::rt::TokioIo;
use nix::libc::VMADDR_CID_ANY;
use nix::unistd::Uid;
use std::sync::Arc;
use tokio::{
    fs::File,
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    sync::{mpsc, Mutex},
    time::{sleep, Duration},
};
use tokio_stream::wrappers::ReceiverStream;
use tokio_vsock::{VsockAddr, VsockListener};
use tonic::transport::{Endpoint, Uri};
use tower::service_fn;
use uuid::Uuid;

use crate::filesystem::mount_virtual_filesystems;
use crate::isolated_container::{isolated_container_service, IsolatedContainerAPI};
use crate::network::configure_network_devices;

#[derive(Debug)]
pub struct FeOSAPI {
    vmm: Arc<vm::Manager>,
    buffer: Arc<RingBuffer>,
    log_receiver: Arc<Mutex<mpsc::Receiver<String>>>,
    network: Arc<network::Manager>,
}

impl FeOSAPI {
    pub fn new(
        vmm: Arc<vm::Manager>,
        buffer: Arc<RingBuffer>,
        log_receiver: Arc<Mutex<mpsc::Receiver<String>>>,
        network: Arc<network::Manager>,
    ) -> Self {
        FeOSAPI {
            vmm,
            buffer,
            log_receiver,
            network,
        }
    }

    fn handle_error(&self, e: vm::Error) -> Status {
        match e {
            vm::Error::AlreadyExists => {
                Status::new(tonic::Code::AlreadyExists, "vm already exists")
            }
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
            vm::Error::NetworkingError(e) => {
                info!("failed to prepare network {:?}", e);
                Status::new(
                    tonic::Code::Internal,
                    format!("failed to prepare network: {:?}", e),
                )
            }
            vm::Error::CHApiFailure(e) => {
                info!("failed to connect to cloud hypervisor api: {:?}", e);
                Status::new(
                    tonic::Code::Internal,
                    "failed to connect to cloud hypervisor api",
                )
            }
        }
    }
}

#[tonic::async_trait]
impl FeosGrpc for FeOSAPI {
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

    async fn ping(
        &self,
        request: Request<Empty>, // Accept request of type HelloRequest
    ) -> Result<Response<Empty>, Status> {
        // Return an instance of type HelloReply
        info!("Got a request: {:?}", request);

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

    async fn create_vm(
        &self,
        request: Request<CreateVmRequest>,
    ) -> Result<Response<CreateVmResponse>, Status> {
        info!("Got create_vm request");

        let id = Uuid::new_v4();
        self.vmm
            .init_vmm(id, true)
            .map_err(|e| self.handle_error(e))?;

        let root_fs = PathBuf::from(format!(
            "./images/{}/application.vnd.ironcore.image.rootfs.v1alpha1.rootfs",
            request.get_ref().image_uuid
        ));
        self.vmm
            .create_vm(
                id,
                request.get_ref().cpu,
                request.get_ref().memory_bytes,
                vm::BootMode::FirmwareBoot(vm::FirmwareBootMode { root_fs }),
                request.get_ref().ignition.clone(),
            )
            .map_err(|e| self.handle_error(e))?;

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
        self.vmm.ping_vmm(id).map_err(|e| self.handle_error(e))?;
        let vm_status = self.vmm.get_vm(id).map_err(|e| self.handle_error(e))?;

        Ok(Response::new(feos_grpc::GetVmResponse { info: vm_status }))
    }

    async fn boot_vm(
        &self,
        request: Request<BootVmRequest>,
    ) -> Result<Response<BootVmResponse>, Status> {
        info!("Received boot_vm request");

        let id = Uuid::parse_str(&request.get_ref().uuid)
            .map_err(|_| Status::invalid_argument("Failed to parse UUID"))?;

        self.vmm.boot_vm(id).map_err(|e| self.handle_error(e))?;
        sleep(Duration::from_secs(2)).await;
        self.network
            .start_dhcp(id)
            .await
            .map_err(vm::Error::NetworkingError)
            .map_err(|e| self.handle_error(e))?;

        Ok(Response::new(feos_grpc::BootVmResponse {}))
    }

    type ConsoleVMStream = ReceiverStream<Result<feos_grpc::ConsoleVmResponse, Status>>;

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
        let socket_path = self
            .vmm
            .get_vm_console_path(id)
            .map_err(|e| self.handle_error(e))?;

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

    async fn attach_nic_vm(
        &self,
        request: Request<AttachNicVmRequest>,
    ) -> Result<Response<AttachNicVmResponse>, Status> {
        info!("Received AttachNicVM request");

        let req = request.into_inner();

        let vm_uuid = Uuid::parse_str(&req.uuid)
            .map_err(|_| Status::invalid_argument("Failed to parse UUID"))?;

        let net_config = match ProtoNicType::try_from(req.nic_type) {
            Ok(ProtoNicType::Mac) => {
                if let Some(NicData::MacAddress(mac)) = req.nic_data {
                    vm::NetworkMode::MACAddress(mac)
                } else {
                    return Err(Status::invalid_argument(
                        "mac_address must be provided for NIC type MAC",
                    ));
                }
            }
            Ok(ProtoNicType::Pci) => {
                if let Some(NicData::PciAddress(pci)) = req.nic_data {
                    vm::NetworkMode::PCIAddress(pci)
                } else {
                    return Err(Status::invalid_argument(
                        "pci_address must be provided for NIC type PCI",
                    ));
                }
            }
            Ok(ProtoNicType::Tap) => {
                let tap_name = network::Manager::device_name(&vm_uuid);
                vm::NetworkMode::TAPDeviceName(tap_name)
            }
            Err(_) => {
                return Err(Status::invalid_argument("Invalid NIC type provided"));
            }
        };

        self.vmm
            .add_net_device(vm_uuid, net_config)
            .map_err(|e| self.handle_error(e))?;

        Ok(Response::new(AttachNicVmResponse {}))
    }

    async fn shutdown_vm(
        &self,
        request: Request<ShutdownVmRequest>,
    ) -> Result<Response<ShutdownVmResponse>, Status> {
        info!("Received shutdown_vm request");

        let id = Uuid::parse_str(&request.get_ref().uuid)
            .map_err(|_| Status::invalid_argument("Failed to parse UUID"))?;

        self.network
            .stop_dhcp(id)
            .await
            .map_err(vm::Error::NetworkingError)
            .map_err(|e| self.handle_error(e))?;

        // TODO differentiate between kill and shutdown
        self.vmm.kill_vm(id).map_err(|e| self.handle_error(e))?;

        Ok(Response::new(feos_grpc::ShutdownVmResponse {}))
    }

    async fn ping_vm(
        &self,
        request: Request<PingVmRequest>,
    ) -> Result<Response<PingVmResponse>, Status> {
        info!("Received ping_vm request");

        let id = Uuid::parse_str(&request.get_ref().uuid)
            .map_err(|_| Status::invalid_argument("Failed to parse UUID"))?;
        let path = format!("vsock{}.sock", network::Manager::vm_tap_name(&id));
        let path_clone = path.clone();

        let channel = Endpoint::try_from("http://[::]:50051")
            .map_err(|e| Status::internal(format!("Failed to create endpoint: {}", e)))?
            .connect_with_connector(service_fn(move |_: Uri| {
                let path = path_clone.clone();
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
            .map_err(|e| Status::internal(format!("Failed to connect: {}", e)))?;

        let mut client = feos_grpc::feos_grpc_client::FeosGrpcClient::new(channel);
        let request = tonic::Request::new(Empty {});
        let _response = client.ping(request).await?;

        Ok(Response::new(feos_grpc::PingVmResponse {}))
    }

    type GetFeOSKernelLogsStream = ReceiverStream<Result<GetFeOsKernelLogResponse, Status>>;

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

    type GetFeOSLogsStream = ReceiverStream<Result<GetFeOsLogResponse, Status>>;

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
}

pub async fn daemon_start(
    vmm: Arc<vm::Manager>,
    network: Arc<network::Manager>,
    buffer: Arc<RingBuffer>,
    log_receiver: Arc<Mutex<mpsc::Receiver<String>>>,
    is_nested: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let api = FeOSAPI::new(vmm.clone(), buffer, log_receiver, network.clone());
    let isolated_container_api = IsolatedContainerAPI::new(vmm, network);

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
            .add_service(isolated_container_service::isolated_container_service_server::IsolatedContainerServiceServer::new(isolated_container_api))
            .serve(addr)
            .await?;
    }

    Ok(())
}

pub async fn start_feos(mut ipv6_address: Ipv6Addr, mut prefix_length: u8) -> Result<(), String> {
    println!(
        "
    ███████╗███████╗ ██████╗ ███████╗
    ██╔════╝██╔════╝██╔═══██╗██╔════╝
    █████╗  █████╗  ██║   ██║███████╗
    ██╔══╝  ██╔══╝  ██║   ██║╚════██║
    ██║     ███████╗╚██████╔╝███████║
    ╚═╝     ╚══════╝ ╚═════╝ ╚══════╝
                 v{}
    ",
        env!("CARGO_PKG_VERSION")
    );

    const FEOS_RINGBUFFER_CAP: usize = 100;
    let buffer = RingBuffer::new(FEOS_RINGBUFFER_CAP);
    let log_receiver = init_logger(buffer.clone());

    // If not run as root, print warning.
    if !Uid::current().is_root() {
        warn!("Not running as root! (uid: {})", Uid::current());
    }

    if ipv6_address == Ipv6Addr::UNSPECIFIED {
        info!("No --ipam flag found. Expecting Prefix Delegation from the dhcpv6 server");
    }

    if std::process::id() == 1 {
        info!("Mounting virtual filesystems...");
        mount_virtual_filesystems();
    } else {
        info!(
            "IPv6 Address: {}, Prefix Length: {}",
            ipv6_address, prefix_length
        );
    }

    let is_nested = is_running_on_vm().await.unwrap_or_else(|e| {
        error!("Error checking VM status: {}", e);
        false // Default to false in case of error
    });

    if std::process::id() == 1 {
        info!("Configuring network devices...");
        if let Some((delegated_prefix, delegated_prefix_length)) = configure_network_devices()
            .await
            .expect("could not configure network devices")
        {
            ipv6_address = delegated_prefix;
            prefix_length = delegated_prefix_length;
        }
    }

    // Special stuff for pid 1
    if std::process::id() == 1 && !is_nested {
        info!("Skip configuring sriov...");
        /*const VFS_NUM: u32 = 125;
        if let Err(e) = configure_sriov(VFS_NUM).await {
            warn!("failed to configure sriov: {}", e.to_string())
        }*/
    }

    let vmm = Manager::new(String::from("cloud-hypervisor"));
    let network_manager = network::Manager::new(ipv6_address, prefix_length);

    info!("Starting FeOS daemon...");
    match daemon_start(
        Arc::new(vmm),
        Arc::new(network_manager),
        buffer,
        log_receiver,
        is_nested,
    )
    .await
    {
        Err(e) => {
            error!("FeOS daemon crashed: {}", e);
            Err(format!("FeOS daemon crashed: {}", e))
        }
        Ok(_) => {
            error!("FeOS daemon exited.");
            Err("FeOS exited".to_string())
        }
    }
}

async fn is_running_on_vm() -> Result<bool, Box<dyn std::error::Error>> {
    let files = [
        "/sys/class/dmi/id/product_name",
        "/sys/class/dmi/id/sys_vendor",
    ];

    let mut match_count = 0;

    for file_path in files.iter() {
        let mut file = File::open(file_path).await?;
        let mut contents = String::new();
        file.read_to_string(&mut contents).await?;

        let lowercase_contents = contents.to_lowercase();
        if lowercase_contents.contains("cloud") && lowercase_contents.contains("hypervisor") {
            match_count += 1;
        }
    }

    Ok(match_count == 2)
}
