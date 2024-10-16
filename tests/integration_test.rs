use feos::feos_grpc::feos_grpc_client::FeosGrpcClient;
use feos::feos_grpc::{BootVmRequest, CreateVmRequest, PingVmRequest};
use regex::Regex;
use serial_test::serial;
use std::path::Path;
use std::process::Stdio;
use tokio::fs;
use tokio::io;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::oneshot;
use tokio::time::{sleep, Duration};
use tonic::transport::Channel;

const FEOS_BINARY: &str = "./target/debug/feos";
const UKI_FILE: &str = "./target/uki.efi";
const IMAGE_DIRECTORY: &str = "./images/feos_nested";
const ROOTFS_FILE: &str =
    "./images/feos_nested/application.vnd.ironcore.image.rootfs.v1alpha1.rootfs";

const VM_MEMORY_SIZE: u64 = 536870912;
const VM_CPU_COUNT: u32 = 2;
const LOCAL_HOST_DESTINATION: &str = "http://localhost:1337";

async fn copy_file_if_needed() -> Result<(), Box<dyn std::error::Error>> {
    if Path::new(ROOTFS_FILE).exists() {
        return Ok(());
    }

    if !Path::new(UKI_FILE).exists() {
        return Err(Box::new(io::Error::new(
            io::ErrorKind::NotFound,
            "Source file not found",
        )));
    }

    if !Path::new(IMAGE_DIRECTORY).exists() {
        fs::create_dir_all(IMAGE_DIRECTORY).await?;
    }

    fs::copy(UKI_FILE, ROOTFS_FILE).await?;

    Ok(())
}

async fn start_feos_server<R: Send + 'static>(
    stdout_processor: impl Fn(&str) -> Option<R> + Send + Sync + 'static,
) -> Result<(tokio::process::Child, oneshot::Receiver<R>), Box<dyn std::error::Error>> {
    copy_file_if_needed().await?;

    let mut child = Command::new(FEOS_BINARY)
        .arg("--ipam")
        .arg("2a10:afc0:e01f:f4:9::/80")
        .env("RUN_MODE", "test")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start FeOS server binary");

    let stdout = child.stdout.take().expect("Failed to capture stdout");
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();

    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        while let Ok(Some(line)) = lines.next_line().await {
            println!("FeOS stdout: {}", line);

            if let Some(result) = stdout_processor(&line) {
                if tx.send(result).is_err() {
                    eprintln!("Receiver dropped");
                }
                break;
            }
        }
    });

    Ok((child, rx))
}

async fn create_and_boot_vm(
    client: &mut FeosGrpcClient<tonic::transport::Channel>,
    image_uuid: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let create_vm_request = CreateVmRequest {
        cpu: VM_CPU_COUNT,
        memory_bytes: VM_MEMORY_SIZE,
        image_uuid: image_uuid.into(),
        ignition: None,
    };

    let create_response = client.create_vm(create_vm_request).await?;
    let vm_uuid = create_response.into_inner().uuid;

    println!("Created VM with UUID: {}", vm_uuid);
    sleep(Duration::from_millis(4000)).await;

    let boot_vm_request = BootVmRequest {
        uuid: vm_uuid.clone(),
    };

    client.boot_vm(boot_vm_request).await?;

    println!("Booted VM with UUID: {}", vm_uuid);

    Ok(vm_uuid)
}

async fn cleanup(child: &mut tokio::process::Child) -> Result<(), Box<dyn std::error::Error>> {
    // TODO client shutdown does not work as expected. Revisit needed
    //let _shutdown_vm_request = ShutdownVmRequest {
    //    uuid: vm_uuid.clone(),
    //};

    // client.shutdown_vm(shutdown_vm_request).await?;
    // sleep(Duration::from_millis(5000)).await;

    child
        .kill()
        .await
        .expect("Failed to kill FeOS server binary");

    //TODO this is just a dirty hack. Normal VM shutdown should handle this actually
    let status = Command::new("pkill").arg("cloud").status().await?;

    if status.success() {
        println!("Successfully executed `pkill cloud`");
    } else {
        eprintln!("`pkill cloud` failed with status: {}", status);
    }

    Ok(())
}

async fn setup_vm() -> Result<(FeosGrpcClient<Channel>, String), Box<dyn std::error::Error>> {
    sleep(Duration::from_millis(2000)).await;
    let mut client = FeosGrpcClient::connect(LOCAL_HOST_DESTINATION).await?;
    let vm_uuid = create_and_boot_vm(&mut client, "feos_nested").await?;
    Ok((client, vm_uuid))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn test_create_and_boot_vm() -> Result<(), Box<dyn std::error::Error>> {
    let ip_regex = Regex::new(r"Assigning IP\s+([0-9a-fA-F:]+)").unwrap();

    let (mut child, rx) = start_feos_server(move |line| {
        if line.contains("Assigning IP") {
            if let Some(captures) = ip_regex.captures(&line) {
                if let Some(ip_match) = captures.get(1) {
                    let ip_str = ip_match.as_str().to_string();
                    println!("Extracted IPv6: {}", ip_str);
                    return Some(ip_str);
                }
            } else {
                eprintln!("Failed to parse IPv6 from line: {}", line);
            }
        }
        None
    })
    .await?;

    setup_vm().await?;

    let assigned_ip = match tokio::time::timeout(Duration::from_secs(20), rx).await {
        Ok(Ok(ip)) => ip,
        Ok(Err(_)) => return Err("Failed to receive IP from stdout".into()),
        Err(_) => return Err("Timed out waiting for IP assignment".into()),
    };

    println!("Assigned IP: {}", assigned_ip);
    sleep(Duration::from_millis(2000)).await;

    let ping_status = Command::new("ping6")
        .arg("-c")
        .arg("1")
        .arg(&assigned_ip)
        .status()
        .await?;

    if ping_status.success() {
        println!("Ping successful");
    } else {
        return Err("Ping failed".into());
    }

    cleanup(&mut child).await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn test_ping_vsock_vm() -> Result<(), Box<dyn std::error::Error>> {
    let (mut child, rx) = start_feos_server(move |line| {
        if line.contains("Connected to vsock: OK") {
            return Some(true);
        }
        None
    })
    .await?;

    let (mut client, vm_uuid) = setup_vm().await?;

    sleep(Duration::from_millis(11000)).await;

    let ping_vm_request = PingVmRequest {
        uuid: vm_uuid.clone(),
    };
    client.ping_vm(ping_vm_request).await?;

    let connected = match tokio::time::timeout(Duration::from_secs(20), rx).await {
        Ok(Ok(status)) => status,
        Ok(Err(_)) => return Err("Failed to receive status from stdout".into()),
        Err(_) => return Err("Timed out waiting for connection".into()),
    };

    assert!(
        connected,
        "Expected 'Connected to vsock: OK' in FeOS output"
    );

    cleanup(&mut child).await?;

    Ok(())
}
