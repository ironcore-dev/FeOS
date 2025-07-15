use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::tty::IsTty;
use prost::Message;
use proto_definitions::vm_service::{
    stream_vm_console_request as console_input, vm_service_client::VmServiceClient,
    AttachConsoleMessage, AttachDiskRequest, ConsoleData, CpuConfig, CreateVmRequest,
    DeleteVmRequest, DiskConfig, GetVmRequest, ListVmsRequest, MemoryConfig, PauseVmRequest,
    PingVmRequest, RemoveDiskRequest, ResumeVmRequest, ShutdownVmRequest, StartVmRequest,
    StreamVmConsoleRequest, StreamVmEventsRequest, VmConfig, VmState, VmStateChangedEvent,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tonic::transport::Channel;

#[derive(Args, Debug)]
pub struct VmArgs {
    #[arg(
        short,
        long,
        global = true,
        env = "FEOS_ADDRESS",
        default_value = "http://[::1]:1337"
    )]
    pub address: String,

    #[command(subcommand)]
    command: VmCommand,
}

#[derive(Subcommand, Debug)]
pub enum VmCommand {
    Create {
        #[arg(long, required = true)]
        image_ref: String,

        #[arg(long, default_value_t = 1)]
        vcpus: u32,

        #[arg(long, default_value_t = 1024)]
        memory: u64,
    },
    Start {
        #[arg(required = true)]
        vm_id: String,
    },
    Info {
        #[arg(required = true)]
        vm_id: String,
    },
    List,
    Ping {
        #[arg(required = true)]
        vm_id: String,
    },
    Shutdown {
        #[arg(required = true)]
        vm_id: String,
    },
    Pause {
        #[arg(required = true)]
        vm_id: String,
    },
    Resume {
        #[arg(required = true)]
        vm_id: String,
    },
    Delete {
        #[arg(required = true)]
        vm_id: String,
    },
    Events {
        #[arg(required = true)]
        vm_id: String,
    },
    Console {
        #[arg(required = true)]
        vm_id: String,
    },
    AttachDisk {
        #[arg(long, required = true)]
        vm_id: String,
        #[arg(long, required = true)]
        path: String,
    },
    RemoveDisk {
        #[arg(long, required = true)]
        vm_id: String,
        #[arg(long, required = true)]
        device_id: String,
    },
}

pub async fn handle_vm_command(args: VmArgs) -> Result<()> {
    let mut client = VmServiceClient::connect(args.address)
        .await
        .context("Failed to connect to VM service")?;

    match args.command {
        VmCommand::Create {
            image_ref,
            vcpus,
            memory,
        } => create_vm(&mut client, image_ref, vcpus, memory).await?,
        VmCommand::Start { vm_id } => start_vm(&mut client, vm_id).await?,
        VmCommand::Info { vm_id } => get_vm_info(&mut client, vm_id).await?,
        VmCommand::List => list_vms(&mut client).await?,
        VmCommand::Ping { vm_id } => ping_vm(&mut client, vm_id).await?,
        VmCommand::Shutdown { vm_id } => shutdown_vm(&mut client, vm_id).await?,
        VmCommand::Pause { vm_id } => pause_vm(&mut client, vm_id).await?,
        VmCommand::Resume { vm_id } => resume_vm(&mut client, vm_id).await?,
        VmCommand::Delete { vm_id } => delete_vm(&mut client, vm_id).await?,
        VmCommand::Events { vm_id } => watch_events(&mut client, vm_id).await?,
        VmCommand::Console { vm_id } => console_vm(&mut client, vm_id).await?,
        VmCommand::AttachDisk { vm_id, path } => attach_disk(&mut client, vm_id, path).await?,
        VmCommand::RemoveDisk { vm_id, device_id } => {
            remove_disk(&mut client, vm_id, device_id).await?
        }
    }

    Ok(())
}

async fn create_vm(
    client: &mut VmServiceClient<Channel>,
    image_ref: String,
    vcpus: u32,
    memory: u64,
) -> Result<()> {
    println!("Requesting VM creation with image: {image_ref}...");

    let request = CreateVmRequest {
        config: Some(VmConfig {
            cpus: Some(CpuConfig {
                boot_vcpus: vcpus,
                max_vcpus: vcpus,
            }),
            memory: Some(MemoryConfig { size_mib: memory }),
            image_ref,
            ..Default::default()
        }),
    };

    let response = client.create_vm(request).await?.into_inner();
    println!("VM creation initiated. VM ID: {}", response.vm_id);
    println!(
        "Use 'feos-cli vm events {}' to watch its progress.",
        response.vm_id
    );
    println!("Then 'feos-cli vm start {}' to start it.", response.vm_id);

    Ok(())
}

