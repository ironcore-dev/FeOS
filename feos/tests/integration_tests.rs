use anyhow::Result;
use image_service::{IMAGE_DIR, IMAGE_SERVICE_SOCKET};
use log::{error, info, warn};
use nix::sys::signal::{kill, Signal};
use nix::unistd::{self, Pid};
use once_cell::sync::OnceCell as SyncOnceCell;
use prost::Message;
use proto_definitions::{
    host_service::{host_service_client::HostServiceClient, HostnameRequest},
    image_service::{
        image_service_client::ImageServiceClient, DeleteImageRequest, ImageState,
        ListImagesRequest, PullImageRequest, WatchImageStatusRequest,
    },
    vm_service::{
        vm_service_client::VmServiceClient, CpuConfig, CreateVmRequest, DeleteVmRequest,
        GetVmRequest, MemoryConfig, PingVmRequest, StartVmRequest, StreamVmEventsRequest, VmConfig,
        VmEvent, VmState, VmStateChangedEvent,
    },
};
use std::env;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UnixStream;
use tokio::sync::OnceCell as TokioOnceCell;
use tokio::time::timeout;
use tokio_stream::StreamExt;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;
use vm_service::{VM_API_SOCKET_DIR, VM_CH_BIN};

const PUBLIC_SERVER_ADDRESS: &str = "http://[::1]:1337";
const TEST_IMAGE_REF: &str = "ghcr.io/ironcore-dev/os-images/gardenlinux-ch-dev";

static SERVER_RUNTIME: TokioOnceCell<Arc<tokio::runtime::Runtime>> = TokioOnceCell::const_new();
static TEMP_DIR_GUARD: SyncOnceCell<tempfile::TempDir> = SyncOnceCell::new();

async fn ensure_server() {
    SERVER_RUNTIME
        .get_or_init(|| async { setup_server().await })
        .await;
}

