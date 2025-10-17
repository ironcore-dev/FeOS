// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use super::{ensure_server, get_public_clients};
use anyhow::{Context, Result};
use feos_proto::host_service::{
    GetCpuInfoRequest, GetNetworkInfoRequest, HostnameRequest, MemoryRequest,
};
use log::info;
use nix::unistd;
use std::fs::File;
use std::io::{BufRead, BufReader};

#[tokio::test]
async fn test_hostname_retrieval() -> Result<()> {
    ensure_server().await;
    let (_, mut host_client, _) = get_public_clients().await?;

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
async fn test_get_memory_info() -> Result<()> {
    ensure_server().await;
    let (_, mut host_client, _) = get_public_clients().await?;

    let file = File::open("/proc/meminfo")?;
    let reader = BufReader::new(file);
    let mut local_memtotal = 0;
    for line in reader.lines() {
        let line = line?;
        if line.starts_with("MemTotal:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                local_memtotal = parts[1].parse::<u64>()?;
            }
            break;
        }
    }

    assert!(
        local_memtotal > 0,
        "Failed to parse MemTotal from local /proc/meminfo"
    );
    info!("Local MemTotal from /proc/meminfo: {} kB", local_memtotal);

    info!("Sending GetMemory request");
    let response = host_client.get_memory(MemoryRequest {}).await?.into_inner();

    let mem_info = response
        .mem_info
        .context("MemoryInfo was not present in the response")?;
    info!(
        "Remote MemTotal from gRPC response: {} kB",
        mem_info.memtotal
    );

    assert_eq!(
        mem_info.memtotal, local_memtotal,
        "MemTotal from API should match the local system's MemTotal"
    );
    assert!(
        mem_info.memfree <= mem_info.memtotal,
        "MemFree should not be greater than MemTotal"
    );

    Ok(())
}

#[tokio::test]
async fn test_get_cpu_info() -> Result<()> {
    ensure_server().await;
    let (_, mut host_client, _) = get_public_clients().await?;

    let file = File::open("/proc/cpuinfo")?;
    let reader = BufReader::new(file);
    let mut local_processor_count = 0;
    let mut local_vendor_id = String::new();
    for line in reader.lines() {
        let line = line?;
        if line.starts_with("processor") {
            local_processor_count += 1;
        }
        if line.starts_with("vendor_id") && local_vendor_id.is_empty() {
            let parts: Vec<&str> = line.splitn(2, ':').collect();
            if parts.len() == 2 {
                local_vendor_id = parts[1].trim().to_string();
            }
        }
    }

    assert!(
        local_processor_count > 0,
        "Failed to parse processor count from /proc/cpuinfo"
    );
    assert!(
        !local_vendor_id.is_empty(),
        "Failed to parse vendor_id from /proc/cpuinfo"
    );
    info!(
        "Local data from /proc/cpuinfo: {} processors, vendor_id: {}",
        local_processor_count, local_vendor_id
    );

    info!("Sending GetCPUInfo request");
    let response = host_client
        .get_cpu_info(GetCpuInfoRequest {})
        .await?
        .into_inner();

    let remote_cpu_info = response.cpu_info;
    info!(
        "Remote data from gRPC: {} processors",
        remote_cpu_info.len()
    );

    assert_eq!(
        remote_cpu_info.len(),
        local_processor_count,
        "Processor count from API should match local count"
    );

    let first_cpu = remote_cpu_info
        .first()
        .context("CPU info list should not be empty")?;
    info!("Remote vendor_id: {}", first_cpu.vendor_id);

    assert_eq!(
        first_cpu.vendor_id, local_vendor_id,
        "Vendor ID of first CPU should match"
    );
    assert!(
        first_cpu.cpu_mhz > 0.0,
        "CPU MHz should be a positive value"
    );

    Ok(())
}

#[tokio::test]
async fn test_get_network_info() -> Result<()> {
    ensure_server().await;
    let (_, mut host_client, _) = get_public_clients().await?;

    info!("Sending GetNetworkInfo request");
    let response = host_client
        .get_network_info(GetNetworkInfoRequest {})
        .await?
        .into_inner();

    assert!(
        !response.devices.is_empty(),
        "The list of network devices should not be empty"
    );
    info!("Received {} network devices", response.devices.len());

    let lo = response
        .devices
        .iter()
        .find(|d| d.name == "lo")
        .context("Could not find the loopback interface 'lo'")?;

    info!("Found loopback interface 'lo'");
    assert_eq!(lo.name, "lo");
    assert!(
        lo.rx_packets > 0 || lo.tx_packets > 0,
        "Loopback interface should have some packets transferred"
    );

    Ok(())
}
