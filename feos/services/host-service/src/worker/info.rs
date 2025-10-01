// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use crate::error::HostError;
use feos_proto::host_service::{
    CpuInfo, GetCpuInfoResponse, GetNetworkInfoResponse, GetVersionInfoResponse, HostnameResponse,
    MemInfo, MemoryResponse, NetDev,
};
use log::{error, info, warn};
use nix::unistd;
use std::collections::HashMap;
use std::path::Path;
use tokio::fs::{self, File};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::oneshot;

pub async fn handle_hostname(responder: oneshot::Sender<Result<HostnameResponse, HostError>>) {
    info!("HostWorker: Processing Hostname request.");
    let result = (|| {
        let host = unistd::gethostname()?;
        let hostname = host
            .into_string()
            .unwrap_or_else(|_| "Invalid UTF-8".into());
        Ok(HostnameResponse { hostname })
    })();

    if responder.send(result).is_err() {
        error!("HostWorker: Failed to send response for Hostname. API handler may have timed out.");
    }
}

async fn read_and_parse_meminfo() -> Result<MemInfo, HostError> {
    let path = "/proc/meminfo";
    let file = File::open(path)
        .await
        .map_err(|e| HostError::SystemInfoRead {
            source: e,
            path: path.to_string(),
        })?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    let mut values = HashMap::new();

    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|e| HostError::SystemInfoRead {
            source: e,
            path: path.to_string(),
        })?
    {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let key = parts[0].trim_end_matches(':');
            if let Ok(value) = parts[1].parse::<u64>() {
                values.insert(key.to_lowercase(), value);
            }
        }
    }

    let get = |key: &str| -> u64 {
        *values.get(key).unwrap_or_else(|| {
            warn!("Memory key {key} not found in /proc/meminfo");
            &0
        })
    };

    Ok(MemInfo {
        memtotal: get("memtotal"),
        memfree: get("memfree"),
        memavailable: get("memavailable"),
        buffers: get("buffers"),
        cached: get("cached"),
        swapcached: get("swapcached"),
        active: get("active"),
        inactive: get("inactive"),
        activeanon: get("active(anon)"),
        inactiveanon: get("inactive(anon)"),
        activefile: get("active(file)"),
        inactivefile: get("inactive(file)"),
        unevictable: get("unevictable"),
        mlocked: get("mlocked"),
        swaptotal: get("swaptotal"),
        swapfree: get("swapfree"),
        dirty: get("dirty"),
        writeback: get("writeback"),
        anonpages: get("anonpages"),
        mapped: get("mapped"),
        shmem: get("shmem"),
        slab: get("slab"),
        sreclaimable: get("sreclaimable"),
        sunreclaim: get("sunreclaim"),
        kernelstack: get("kernelstack"),
        pagetables: get("pagetables"),
        nfsunstable: get("nfs_unstable"),
        bounce: get("bounce"),
        writebacktmp: get("writebacktmp"),
        commitlimit: get("commitlimit"),
        committedas: get("committed_as"),
        vmalloctotal: get("vmalloctotal"),
        vmallocused: get("vmallocused"),
        vmallocchunk: get("vmallocchunk"),
        hardwarecorrupted: get("hardwarecorrupted"),
        anonhugepages: get("anonhugepages"),
        shmemhugepages: get("shmemhugepages"),
        shmempmdmapped: get("shmempmdmapped"),
        cmatotal: get("cmatotal"),
        cmafree: get("cmafree"),
        hugepagestotal: get("hugepages_total"),
        hugepagesfree: get("hugepages_free"),
        hugepagesrsvd: get("hugepages_rsvd"),
        hugepagessurp: get("hugepages_surp"),
        hugepagesize: get("hugepagesize"),
        directmap4k: get("directmap4k"),
        directmap2m: get("directmap2m"),
        directmap1g: get("directmap1g"),
    })
}

pub async fn handle_get_memory(responder: oneshot::Sender<Result<MemoryResponse, HostError>>) {
    info!("HostWorker: Processing GetMemory request.");
    let result = read_and_parse_meminfo()
        .await
        .map(|mem_info| MemoryResponse {
            mem_info: Some(mem_info),
        });

    if responder.send(result).is_err() {
        error!(
            "HostWorker: Failed to send response for GetMemory. API handler may have timed out."
        );
    }
}

fn parse_map_to_cpu_info(map: &HashMap<String, String>) -> CpuInfo {
    let get_string = |key: &str| -> String { map.get(key).cloned().unwrap_or_default() };
    let get_u32 = |key: &str| -> u32 { map.get(key).and_then(|v| v.parse().ok()).unwrap_or(0) };
    let get_f64 = |key: &str| -> f64 { map.get(key).and_then(|v| v.parse().ok()).unwrap_or(0.0) };
    let get_vec_string = |key: &str| -> Vec<String> {
        map.get(key)
            .map(|v| v.split_whitespace().map(String::from).collect())
            .unwrap_or_default()
    };

    CpuInfo {
        processor: get_u32("processor"),
        vendor_id: get_string("vendor_id"),
        cpu_family: get_string("cpu family"),
        model: get_string("model"),
        model_name: get_string("model name"),
        stepping: get_string("stepping"),
        microcode: get_string("microcode"),
        cpu_mhz: get_f64("cpu mhz"),
        cache_size: get_string("cache size"),
        physical_id: get_string("physical id"),
        siblings: get_u32("siblings"),
        core_id: get_string("core id"),
        cpu_cores: get_u32("cpu cores"),
        apic_id: get_string("apicid"),
        initial_apic_id: get_string("initial apicid"),
        fpu: get_string("fpu"),
        fpu_exception: get_string("fpu_exception"),
        cpu_id_level: get_u32("cpuid level"),
        wp: get_string("wp"),
        flags: get_vec_string("flags"),
        bugs: get_vec_string("bugs"),
        bogo_mips: get_f64("bogomips"),
        cl_flush_size: get_u32("clflush size"),
        cache_alignment: get_u32("cache_alignment"),
        address_sizes: get_string("address sizes"),
        power_management: get_string("power management"),
    }
}

