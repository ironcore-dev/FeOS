use container_service::container_service_server::ContainerService;
use libcontainer::container::builder::ContainerBuilder;
use libcontainer::container::Container;
use libcontainer::signal::Signal;
use libcontainer::syscall::syscall::SyscallType;
use log::info;
use oci::{fetch_image, DEFAULT_IMAGE_PATH};
use std::fs::File;
use std::{fmt::Debug, path::PathBuf};
use tonic::{Request, Response, Status};
pub mod oci;
use flate2::read::GzDecoder;
use libcontainer::oci_spec::runtime::{LinuxNamespace, LinuxNamespaceType, Spec};
use libcontainer::workload::default::DefaultExecutor;
use serde_json::to_writer_pretty;
use std::fs;
use std::io::BufReader;
use std::io::{BufWriter, Write};
use tar::Archive;
use uuid::Uuid;

pub const DEFAULT_CONTAINER_PATH: &str = "/var/lib/feos/containers";

pub mod container_service {
    tonic::include_proto!("container");
}

#[derive(Debug, Default)]
pub struct ContainerAPI {}

fn create(
    id: String,
    bundle: PathBuf,
    socket: Option<PathBuf>,
) -> anyhow::Result<libcontainer::container::Container> {
    let container = ContainerBuilder::new(id, SyscallType::default())
        .with_executor(DefaultExecutor {})
        .with_root_path("/run/containers/youki")?
        .with_console_socket(socket)
        .validate_id()?
        .as_init(bundle)
        .with_systemd(false)
        .with_detach(true)
        .build()?;

    Ok(container)
}

#[tonic::async_trait]
impl ContainerService for ContainerAPI {
    async fn create_container(
        &self,
        request: Request<container_service::CreateContainerRequest>,
    ) -> Result<Response<container_service::CreateContainerResponse>, Status> {
        info!("Got create_container request");

        let id: Uuid = Uuid::new_v4();

        let digest = fetch_image(request.get_ref().image.to_owned())
            .await
            .map_err(|e| {
                Status::new(
                    tonic::Code::Internal,
                    format!("failed to fetch image: {}", e),
                )
            })?;

        let mut bundle_path = PathBuf::from(DEFAULT_CONTAINER_PATH);
        bundle_path.push(id.to_string());

        let mut rootfs_path = bundle_path.clone();
        rootfs_path.push("rootfs");
        fs::create_dir_all(&rootfs_path)?;

        info!("unpacking image content");
        let src = format!("{}/{}", DEFAULT_IMAGE_PATH, digest);
        let paths = fs::read_dir(PathBuf::from(&src))?;
        for path in paths {
            let path = path?.path();

            if path.is_file() {
                let file = File::open(&path)?;
                let gz_decoder = GzDecoder::new(BufReader::new(file));
                let mut archive = Archive::new(gz_decoder);

                archive.unpack(&rootfs_path)?;
            }
        }

        let mut spec = Spec::default();
        let linux = spec
            .linux_mut()
            .as_mut()
            .ok_or_else(|| Status::new(tonic::Code::Internal, ""))?;
        let ns: &mut Vec<LinuxNamespace> = linux
            .namespaces_mut()
            .as_mut()
            .ok_or_else(|| Status::new(tonic::Code::Internal, ""))?;
        ns.retain(|ns| ns.typ() != LinuxNamespaceType::Network);

        if let Some(mut process) = spec.process_mut().take() {
            process.set_args(Some(request.get_ref().command.to_owned()));
            spec.set_process(Some(process));
        }

        let mut cfg_file = bundle_path.clone();
        cfg_file.push("config.json");
        let cfg_file = File::create(cfg_file).expect("msg");
        let mut writer = BufWriter::new(cfg_file);
        to_writer_pretty(&mut writer, &spec).expect("msg");
        writer.flush().expect("msg");

        // let socket = PathBuf::from("/home/lukasfrank/dev/sample-nginx-pod/sock.tty");

        let _ = create(id.to_string(), bundle_path, None).expect("msg");

        Ok(Response::new(container_service::CreateContainerResponse {
            uuid: id.to_string(),
        }))
    }

    async fn run_container(
        &self,
        request: Request<container_service::RunContainerRequest>,
    ) -> Result<Response<container_service::RunContainerResponse>, Status> {
        info!("Got run_container request");

        let container_id: String = request.get_ref().uuid.clone();

        let container_root = PathBuf::from(format!("/run/containers/youki/{}", container_id));
        if !container_root.exists() {
            info!("container {} does not exist.", container_id)
        }

        let mut container = Container::load(container_root).expect("msg");
        container.start().expect("msg");

        Ok(Response::new(container_service::RunContainerResponse {}))
    }

    async fn kill_container(
        &self,
        request: Request<container_service::KillContainerRequest>,
    ) -> Result<Response<container_service::KillContainerResponse>, Status> {
        info!("Got kill_container request");

        let container_id: String = request.get_ref().uuid.clone();

        let container_root = PathBuf::from(format!("/run/containers/youki/{}", container_id));
        if !container_root.exists() {
            info!("container {} does not exist.", container_id)
        }

        let signal: Signal = "9".try_into().expect("msg");

        let mut container = Container::load(container_root).expect("msg");
        container.kill(signal, false).expect("msg");

        Ok(Response::new(container_service::KillContainerResponse {}))
    }

    async fn state_container(
        &self,
        request: Request<container_service::StateContainerRequest>,
    ) -> Result<Response<container_service::StateContainerResponse>, Status> {
        info!("Got state_container request");

        let container_id: String = request.get_ref().uuid.clone();

        let container_root = PathBuf::from(format!("/run/containers/youki/{}", container_id));
        if !container_root.exists() {
            info!("container {} does not exist.", container_id)
        }

        let mut container = Container::load(container_root.clone()).expect("msg");
        let _ = container.refresh_status().expect("msg");

        info!("{:?}", container.state);

        Ok(Response::new(container_service::StateContainerResponse {
            state: container.state.status.to_string(),
            pid: container.state.pid,
        }))
    }

    async fn delete_container(
        &self,
        request: Request<container_service::DeleteContainerRequest>,
    ) -> Result<Response<container_service::DeleteContainerResponse>, Status> {
        info!("Got delete_container request");

        let container_id: String = request.get_ref().uuid.clone();

        let container_root = PathBuf::from(format!("/run/containers/youki/{}", container_id));
        if !container_root.exists() {
            info!("container {} does not exist.", container_id);
            return Ok(Response::new(container_service::DeleteContainerResponse {}));
        }

        let mut container = Container::load(container_root).expect("msg");
        container.delete(false).expect("msg");

        let bundle_path = PathBuf::from(format!("{}/{}", DEFAULT_CONTAINER_PATH, container_id));
        if bundle_path.exists() {
            fs::remove_dir_all(bundle_path).expect("msg");
        }

        Ok(Response::new(container_service::DeleteContainerResponse {}))
    }
}