async fn start_vm(client: &mut VmServiceClient<Channel>, vm_id: String) -> Result<()> {
    let request = StartVmRequest {
        vm_id: vm_id.clone(),
    };
    client.start_vm(request).await?;
    println!("Start request sent for VM: {vm_id}");
    Ok(())
}

async fn get_vm_info(client: &mut VmServiceClient<Channel>, vm_id: String) -> Result<()> {
    let request = GetVmRequest {
        vm_id: vm_id.clone(),
    };
    let response = client.get_vm(request).await?.into_inner();

    println!("VM Info for: {vm_id}");
    println!(
        "  State: {:?}",
        VmState::try_from(response.state).unwrap_or(VmState::Unspecified)
    );
    if let Some(config) = response.config {
        println!("  Config:");
        println!("    Image Ref: {}", config.image_ref);
        if let Some(cpus) = config.cpus {
            println!("    vCPUs: {}", cpus.boot_vcpus);
        }
        if let Some(mem) = config.memory {
            println!("    Memory: {} MiB", mem.size_mib);
        }
    }
    Ok(())
}

async fn list_vms(client: &mut VmServiceClient<Channel>) -> Result<()> {
    let request = ListVmsRequest {};
    let response = client.list_vms(request).await?.into_inner();

    if response.vms.is_empty() {
        println!("No VMs found.");
        return Ok(());
    }

    println!("{:<38} {:<12} IMAGE_REF", "VM_ID", "STATE");
    println!("{:-<38} {:-<12} {:-<40}", "", "", "");
    for vm in response.vms {
        let state = VmState::try_from(vm.state).unwrap_or(VmState::Unspecified);
        let image_ref = vm.config.map(|c| c.image_ref).unwrap_or_default();
        println!(
            "{:<38} {:<12} {}",
            vm.vm_id,
            format!("{state:?}"),
            image_ref
        );
    }
    Ok(())
}

async fn ping_vm(client: &mut VmServiceClient<Channel>, vm_id: String) -> Result<()> {
    let request = PingVmRequest {
        vm_id: vm_id.clone(),
    };
    let response = client.ping_vm(request).await?.into_inner();

    println!("VMM Ping response for: {vm_id}");
    println!("  PID: {}", response.pid);
    println!("  Version: {}", response.version);
    println!("  Build Version: {}", response.build_version);
    println!("  Features: {:?}", response.features);
    Ok(())
}

async fn shutdown_vm(client: &mut VmServiceClient<Channel>, vm_id: String) -> Result<()> {
    let request = ShutdownVmRequest {
        vm_id: vm_id.clone(),
    };
    client.shutdown_vm(request).await?;
    println!("Shutdown request sent for VM: {vm_id}");
    Ok(())
}

async fn pause_vm(client: &mut VmServiceClient<Channel>, vm_id: String) -> Result<()> {
    let request = PauseVmRequest {
        vm_id: vm_id.clone(),
    };
    client.pause_vm(request).await?;
    println!("Pause request sent for VM: {vm_id}");
    Ok(())
}

async fn resume_vm(client: &mut VmServiceClient<Channel>, vm_id: String) -> Result<()> {
    let request = ResumeVmRequest {
        vm_id: vm_id.clone(),
    };
    client.resume_vm(request).await?;
    println!("Resume request sent for VM: {vm_id}");
    Ok(())
}

async fn delete_vm(client: &mut VmServiceClient<Channel>, vm_id: String) -> Result<()> {
    let request = DeleteVmRequest {
        vm_id: vm_id.clone(),
    };
    client.delete_vm(request).await?.into_inner();
    println!("Successfully deleted VM: {vm_id}");
    Ok(())
}

