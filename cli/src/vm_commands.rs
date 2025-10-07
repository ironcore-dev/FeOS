// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::tty::IsTty;
use feos_proto::vm_service::{
    net_config, stream_vm_console_request as console_input, vm_service_client::VmServiceClient,
    AttachConsoleMessage, AttachDiskRequest, AttachNicRequest, ConsoleData, CpuConfig,
    CreateVmRequest, DeleteVmRequest, DiskConfig, GetVmRequest, ListVmsRequest, MemoryConfig,
    NetConfig, PauseVmRequest, PingVmRequest, RemoveDiskRequest, RemoveNicRequest, ResumeVmRequest,
    ShutdownVmRequest, StartVmRequest, StreamVmConsoleRequest, StreamVmEventsRequest, TapConfig,
    VfioPciConfig, VmConfig, VmState, VmStateChangedEvent,
};
use prost::Message;
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
    /// Create a new virtual machine with specified configuration
    Create {
        #[arg(
            long,
            required = true,
            help = "Container image reference to use for the VM"
        )]
        image_ref: String,

        #[arg(long, default_value_t = 1, help = "Number of virtual CPUs to allocate")]
        vcpus: u32,

        #[arg(long, default_value_t = 1024, help = "Memory size in MiB")]
        memory: u64,

        #[arg(long, help = "Optional custom VM identifier")]
        vm_id: Option<String>,

        #[arg(
            long,
            help = "PCI device BDF to passthrough for networking (e.g., 0000:03:00.0)"
        )]
        pci_device: Vec<String>,

        #[arg(long, help = "Enable hugepages for memory allocation")]
        hugepages: bool,

        #[arg(long, help = "Path to ignition file or the content itself")]
        ignition: Option<String>,
    },
    /// Start an existing virtual machine
    Start {
        #[arg(required = true, help = "VM identifier")]
        vm_id: String,
    },
    /// Get detailed information about a virtual machine
    Info {
        #[arg(required = true, help = "VM identifier")]
        vm_id: String,
    },
    /// List all virtual machines
    List,
    /// Ping a virtual machine's VMM to check status
    Ping {
        #[arg(required = true, help = "VM identifier")]
        vm_id: String,
    },
    /// Gracefully shutdown a virtual machine
    Shutdown {
        #[arg(required = true, help = "VM identifier")]
        vm_id: String,
    },
    /// Pause a running virtual machine
    Pause {
        #[arg(required = true, help = "VM identifier")]
        vm_id: String,
    },
    /// Resume a paused virtual machine
    Resume {
        #[arg(required = true, help = "VM identifier")]
        vm_id: String,
    },
    /// Delete a virtual machine
    Delete {
        #[arg(required = true, help = "VM identifier")]
        vm_id: String,
    },
    /// Create and start a virtual machine in one operation
    CreateAndStart {
        #[arg(
            long,
            required = true,
            help = "Container image reference to use for the VM"
        )]
        image_ref: String,

        #[arg(long, default_value_t = 1, help = "Number of virtual CPUs to allocate")]
        vcpus: u32,

        #[arg(long, default_value_t = 1024, help = "Memory size in MiB")]
        memory: u64,

        #[arg(long, help = "Optional custom VM identifier")]
        vm_id: Option<String>,

        #[arg(
            long,
            help = "PCI device BDF to passthrough for networking (e.g., 0000:03:00.0)"
        )]
        pci_device: Vec<String>,

        #[arg(long, help = "Enable hugepages for memory allocation")]
        hugepages: bool,

        #[arg(long, help = "Path to ignition file or the content itself")]
        ignition: Option<String>,
    },
    /// Watch virtual machine state change events
    Events {
        #[arg(
            long,
            help = "VM identifier (optional, if not provided watches all VMs)"
        )]
        vm_id: Option<String>,
    },
    /// Connect to a virtual machine's console
    Console {
        #[arg(required = true, help = "VM identifier")]
        vm_id: String,
    },
    /// Attach a disk to a running virtual machine
    AttachDisk {
        #[arg(long, required = true, help = "VM identifier")]
        vm_id: String,
        #[arg(long, required = true, help = "Path to the disk image file")]
        path: String,
    },
    /// Remove a disk from a virtual machine
    RemoveDisk {
        #[arg(long, required = true, help = "VM identifier")]
        vm_id: String,
        #[arg(
            long,
            required = true,
            help = "Device identifier of the disk to remove"
        )]
        device_id: String,
    },
    /// Attach a network interface to a VM
    AttachNic {
        #[arg(long, required = true, help = "VM identifier")]
        vm_id: String,
        #[arg(
            long,
            help = "Name of the TAP device to attach",
            conflicts_with = "pci_device"
        )]
        tap_name: Option<String>,
        #[arg(
            long,
            help = "PCI device BDF to passthrough for networking (e.g., 0000:03:00.0)",
            conflicts_with = "tap_name"
        )]
        pci_device: Option<String>,
        #[arg(long, help = "MAC address for the new interface")]
        mac_address: Option<String>,
        #[arg(long, help = "Custom device identifier for the new interface")]
        device_id: Option<String>,
    },
    /// Remove a network interface from a VM
    RemoveNic {
        #[arg(long, required = true, help = "VM identifier")]
        vm_id: String,
        #[arg(long, required = true, help = "Device identifier of the NIC to remove")]
        device_id: String,
    },
}