async fn read_and_parse_cpuinfo() -> Result<Vec<CpuInfo>, HostError> {
    let path = "/proc/cpuinfo";
    let file = File::open(path)
        .await
        .map_err(|e| HostError::SystemInfoRead {
            source: e,
            path: path.to_string(),
        })?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    let mut cpus = Vec::new();
    let mut current_cpu_map = HashMap::new();

    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|e| HostError::SystemInfoRead {
            source: e,
            path: path.to_string(),
        })?
    {
        if line.trim().is_empty() {
            if !current_cpu_map.is_empty() {
                let cpu_info = parse_map_to_cpu_info(&current_cpu_map);
                cpus.push(cpu_info);
                current_cpu_map.clear();
            }
            continue;
        }

        let parts: Vec<&str> = line.splitn(2, ':').collect();
        if parts.len() == 2 {
            let key = parts[0].trim().to_lowercase();
            let value = parts[1].trim().to_string();
            current_cpu_map.insert(key, value);
        }
    }

    if !current_cpu_map.is_empty() {
        let cpu_info = parse_map_to_cpu_info(&current_cpu_map);
        cpus.push(cpu_info);
    }

    Ok(cpus)
}

pub async fn handle_get_cpu_info(
    responder: oneshot::Sender<Result<GetCpuInfoResponse, HostError>>,
) {
    info!("HostWorker: Processing GetCPUInfo request.");
    let result = read_and_parse_cpuinfo()
        .await
        .map(|cpu_info| GetCpuInfoResponse { cpu_info });

    if responder.send(result).is_err() {
        error!(
            "HostWorker: Failed to send response for GetCPUInfo. API handler may have timed out."
        );
    }
}

async fn read_net_stat(base_path: &Path, stat_name: &str) -> u64 {
    let stat_path = base_path.join(stat_name);
    fs::read_to_string(stat_path)
        .await
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0)
}

async fn read_all_net_stats() -> Result<Vec<NetDev>, HostError> {
    let path = "/sys/class/net";
    let mut devices = Vec::new();
    let mut entries = fs::read_dir(path)
        .await
        .map_err(|e| HostError::SystemInfoRead {
            source: e,
            path: path.to_string(),
        })?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| HostError::SystemInfoRead {
            source: e,
            path: path.to_string(),
        })?
    {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let name = entry
            .file_name()
            .into_string()
            .unwrap_or_else(|_| "invalid_utf8".to_string());
        let stats_path = path.join("statistics");

        if !stats_path.is_dir() {
            continue;
        }

        let device = NetDev {
            name,
            rx_bytes: read_net_stat(&stats_path, "rx_bytes").await,
            rx_packets: read_net_stat(&stats_path, "rx_packets").await,
            rx_errors: read_net_stat(&stats_path, "rx_errors").await,
            rx_dropped: read_net_stat(&stats_path, "rx_dropped").await,
            rx_fifo: read_net_stat(&stats_path, "rx_fifo_errors").await,
            rx_frame: read_net_stat(&stats_path, "rx_frame_errors").await,
            rx_compressed: read_net_stat(&stats_path, "rx_compressed").await,
            rx_multicast: read_net_stat(&stats_path, "multicast").await,
            tx_bytes: read_net_stat(&stats_path, "tx_bytes").await,
            tx_packets: read_net_stat(&stats_path, "tx_packets").await,
            tx_errors: read_net_stat(&stats_path, "tx_errors").await,
            tx_dropped: read_net_stat(&stats_path, "tx_dropped").await,
            tx_fifo: read_net_stat(&stats_path, "tx_fifo_errors").await,
            tx_collisions: read_net_stat(&stats_path, "collisions").await,
            tx_carrier: read_net_stat(&stats_path, "tx_carrier_errors").await,
            tx_compressed: read_net_stat(&stats_path, "tx_compressed").await,
        };
        devices.push(device);
    }

    Ok(devices)
}

pub async fn handle_get_network_info(
    responder: oneshot::Sender<Result<GetNetworkInfoResponse, HostError>>,
) {
    info!("HostWorker: Processing GetNetworkInfo request.");
    let result = read_all_net_stats()
        .await
        .map(|devices| GetNetworkInfoResponse { devices });

    if responder.send(result).is_err() {
        error!(
            "HostWorker: Failed to send response for GetNetworkInfo. API handler may have timed out."
        );
    }
}

pub async fn handle_get_version_info(
    responder: oneshot::Sender<Result<GetVersionInfoResponse, HostError>>,
) {
    info!("HostWorker: Processing GetVersionInfo request.");
    let path = "/proc/version";
    let result = fs::read_to_string(path)
        .await
        .map(|kernel_version| {
            let feos_version = feos_utils::version::full_version_string();
            GetVersionInfoResponse {
                kernel_version: kernel_version.trim().to_string(),
                feos_version,
            }
        })
        .map_err(|e| HostError::SystemInfoRead {
            source: e,
            path: path.to_string(),
        });

    if responder.send(result).is_err() {
        error!(
            "HostWorker: Failed to send response for GetVersionInfo. API handler may have timed out."
        );
    }
}