async fn setup_server() -> Arc<tokio::runtime::Runtime> {
    let _ = env_logger::builder().is_test(true).try_init();

    let temp_dir = TEMP_DIR_GUARD.get_or_init(|| {
        tempfile::Builder::new()
            .prefix("feos-test-")
            .tempdir()
            .expect("Failed to create temp dir")
    });

    let db_path = temp_dir.path().join("vms.db");
    let db_url = format!("sqlite:{}", db_path.to_str().unwrap());

    env::set_var("DATABASE_URL", &db_url);
    info!("Using temporary database for tests: {}", db_url);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create a new Tokio runtime for the server");

    runtime.spawn(async {
        if let Err(e) = main_server::run_server(false).await {
            panic!("Test server failed to run: {}", e);
        }
    });

    info!("Waiting for the server to start...");
    for _ in 0..20 {
        if Channel::from_static(PUBLIC_SERVER_ADDRESS)
            .connect()
            .await
            .is_ok()
        {
            info!("Server is up and running at {}", PUBLIC_SERVER_ADDRESS);
            return Arc::new(runtime);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    error!("Server did not start in time.");
    panic!("Server did not start in time.");
}

async fn get_public_clients() -> Result<(VmServiceClient<Channel>, HostServiceClient<Channel>)> {
    let vm_client = VmServiceClient::connect(PUBLIC_SERVER_ADDRESS).await?;
    let host_client = HostServiceClient::connect(PUBLIC_SERVER_ADDRESS).await?;
    Ok((vm_client, host_client))
}

async fn get_image_service_client() -> Result<ImageServiceClient<Channel>> {
    let endpoint = Endpoint::from_static("http://[::1]:50051");
    let channel = endpoint
        .connect_with_connector(service_fn(|_: Uri| {
            UnixStream::connect(IMAGE_SERVICE_SOCKET)
        }))
        .await?;
    Ok(ImageServiceClient::new(channel))
}

fn check_ch_binary() -> bool {
    Command::new("which")
        .arg(VM_CH_BIN)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn skip_if_ch_binary_missing() -> bool {
    if !check_ch_binary() {
        warn!(
            "Skipping test because '{}' binary was not found in PATH.",
            VM_CH_BIN
        );
        return true;
    }
    false
}

struct VmGuard {
    vm_id: String,
    pid: Option<Pid>,
    cleanup_disabled: bool,
}

impl Drop for VmGuard {
    fn drop(&mut self) {
        if self.cleanup_disabled {
            return;
        }
        info!("Cleaning up VM '{}'...", self.vm_id);
        if let Some(pid) = self.pid {
            info!("Killing process with PID: {}", pid);
            let _ = kill(pid, Signal::SIGKILL);
        }
        let socket_path = format!("{}/{}", VM_API_SOCKET_DIR, self.vm_id);
        if let Err(e) = std::fs::remove_file(&socket_path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                warn!("Could not remove socket file '{}': {}", socket_path, e);
            }
        } else {
            info!("Removed socket file '{}'", socket_path);
        }
    }
}

async fn wait_for_vm_state(
    stream: &mut tonic::Streaming<VmEvent>,
    target_state: VmState,
) -> Result<()> {
    while let Some(event_res) = stream.next().await {
        let event = event_res?;
        let any_data = event.data.expect("Event should have data payload");
        if any_data.type_url == "type.googleapis.com/feos.vm.vmm.api.v1.VmStateChangedEvent" {
            let state_change = VmStateChangedEvent::decode(&*any_data.value)?;
            let new_state =
                VmState::try_from(state_change.new_state).unwrap_or(VmState::Unspecified);

            info!(
                "Received VM state change event: new_state={:?}, reason='{}'",
                new_state, state_change.reason
            );

            if new_state == target_state {
                return Ok(());
            }

            if new_state == VmState::Crashed {
                let err_msg = format!("VM entered Crashed state. Reason: {}", state_change.reason);
                error!("{}", &err_msg);
                return Err(anyhow::anyhow!(err_msg));
            }
        }
    }
    Err(anyhow::anyhow!(
        "Event stream ended before VM reached {:?} state.",
        target_state
    ))
}

#[tokio::test]
async fn test_create_and_start_vm() -> Result<()> {
    if skip_if_ch_binary_missing() {
        return Ok(());
    }

    ensure_server().await;
    let (mut vm_client, _) = get_public_clients().await?;

    let image_ref = TEST_IMAGE_REF.to_string();
    let vm_config = VmConfig {
        cpus: Some(CpuConfig {
            boot_vcpus: 2,
            max_vcpus: 2,
        }),
        memory: Some(MemoryConfig { size_mib: 2048 }),
        image_ref,
        disks: vec![],
        net: vec![],
        ignition: None,
    };
    let create_req = CreateVmRequest {
        config: Some(vm_config),
    };

    info!("Sending CreateVm request");
    let create_res = vm_client.create_vm(create_req).await?.into_inner();
    let vm_id = create_res.vm_id;
    info!("VM created with ID: {}", vm_id);

    let mut guard = VmGuard {
        vm_id: vm_id.clone(),
        pid: None,
        cleanup_disabled: false,
    };

    info!("Connecting to StreamVmEvents stream for vm_id: {}", &vm_id);
    let events_req = StreamVmEventsRequest {
        vm_id: vm_id.clone(),
        ..Default::default()
    };
    let mut stream = vm_client.stream_vm_events(events_req).await?.into_inner();

    timeout(
        Duration::from_secs(180),
        wait_for_vm_state(&mut stream, VmState::Created),
    )
    .await
    .expect("Timed out waiting for VM to become created")?;
    info!("VM is in CREATED state");

    info!("Sending StartVm request for vm_id: {}", &vm_id);
    let start_req = StartVmRequest {
        vm_id: vm_id.clone(),
    };
    vm_client.start_vm(start_req).await?;

    timeout(
        Duration::from_secs(30),
        wait_for_vm_state(&mut stream, VmState::Running),
    )
    .await
    .expect("Timed out waiting for VM to become running")?;
    info!("VM is in RUNNING state");

    let get_req = GetVmRequest {
        vm_id: vm_id.clone(),
    };
    let info_res = vm_client.get_vm(get_req).await?.into_inner();
    assert_eq!(
        VmState::try_from(info_res.state).unwrap(),
        VmState::Running,
        "VM state from GetVm should be RUNNING"
    );

    info!("Pinging VMM for vm_id: {}", &vm_id);
    let ping_req = PingVmRequest {
        vm_id: vm_id.clone(),
    };
    let ping_res = vm_client.ping_vm(ping_req).await?.into_inner();
    info!("VMM Ping successful, PID: {}", ping_res.pid);
    guard.pid = Some(Pid::from_raw(ping_res.pid as i32));

    info!("Deleting VM: {}", &vm_id);
    let delete_req = DeleteVmRequest {
        vm_id: vm_id.clone(),
    };
    vm_client.delete_vm(delete_req).await?.into_inner();
    info!("DeleteVm call successful");

    let socket_path = format!("{}/{}", VM_API_SOCKET_DIR, &vm_id);
    assert!(
        !Path::new(&socket_path).exists(),
        "Socket file '{}' should not exist after DeleteVm",
        socket_path
    );
    info!("Verified VM API socket is deleted: {}", socket_path);

    guard.cleanup_disabled = true;
    Ok(())
}

#[tokio::test]
async fn test_hostname_retrieval() -> Result<()> {
    ensure_server().await;
    let (_, mut host_client) = get_public_clients().await?;

    let response = host_client.hostname(HostnameRequest {}).await?;
    let remote_hostname = response.into_inner().hostname;
    let local_hostname = unistd::gethostname()?
        .into_string()
        .expect("Hostname is not valid UTF-8");

    info!(
        "Hostname from API: '{}', Hostname from local call: '{}'",
        remote_hostname, local_hostname
    );
    assert_eq!(
        remote_hostname, local_hostname,
        "The hostname from the API should match the local system's hostname"
    );

    Ok(())
}

#[tokio::test]
async fn test_image_lifecycle() -> Result<()> {
    if skip_if_ch_binary_missing() {
        return Ok(());
    }
    ensure_server().await;
    let mut image_client = get_image_service_client().await?;

    let image_ref = TEST_IMAGE_REF.to_string();
    info!("Pulling image: {}", image_ref);
    let pull_req = PullImageRequest {
        image_ref: image_ref.clone(),
    };
    let pull_res = image_client.pull_image(pull_req).await?.into_inner();
    let image_uuid = pull_res.image_uuid;
    info!("Image pull initiated with UUID: {}", image_uuid);

    let watch_req = WatchImageStatusRequest {
        image_uuid: image_uuid.clone(),
    };
    let mut stream = image_client
        .watch_image_status(watch_req)
        .await?
        .into_inner();

    timeout(Duration::from_secs(120), wait_for_image_ready(&mut stream))
        .await
        .expect("Timed out waiting for image to become ready")?;

    info!("Verifying image {} is in the list...", image_uuid);
    let list_req = ListImagesRequest {};
    let list_res = image_client.list_images(list_req).await?.into_inner();
    let found_image = list_res
        .images
        .iter()
        .find(|i| i.image_uuid == image_uuid)
        .expect("Image UUID should be in the list after pulling");
    assert_eq!(found_image.state, ImageState::Ready as i32);

    let image_path = Path::new(IMAGE_DIR).join(&image_uuid);
    info!("Verifying filesystem path: {}", image_path.display());
    assert!(image_path.exists(), "Image directory should exist");
    assert!(image_path.join("disk.image").exists());
    assert!(image_path.join("metadata.json").exists());

    info!("Deleting image: {}", image_uuid);
    let delete_req = DeleteImageRequest {
        image_uuid: image_uuid.clone(),
    };
    image_client.delete_image(delete_req).await?;

    info!("Verifying image {} is NOT in the list...", image_uuid);
    let list_req_after_delete = ListImagesRequest {};
    let list_res_after_delete = image_client
        .list_images(list_req_after_delete)
        .await?
        .into_inner();
    assert!(!list_res_after_delete
        .images
        .iter()
        .any(|i| i.image_uuid == image_uuid));

    info!(
        "Verifying filesystem path is gone: {}",
        image_path.display()
    );
    assert!(!image_path.exists(), "Image directory should be deleted");

    Ok(())
}

async fn wait_for_image_ready<S>(mut stream: S) -> anyhow::Result<()>
where
    S: tokio_stream::Stream<
            Item = Result<proto_definitions::image_service::ImageStatusResponse, tonic::Status>,
        > + Unpin,
{
    let mut saw_downloading = false;
    while let Some(status_res) = stream.next().await {
        let status = status_res?;
        let state = ImageState::try_from(status.state).unwrap();
        info!("Received image status update: {:?}", state);
        if state == ImageState::Downloading {
            saw_downloading = true;
        }
        if state == ImageState::Ready {
            assert!(
                saw_downloading,
                "Should have seen DOWNLOADING state before READY"
            );
            return Ok(());
        }
        if state == ImageState::PullFailed {
            panic!("Image pull failed unexpectedly: {}", status.message);
        }
    }
    anyhow::bail!("Stream ended before image became ready");
}