#[derive(Debug, Clone)]
struct CreateVmOptions {
    image_ref: String,
    vcpus: u32,
    memory: u64,
    vm_id: Option<String>,
    pci_devices: Vec<String>,
    hugepages: bool,
    ignition: Option<String>,
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
            vm_id,
            pci_device,
            hugepages,
            ignition,
        } => {
            let opts = CreateVmOptions {
                image_ref,
                vcpus,
                memory,
                vm_id,
                pci_devices: pci_device,
                hugepages,
                ignition,
            };
            create_vm(&mut client, opts).await?
        }
        VmCommand::Start { vm_id } => start_vm(&mut client, vm_id).await?,
        VmCommand::Info { vm_id } => get_vm_info(&mut client, vm_id).await?,
        VmCommand::List => list_vms(&mut client).await?,
        VmCommand::Ping { vm_id } => ping_vm(&mut client, vm_id).await?,
        VmCommand::Shutdown { vm_id } => shutdown_vm(&mut client, vm_id).await?,
        VmCommand::Pause { vm_id } => pause_vm(&mut client, vm_id).await?,
        VmCommand::Resume { vm_id } => resume_vm(&mut client, vm_id).await?,
        VmCommand::Delete { vm_id } => delete_vm(&mut client, vm_id).await?,
        VmCommand::CreateAndStart {
            image_ref,
            vcpus,
            memory,
            vm_id,
            pci_device,
            hugepages,
            ignition,
        } => {
            let opts = CreateVmOptions {
                image_ref,
                vcpus,
                memory,
                vm_id,
                pci_devices: pci_device,
                hugepages,
                ignition,
            };
            create_and_start_vm(&mut client, opts).await?
        }
        VmCommand::Events { vm_id } => watch_events(&mut client, vm_id).await?,
        VmCommand::Console { vm_id } => console_vm(&mut client, vm_id).await?,
        VmCommand::AttachDisk { vm_id, path } => attach_disk(&mut client, vm_id, path).await?,
        VmCommand::RemoveDisk { vm_id, device_id } => {
            remove_disk(&mut client, vm_id, device_id).await?
        }
        VmCommand::AttachNic {
            vm_id,
            tap_name,
            pci_device,
            mac_address,
            device_id,
        } => {
            attach_nic(
                &mut client,
                vm_id,
                tap_name,
                pci_device,
                mac_address,
                device_id,
            )
            .await?
        }
        VmCommand::RemoveNic { vm_id, device_id } => {
            remove_nic(&mut client, vm_id, device_id).await?
        }
    }

    Ok(())
}