async fn watch_events(client: &mut VmServiceClient<Channel>, vm_id: String) -> Result<()> {
    println!("Watching events for VM: {vm_id}. Press Ctrl+C to stop.");
    let request = StreamVmEventsRequest {
        vm_id: vm_id.clone(),
        ..Default::default()
    };
    let mut stream = client.stream_vm_events(request).await?.into_inner();

    while let Some(event) = stream.next().await {
        match event {
            Ok(event) => {
                println!("[{}] Event ID: {}", event.vm_id, event.id);
                if let Some(data) = event.data {
                    if data
                        .type_url
                        .contains("feos.vm.vmm.api.v1.VmStateChangedEvent")
                    {
                        let state_change = VmStateChangedEvent::decode(&*data.value)?;
                        println!(
                            "  New State: {:?} (Reason: {})",
                            VmState::try_from(state_change.new_state)
                                .unwrap_or(VmState::Unspecified),
                            state_change.reason
                        );
                    } else {
                        println!("  Data Type: {}", data.type_url);
                    }
                }
            }
            Err(status) => {
                eprintln!("Error in event stream: {status}");
                break;
            }
        }
    }

    Ok(())
}

async fn console_vm(client: &mut VmServiceClient<Channel>, vm_id: String) -> Result<()> {
    if !std::io::stdin().is_tty() {
        anyhow::bail!("Cannot enter interactive console mode without a TTY.");
    }

    println!("Connecting to console for VM: {vm_id}. Press Ctrl+] to exit.");

    struct RawModeGuard;
    impl Drop for RawModeGuard {
        fn drop(&mut self) {
            if let Err(e) = disable_raw_mode() {
                eprintln!("\r\nFailed to disable raw mode: {e}. Please reset your terminal.\r\n");
            }
        }
    }

    enable_raw_mode().context("Failed to enable terminal raw mode")?;
    let _guard = RawModeGuard;

    let (input_tx, input_rx) = mpsc::channel(10);
    let input_stream = tokio_stream::wrappers::ReceiverStream::new(input_rx);

    let response = client.stream_vm_console(input_stream).await?;
    let mut output_stream = response.into_inner();

    let attach_payload = console_input::Payload::Attach(AttachConsoleMessage {
        vm_id: vm_id.clone(),
    });
    let attach_input = StreamVmConsoleRequest {
        payload: Some(attach_payload),
    };
    input_tx
        .send(attach_input)
        .await
        .context("Failed to send attach message")?;

    let output_task = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(result) = output_stream.next().await {
            match result {
                Ok(msg) => {
                    if let Err(e) = stdout.write_all(&msg.output).await {
                        eprintln!("\r\nError writing to stdout: {e}\r\n");
                        break;
                    }
                    if let Err(e) = stdout.flush().await {
                        eprintln!("\r\nError flushing stdout: {e}\r\n");
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("\r\nError from server stream: {e}\r\n");
                    break;
                }
            }
        }
    });

    let input_task = tokio::spawn(async move {
        let mut stdin = tokio::io::stdin();
        let mut buffer = vec![0; 1];
        loop {
            match stdin.read(&mut buffer).await {
                Ok(0) => break,
                Ok(n) => {
                    if buffer[0] == 29 {
                        break;
                    }
                    let data_payload = console_input::Payload::Data(ConsoleData {
                        input: buffer[..n].to_vec(),
                    });
                    let data_input = StreamVmConsoleRequest {
                        payload: Some(data_payload),
                    };
                    if input_tx.send(data_input).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("\r\nError reading from stdin: {e}\r\n");
                    break;
                }
            }
        }
    });

    tokio::select! {
        _ = output_task => {},
        _ = input_task => {},
    }

    Ok(())
}

async fn attach_disk(
    client: &mut VmServiceClient<Channel>,
    vm_id: String,
    path: String,
) -> Result<()> {
    let request = AttachDiskRequest {
        vm_id: vm_id.clone(),
        disk: Some(DiskConfig {
            backend: Some(proto_definitions::vm_service::disk_config::Backend::Path(
                path,
            )),
            ..Default::default()
        }),
    };
    let response = client.attach_disk(request).await?.into_inner();
    println!(
        "Disk attach request sent for VM: {}. Assigned device_id: {}",
        vm_id, response.device_id
    );
    Ok(())
}

async fn remove_disk(
    client: &mut VmServiceClient<Channel>,
    vm_id: String,
    device_id: String,
) -> Result<()> {
    let request = RemoveDiskRequest {
        vm_id: vm_id.clone(),
        device_id: device_id.clone(),
    };
    client.remove_disk(request).await?;
    println!("Disk remove request sent for device {device_id} on VM {vm_id}");
    Ok(())
}
