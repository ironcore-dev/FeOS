// SPDX-FileCopyrightText: 2025 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use crate::error::HostError;
use feos_proto::host_service::{CpuTime, GetKernelStatsResponse, KernelStats};
use log::{error, info, warn};
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::oneshot;

/// Parse a single CPU line from /proc/stat
/// Format: cpu<N>  user nice system idle iowait irq softirq steal guest guest_nice
fn parse_cpu_line(line: &str) -> Option<CpuTime> {
    let parts: Vec<&str> = line.split_whitespace().collect();

    if parts.is_empty() || !parts[0].starts_with("cpu") {
        return None;
    }

    let name = parts[0].to_string();

    // Parse numeric values, defaulting to 0 if missing
    // Some older kernels may not have all fields
    let get_value = |index: usize| -> u64 {
        parts
            .get(index)
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0)
    };

    Some(CpuTime {
        name,
        user: get_value(1),
        nice: get_value(2),
        system: get_value(3),
        idle: get_value(4),
        iowait: get_value(5),
        irq: get_value(6),
        softirq: get_value(7),
        steal: get_value(8),
        guest: get_value(9),
        guest_nice: get_value(10),
    })
}

async fn read_and_parse_proc_stat() -> Result<KernelStats, HostError> {
    let path = "/proc/stat";
    let file = File::open(path)
        .await
        .map_err(|e| HostError::SystemInfoRead {
            source: e,
            path: path.to_string(),
        })?;

    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    let mut total: Option<CpuTime> = None;
    let mut per_cpu = Vec::new();
    let mut context_switches = 0u64;
    let mut boot_time = 0u64;
    let mut processes_created = 0u64;
    let mut processes_running = 0u32;
    let mut processes_blocked = 0u32;

    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|e| HostError::SystemInfoRead {
            source: e,
            path: path.to_string(),
        })?
    {
        let prefix = line.split_whitespace().next().unwrap_or("");
        match prefix {
            "cpu" if line.starts_with("cpu ") => {
                // Total CPU stats (note the space after "cpu")
                total = parse_cpu_line(&line);
            }
            p if p.starts_with("cpu") => {
                // Per-CPU stats (cpu0, cpu1, etc.)
                if let Some(cpu) = parse_cpu_line(&line) {
                    per_cpu.push(cpu);
                }
            }
            "ctxt" => {
                // Context switches
                if let Some(value) = line.split_whitespace().nth(1) {
                    context_switches = value.parse().unwrap_or(0);
                }
            }
            "btime" => {
                // Boot time (seconds since epoch)
                if let Some(value) = line.split_whitespace().nth(1) {
                    boot_time = value.parse().unwrap_or(0);
                }
            }
            "processes" => {
                // Number of processes created since boot
                if let Some(value) = line.split_whitespace().nth(1) {
                    processes_created = value.parse().unwrap_or(0);
                }
            }
            "procs_running" => {
                // Number of processes currently running
                if let Some(value) = line.split_whitespace().nth(1) {
                    processes_running = value.parse().unwrap_or(0);
                }
            }
            "procs_blocked" => {
                // Number of processes blocked waiting for I/O
                if let Some(value) = line.split_whitespace().nth(1) {
                    processes_blocked = value.parse().unwrap_or(0);
                }
            }
            _ => {}
        }
    }

    if total.is_none() {
        warn!("Failed to parse total CPU stats from /proc/stat");
    }

    if per_cpu.is_empty() {
        warn!("No per-CPU stats found in /proc/stat");
    }

    Ok(KernelStats {
        total,
        per_cpu,
        context_switches,
        boot_time,
        processes_created,
        processes_running,
        processes_blocked,
    })
}

pub async fn handle_get_kernel_stats(
    responder: oneshot::Sender<Result<GetKernelStatsResponse, HostError>>,
) {
    info!("HostWorker: Processing GetKernelStats request.");
    let result = read_and_parse_proc_stat()
        .await
        .map(|stats| GetKernelStatsResponse { stats: Some(stats) });

    if responder.send(result).is_err() {
        error!(
            "HostWorker: Failed to send response for GetKernelStats. API handler may have timed out."
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cpu_line() {
        let line = "cpu  123 456 789 1011 1213 1415 1617 1819 2021 2223";
        let cpu = parse_cpu_line(line).unwrap();

        assert_eq!(cpu.name, "cpu");
        assert_eq!(cpu.user, 123);
        assert_eq!(cpu.nice, 456);
        assert_eq!(cpu.system, 789);
        assert_eq!(cpu.idle, 1011);
        assert_eq!(cpu.iowait, 1213);
        assert_eq!(cpu.irq, 1415);
        assert_eq!(cpu.softirq, 1617);
        assert_eq!(cpu.steal, 1819);
        assert_eq!(cpu.guest, 2021);
        assert_eq!(cpu.guest_nice, 2223);
    }

    #[test]
    fn test_parse_cpu_line_minimal() {
        // Older kernels might have fewer fields
        let line = "cpu0 123 456 789 1011";
        let cpu = parse_cpu_line(line).unwrap();

        assert_eq!(cpu.name, "cpu0");
        assert_eq!(cpu.user, 123);
        assert_eq!(cpu.nice, 456);
        assert_eq!(cpu.system, 789);
        assert_eq!(cpu.idle, 1011);
        assert_eq!(cpu.iowait, 0);
    }
}
