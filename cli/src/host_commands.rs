use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use feos_proto::host_service::{
    host_service_client::HostServiceClient, GetCpuInfoRequest, GetNetworkInfoRequest,
    HostnameRequest, MemoryRequest, RebootRequest, ShutdownRequest, StreamFeosLogsRequest,
    StreamKernelLogsRequest, UpgradeFeosBinaryRequest,
};
use tokio_stream::StreamExt;
use tonic::transport::Channel;

#[derive(Args, Debug)]
pub struct HostArgs {
    #[arg(
        short,
        long,
        global = true,
        env = "FEOS_ADDRESS",
        default_value = "http://[::1]:1337"
    )]
    pub address: String,

    #[command(subcommand)]
    command: HostCommand,
}

#[derive(Subcommand, Debug)]
pub enum HostCommand {
    Hostname,
    Memory,
    CpuInfo,
    NetworkInfo,
    Upgrade {
        #[arg(long, required = true, help = "URL to fetch the new FeOS binary from")]
        url: String,
        #[arg(
            long,
            required = true,
            help = "Hex-encoded SHA256 checksum for verification"
        )]
        sha256_sum: String,
    },
    /// Stream kernel logs from /dev/kmsg
    Klogs,
    /// Stream logs from the internal FeOS logger
    Flogs,
    /// Shutdown the host machine
    Shutdown,
    /// Reboot the host machine
    Reboot,
}

pub async fn handle_host_command(args: HostArgs) -> Result<()> {
    let mut client = HostServiceClient::connect(args.address)
        .await
        .context("Failed to connect to host service")?;

    match args.command {
        HostCommand::Hostname => get_hostname(&mut client).await?,
        HostCommand::Memory => get_memory(&mut client).await?,
        HostCommand::CpuInfo => get_cpu_info(&mut client).await?,
        HostCommand::NetworkInfo => get_network_info(&mut client).await?,
        HostCommand::Upgrade { url, sha256_sum } => {
            upgrade_feos(&mut client, url, sha256_sum).await?
        }
        HostCommand::Klogs => stream_klogs(&mut client).await?,
        HostCommand::Flogs => stream_flogs(&mut client).await?,
        HostCommand::Shutdown => shutdown_host(&mut client).await?,
        HostCommand::Reboot => reboot_host(&mut client).await?,
    }

    Ok(())
}

async fn get_hostname(client: &mut HostServiceClient<Channel>) -> Result<()> {
    let request = HostnameRequest {};
    let response = client.hostname(request).await?.into_inner();
    println!("{}", response.hostname);
    Ok(())
}

