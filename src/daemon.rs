use log::info;
use std::path::PathBuf;
use tonic::{transport::Server, Request, Response, Status};

use crate::host;
use crate::vm::image;
use feos_grpc::feos_grpc_server::{FeosGrpc, FeosGrpcServer};
use tokio::time::{sleep, Duration};
use uuid::Uuid;

use self::feos_grpc::{
    AttachNicVmRequest, AttachNicVmResponse, BootVmRequest, BootVmResponse, CreateVmRequest,
    CreateVmResponse, Empty, FetchImageRequest, FetchImageResponse, GetVmRequest, GetVmResponse,
    HostInfoRequest, HostInfoResponse, NetInterface, RebootRequest, RebootResponse,
    ShutdownRequest, ShutdownResponse,
};
use crate::vm::{self};

pub mod feos_grpc {
    tonic::include_proto!("feos_grpc"); // The string specified here must match the proto package name
}

#[derive(Debug, Default)]
pub struct FeOSAPI {
    vmm: vm::Manager,
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

    async fn console_vm(
        &self,
        request: Request<feos_grpc::ConsoleVmRequest>,
    ) -> Result<Response<feos_grpc::ConsoleVmResponse>, Status> {
        info!("Got console_vm request");

        let id = request.get_ref().uuid.to_owned();
        let id =
            Uuid::parse_str(&id).map_err(|_| Status::invalid_argument("failed to parse uuid"))?;
        self.vmm.console(id).map_err(handle_error)?;

        Ok(Response::new(feos_grpc::ConsoleVmResponse {}))
    }
}

pub async fn daemon_start(vmm: vm::Manager) -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::]:1337".parse()?;

    let api = FeOSAPI { vmm };

    Server::builder()
        .timeout(Duration::from_secs(30))
        .add_service(FeosGrpcServer::new(api))
        .serve(addr)
        .await?;

    Ok(())
}