async fn create_and_start_vm(
    client: &mut VmServiceClient<Channel>,
    opts: CreateVmOptions,
) -> Result<()> {
    let CreateVmOptions {
        image_ref,
        vcpus,
        memory,
        vm_id,
        pci_devices,
        hugepages,
        ignition,
    } = opts;

    println!("� Starting create and start operation for VM with image: {image_ref}");

    // Step 1: Create the VM
    println!("� Step 1: Creating VM...");

    let ignition_data = if let Some(ignition_str) = ignition {
        if tokio::fs::metadata(&ignition_str).await.is_ok() {
            Some(tokio::fs::read_to_string(ignition_str).await?)
        } else {
            Some(ignition_str)
        }
    } else {
        None
    };

    let net_configs = pci_devices
        .iter()
        .map(|bdf| {
            println!("   Adding PCI device: {bdf}");
            NetConfig {
                backend: Some(net_config::Backend::VfioPci(VfioPciConfig {
                    bdf: bdf.clone(),
                })),
                ..Default::default()
            }
        })
        .collect();

    let request = CreateVmRequest {
        config: Some(VmConfig {
            cpus: Some(CpuConfig {
                boot_vcpus: vcpus,
                max_vcpus: vcpus,
            }),
            memory: Some(MemoryConfig {
                size_mib: memory,
                hugepages,
            }),
            image_ref: image_ref.clone(),
            net: net_configs,
            ignition: ignition_data,
            ..Default::default()
        }),
        vm_id: vm_id.clone(),
    };

    let response = client.create_vm(request).await?.into_inner();
    let vm_id = response.vm_id;
    println!("✅ VM created successfully with ID: {vm_id}");

    // Step 2: Wait for VM to be in 'Created' state
    println!("⏳ Step 2: Waiting for VM to reach 'Created' state...");
    wait_for_vm_state(client, &vm_id, VmState::Created).await?;
    println!("✅ VM is now in 'Created' state");

    // Step 3: Start the VM
    println!("🔄 Step 3: Starting VM...");
    let start_request = StartVmRequest {
        vm_id: vm_id.clone(),
    };
    client.start_vm(start_request).await?;
    println!("✅ Start request sent successfully");

    // Step 4: Wait for VM to be in 'Running' state
    println!("⏳ Step 4: Waiting for VM to reach 'Running' state...");
    wait_for_vm_state(client, &vm_id, VmState::Running).await?;
    println!("🎉 VM '{vm_id}' is now running successfully!");

    println!("Use 'feos-cli vm console {vm_id}' to connect to the VM console.");

    Ok(())
}

async fn wait_for_vm_state(
    client: &mut VmServiceClient<Channel>,
    vm_id: &str,
    target_state: VmState,
) -> Result<()> {
    let request = StreamVmEventsRequest {
        vm_id: Some(vm_id.to_string()),
        ..Default::default()
    };

    let mut stream = client.stream_vm_events(request).await?.into_inner();

    // First, check current state
    let get_request = GetVmRequest {
        vm_id: vm_id.to_string(),
    };
    let current_vm = client.get_vm(get_request).await?.into_inner();
    let current_state = VmState::try_from(current_vm.state).unwrap_or(VmState::Unspecified);

    if current_state == target_state {
        return Ok(());
    }

    println!("   Current state: {current_state:?}, waiting for: {target_state:?}");

    // Listen for state changes
    while let Some(event) = stream.next().await {
        match event {
            Ok(event) => {
                if let Some(data) = event.data {
                    if data
                        .type_url
                        .contains("feos.vm.vmm.api.v1.VmStateChangedEvent")
                    {
                        let state_change = VmStateChangedEvent::decode(&*data.value)?;
                        let new_state = VmState::try_from(state_change.new_state)
                            .unwrap_or(VmState::Unspecified);

                        println!(
                            "   State transition: {:?} ({})",
                            new_state, state_change.reason
                        );

                        if new_state == target_state {
                            return Ok(());
                        }

                        // Check for error states
                        if new_state == VmState::Crashed {
                            anyhow::bail!("VM entered crashed state: {}", state_change.reason);
                        }
                    }
                }
            }
            Err(status) => {
                anyhow::bail!("Error in event stream: {status}");
            }
        }
    }

    anyhow::bail!("Event stream ended before reaching target state: {target_state:?}")
}