async fn get_memory(client: &mut HostServiceClient<Channel>) -> Result<()> {
    let request = MemoryRequest {};
    let response = client.get_memory(request).await?.into_inner();

    if let Some(mem_info) = response.mem_info {
        println!("{:<20} {:>15} kB", "Key", "Value");
        println!("{:-<20} {:-<16}", "", "");
        println!("{:<20} {:>15} kB", "MemTotal:", mem_info.memtotal);
        println!("{:<20} {:>15} kB", "MemFree:", mem_info.memfree);
        println!("{:<20} {:>15} kB", "MemAvailable:", mem_info.memavailable);
        println!("{:<20} {:>15} kB", "Buffers:", mem_info.buffers);
        println!("{:<20} {:>15} kB", "Cached:", mem_info.cached);
        println!("{:<20} {:>15} kB", "SwapCached:", mem_info.swapcached);
        println!("{:<20} {:>15} kB", "Active:", mem_info.active);
        println!("{:<20} {:>15} kB", "Inactive:", mem_info.inactive);
        println!("{:<20} {:>15} kB", "Active(anon):", mem_info.activeanon);
        println!("{:<20} {:>15} kB", "Inactive(anon):", mem_info.inactiveanon);
        println!("{:<20} {:>15} kB", "Active(file):", mem_info.activefile);
        println!("{:<20} {:>15} kB", "Inactive(file):", mem_info.inactivefile);
        println!("{:<20} {:>15} kB", "Unevictable:", mem_info.unevictable);
        println!("{:<20} {:>15} kB", "Mlocked:", mem_info.mlocked);
        println!("{:<20} {:>15} kB", "SwapTotal:", mem_info.swaptotal);
        println!("{:<20} {:>15} kB", "SwapFree:", mem_info.swapfree);
        println!("{:<20} {:>15} kB", "Dirty:", mem_info.dirty);
        println!("{:<20} {:>15} kB", "Writeback:", mem_info.writeback);
        println!("{:<20} {:>15} kB", "AnonPages:", mem_info.anonpages);
        println!("{:<20} {:>15} kB", "Mapped:", mem_info.mapped);
        println!("{:<20} {:>15} kB", "Shmem:", mem_info.shmem);
        println!("{:<20} {:>15} kB", "Slab:", mem_info.slab);
        println!("{:<20} {:>15} kB", "SReclaimable:", mem_info.sreclaimable);
        println!("{:<20} {:>15} kB", "SUnreclaim:", mem_info.sunreclaim);
        println!("{:<20} {:>15} kB", "KernelStack:", mem_info.kernelstack);
        println!("{:<20} {:>15} kB", "PageTables:", mem_info.pagetables);
        println!("{:<20} {:>15} kB", "NFS_Unstable:", mem_info.nfsunstable);
        println!("{:<20} {:>15} kB", "Bounce:", mem_info.bounce);
        println!("{:<20} {:>15} kB", "WritebackTmp:", mem_info.writebacktmp);
        println!("{:<20} {:>15} kB", "CommitLimit:", mem_info.commitlimit);
        println!("{:<20} {:>15} kB", "Committed_AS:", mem_info.committedas);
        println!("{:<20} {:>15} kB", "VmallocTotal:", mem_info.vmalloctotal);
        println!("{:<20} {:>15} kB", "VmallocUsed:", mem_info.vmallocused);
        println!("{:<20} {:>15} kB", "VmallocChunk:", mem_info.vmallocchunk);
        println!(
            "{:<20} {:>15} kB",
            "HardwareCorrupted:", mem_info.hardwarecorrupted
        );
        println!("{:<20} {:>15} kB", "AnonHugePages:", mem_info.anonhugepages);
        println!(
            "{:<20} {:>15} kB",
            "ShmemHugePages:", mem_info.shmemhugepages
        );
        println!(
            "{:<20} {:>15} kB",
            "ShmemPmdMapped:", mem_info.shmempmdmapped
        );
        println!("{:<20} {:>15} kB", "CmaTotal:", mem_info.cmatotal);
        println!("{:<20} {:>15} kB", "CmaFree:", mem_info.cmafree);
        println!(
            "{:<20} {:>15} kB",
            "HugePages_Total:", mem_info.hugepagestotal
        );
        println!(
            "{:<20} {:>15} kB",
            "HugePages_Free:", mem_info.hugepagesfree
        );
        println!(
            "{:<20} {:>15} kB",
            "HugePages_Rsvd:", mem_info.hugepagesrsvd
        );
        println!(
            "{:<20} {:>15} kB",
            "HugePages_Surp:", mem_info.hugepagessurp
        );
        println!("{:<20} {:>15} kB", "Hugepagesize:", mem_info.hugepagesize);
        println!("{:<20} {:>15} kB", "DirectMap4k:", mem_info.directmap4k);
        println!("{:<20} {:>15} kB", "DirectMap2m:", mem_info.directmap2m);
        println!("{:<20} {:>15} kB", "DirectMap1G:", mem_info.directmap1g);
    } else {
        println!("No memory information received from the host.");
    }
    Ok(())
}

async fn get_cpu_info(client: &mut HostServiceClient<Channel>) -> Result<()> {
    let request = GetCpuInfoRequest {};
    let response = client.get_cpu_info(request).await?.into_inner();

    if response.cpu_info.is_empty() {
        println!("No CPU information received from the host.");
        return Ok(());
    }

    for (i, cpu) in response.cpu_info.iter().enumerate() {
        println!("--- Processor {} ---", cpu.processor);
        println!("{:<20}: {}", "Vendor ID", cpu.vendor_id);
        println!("{:<20}: {}", "Model Name", cpu.model_name);
        println!("{:<20}: {}", "CPU Family", cpu.cpu_family);
        println!("{:<20}: {}", "Model", cpu.model);
        println!("{:<20}: {}", "Stepping", cpu.stepping);
        println!("{:<20}: {:.3} MHz", "CPU MHz", cpu.cpu_mhz);
        println!("{:<20}: {}", "Cache Size", cpu.cache_size);
        println!("{:<20}: {}", "Physical ID", cpu.physical_id);
        println!("{:<20}: {}", "Core ID", cpu.core_id);
        println!("{:<20}: {}", "CPU Cores", cpu.cpu_cores);
        println!("{:<20}: {}", "Siblings", cpu.siblings);
        println!("{:<20}: {}", "Address Sizes", cpu.address_sizes);
        println!("{:<20}: {:.2}", "BogoMIPS", cpu.bogo_mips);
        if i < response.cpu_info.len() - 1 {
            println!();
        }
    }

    Ok(())
}

