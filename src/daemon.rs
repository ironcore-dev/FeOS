use log::info;
use std::path::PathBuf;
use tonic::{transport::Server, Request, Response, Status};

use feos_grpc::feos_grpc_server::{FeosGrpc, FeosGrpcServer};
use feos_grpc::Empty;
use tokio::time::Duration;
use uuid::Uuid;

use self::feos_grpc::{
    BootVmRequest, BootVmResponse, CreateVmRequest, CreateVmResponse, FetchImageRequest,
    FetchImageResponse, GetVmRequest, GetVmResponse,
};
use crate::vm::{self};

pub mod feos_grpc {
    tonic::include_proto!("feos_grpc"); // The string specified here must match the proto package name
}

#[derive(Debug, Default)]
pub struct FeOSAPI {
    vmm: vm::Manager,
}

#[tonic::async_trait]
impl FeosGrpc for FeOSAPI {
    async fn ping(
        &self,
        request: Request<Empty>, // Accept request of type HelloRequest
    ) -> Result<Response<Empty>, Status> {
        // Return an instance of type HelloReply
        println!("Got a request: {:?}", request);

        let reply = feos_grpc::Empty {};

        Ok(Response::new(reply)) // Send back our formatted greeting
    }

    async fn fetch_image(
        &self,
        request: Request<FetchImageRequest>,
    ) -> Result<Response<FetchImageResponse>, Status> {
        info!("Got create_vm request");

        let id = Uuid::new_v4();
        let path: PathBuf = PathBuf::from(format!("./images/{}", id.clone()));
        tokio::spawn(async move {
            match vm::image::fetch_image(request.get_ref().image.clone(), path).await {
                Ok(_) => info!("image pulled"),
                Err(e) => info!("failed to pull image: {:?}", e),
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
        self.vmm.init_vmm(id, true).map_err(|e| {
            info!("failed to init vvm: {:?}", e);
            Status::unknown("failed to init vvm")
        })?;

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
            )
            .map_err(|e| {
                info!("failed to create vm: {:?}", e);
                Status::unknown("failed to create vm")
            })?;

        Ok(Response::new(feos_grpc::CreateVmResponse {
            uuid: id.to_string(),
        }))
    }

    async fn boot_vm(
        &self,
        request: Request<BootVmRequest>,
    ) -> Result<Response<BootVmResponse>, Status> {
        info!("Got boot_vm request");

        let id = request.get_ref().uuid.to_owned();
        let id =
            Uuid::parse_str(&id).map_err(|_| Status::invalid_argument("failed to parse uuid"))?;
        self.vmm.boot_vm(id).map_err(|e| {
            info!("failed to boot vm: {:?}", e);
            Status::unknown("failed to boot vm")
        })?;

        Ok(Response::new(feos_grpc::BootVmResponse {}))
    }

    async fn get_vm(
        &self,
        request: Request<GetVmRequest>,
    ) -> Result<Response<GetVmResponse>, Status> {
        info!("Got get_vm request");

        let id = request.get_ref().uuid.to_owned();
        let id =
            Uuid::parse_str(&id).map_err(|_| Status::invalid_argument("failed to parse uuid"))?;
        self.vmm.ping_vmm(id).map_err(|e| {
            info!("failed to ping vvm: {:?}", e);
            Status::unknown("failed to ping vvm")
        })?;
        let vm_status = self.vmm.get_vm(id).map_err(|e| {
            info!("failed to get vm: {:?}", e);
            Status::unknown("failed to get vm")
        })?;

        Ok(Response::new(feos_grpc::GetVmResponse { info: vm_status }))
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