async fn create_vm(client: &mut VmServiceClient<Channel>, opts: CreateVmOptions) -> Result<()> {
    let CreateVmOptions {
        image_ref,
        vcpus,
        memory,
        vm_id,
        pci_devices,
        hugepages,
        ignition,
    } = opts;

    println!("Requesting VM creation with image: {image_ref}...");

    let ignition_data = if let Some(ignition_str) = ignition {
        if tokio::fs::metadata(&ignition_str).await.is_ok() {
            Some(tokio::fs::read_to_string(ignition_str).await?)
        } else {
            Some(ignition_str)
        }
    } else {
        None
    };

    let net_configs = pci_devices
        .into_iter()
        .map(|bdf| {
            println!("Adding PCI device: {bdf}");
            NetConfig {
                backend: Some(net_config::Backend::VfioPci(VfioPciConfig { bdf })),
                ..Default::default()
            }
        })
        .collect();

    let request = CreateVmRequest {
        config: Some(VmConfig {
            cpus: Some(CpuConfig {
                boot_vcpus: vcpus,
                max_vcpus: vcpus,
            }),
            memory: Some(MemoryConfig {
                size_mib: memory,
                hugepages,
            }),
            image_ref,
            net: net_configs,
            ignition: ignition_data,
            ..Default::default()
        }),
        vm_id,
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
        if !config.net.is_empty() {
            println!("    Network Devices:");
            for (i, net_conf) in config.net.iter().enumerate() {
                if let Some(backend) = &net_conf.backend {
                    match backend {
                        net_config::Backend::VfioPci(pci) => {
                            println!("      Device {}: PCI Passthrough - {}", i, pci.bdf);
                        }
                        net_config::Backend::Tap(tap) => {
                            println!("      Device {}: TAP - {}", i, tap.tap_name);
                        }
                    }
                }
            }
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

async fn watch_events(client: &mut VmServiceClient<Channel>, vm_id: Option<String>) -> Result<()> {
    if let Some(id) = &vm_id {
        println!("Watching events for VM: {id}. Press Ctrl+C to stop.");
    } else {
        println!("Watching events for all VMs. Press Ctrl+C to stop.");
    }

    let request = StreamVmEventsRequest {
        vm_id,
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
            backend: Some(feos_proto::vm_service::disk_config::Backend::Path(path)),
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

async fn attach_nic(
    client: &mut VmServiceClient<Channel>,
    vm_id: String,
    tap_name: Option<String>,
    pci_device: Option<String>,
    mac_address: Option<String>,
    device_id: Option<String>,
) -> Result<()> {
    let backend = if let Some(tap) = tap_name {
        Some(net_config::Backend::Tap(TapConfig { tap_name: tap }))
    } else if let Some(bdf) = pci_device {
        Some(net_config::Backend::VfioPci(VfioPciConfig { bdf }))
    } else {
        anyhow::bail!("Either --tap-name or --pci-device must be specified.");
    };

    let nic = NetConfig {
        device_id: device_id.unwrap_or_default(),
        mac_address: mac_address.unwrap_or_default(),
        backend,
    };

    let request = AttachNicRequest {
        vm_id: vm_id.clone(),
        nic: Some(nic),
    };

    let response = client.attach_nic(request).await?.into_inner();
    println!(
        "NIC attach request sent for VM: {}. Assigned device_id: {}",
        vm_id, response.device_id
    );

    Ok(())
}

async fn remove_nic(
    client: &mut VmServiceClient<Channel>,
    vm_id: String,
    device_id: String,
) -> Result<()> {
    let request = RemoveNicRequest {
        vm_id: vm_id.clone(),
        device_id: device_id.clone(),
    };
    client.remove_nic(request).await?;
    println!("NIC remove request sent for device {device_id} on VM {vm_id}");
    Ok(())
}