async fn get_network_info(client: &mut HostServiceClient<Channel>) -> Result<()> {
    let request = GetNetworkInfoRequest {};
    let response = client.get_network_info(request).await?.into_inner();

    if response.devices.is_empty() {
        println!("No network devices found on the host.");
        return Ok(());
    }

    for dev in response.devices {
        println!("Interface: {}", dev.name);
        println!("  RX");
        println!("    Bytes:    {:>15}", dev.rx_bytes);
        println!("    Packets:  {:>15}", dev.rx_packets);
        println!("    Errors:   {:>15}", dev.rx_errors);
        println!("    Dropped:  {:>15}", dev.rx_dropped);
        println!("  TX");
        println!("    Bytes:    {:>15}", dev.tx_bytes);
        println!("    Packets:  {:>15}", dev.tx_packets);
        println!("    Errors:   {:>15}", dev.tx_errors);
        println!("    Dropped:  {:>15}", dev.tx_dropped);
        println!();
    }

    Ok(())
}

async fn stream_klogs(client: &mut HostServiceClient<Channel>) -> Result<()> {
    println!("Streaming kernel logs... Press Ctrl+C to stop.");
    let request = StreamKernelLogsRequest {};
    let mut stream = client.stream_kernel_logs(request).await?.into_inner();

    while let Some(entry_res) = stream.next().await {
        match entry_res {
            Ok(entry) => {
                println!("{}", entry.message);
            }
            Err(status) => {
                eprintln!("Error in kernel log stream: {status}");
                break;
            }
        }
    }

    Ok(())
}

async fn stream_flogs(client: &mut HostServiceClient<Channel>) -> Result<()> {
    println!("Streaming FeOS logs... Press Ctrl+C to stop.");
    let request = StreamFeosLogsRequest {};
    let mut stream = client.stream_fe_os_logs(request).await?.into_inner();

    while let Some(entry_res) = stream.next().await {
        match entry_res {
            Ok(entry) => {
                let ts = entry
                    .timestamp
                    .map(|t| {
                        chrono::DateTime::from_timestamp(t.seconds, t.nanos as u32)
                            .unwrap_or_default()
                            .to_rfc3339()
                    })
                    .unwrap_or_default();
                println!(
                    "[{ts} {:<5} {}] {}",
                    entry.level, entry.target, entry.message
                );
            }
            Err(status) => {
                eprintln!("Error in FeOS log stream: {status}");
                break;
            }
        }
    }
    Ok(())
}

async fn upgrade_feos(
    client: &mut HostServiceClient<Channel>,
    url: String,
    sha256_sum: String,
) -> Result<()> {
    println!("Requesting FeOS upgrade from URL: {url}");
    println!("Expected SHA256: {sha256_sum}");

    let request = UpgradeFeosBinaryRequest { url, sha256_sum };

    client.upgrade_feos_binary(request).await?;

    println!("Upgrade request accepted by host.");

    Ok(())
}

async fn shutdown_host(client: &mut HostServiceClient<Channel>) -> Result<()> {
    println!("Requesting host shutdown...");
    let request = ShutdownRequest {};
    client.shutdown(request).await?;
    println!("Shutdown command sent successfully. Connection will be lost.");
    Ok(())
}

async fn reboot_host(client: &mut HostServiceClient<Channel>) -> Result<()> {
    println!("Requesting host reboot...");
    let request = RebootRequest {};
    client.reboot(request).await?;
    println!("Reboot command sent successfully. Connection will be lost.");
    Ok(())
}
