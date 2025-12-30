// SPDX-FileCopyrightText: 2025 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use feos_proto::host_service::host_service_client::HostServiceClient;
use feos_proto::host_service::GetKernelStatsRequest;
use tonic::transport::Channel;

pub async fn get_kernel_stats(client: &mut HostServiceClient<Channel>) -> Result<()> {
    use std::time::Duration;
    use tokio::time::sleep;

    println!("Taking CPU measurements...");

    // First sample
    let request = GetKernelStatsRequest {};
    let response1 = client.get_kernel_stats(request).await?.into_inner();
    let stats1 = response1.stats.context("No CPU stats in response")?;

    // Wait 1 second
    sleep(Duration::from_secs(1)).await;

    // Second sample
    let stats2 = client
        .get_kernel_stats(GetKernelStatsRequest {})
        .await?
        .into_inner()
        .stats
        .context("No CPU stats in response")?;

    // Calculate and display usage
    display_cpu_usage(&stats1, &stats2);

    // Display additional stats
    println!("\nSystem Statistics:");
    println!("  Context Switches: {}", stats2.context_switches);
    println!("  Boot Time:        {}", format_boot_time(stats2.boot_time));
    println!(
        "  Processes:        {} created, {} running, {} blocked",
        stats2.processes_created, stats2.processes_running, stats2.processes_blocked
    );

    Ok(())
}

fn display_cpu_usage(
    stats1: &feos_proto::host_service::KernelStats,
    stats2: &feos_proto::host_service::KernelStats,
) {
    println!("\nCPU Usage (1 second average):");

    // Calculate total CPU usage
    if let (Some(total1), Some(total2)) = (&stats1.total, &stats2.total) {
        let usage = calculate_cpu_usage_percent(total1, total2);
        println!(
            "  Overall:  {:>5.1}% (user: {:.1}%, system: {:.1}%, iowait: {:.1}%)",
            usage.total, usage.user, usage.system, usage.iowait
        );
    }

    // Calculate per-CPU usage
    println!("\n  Per-CPU:");
    for (cpu1, cpu2) in stats1.per_cpu.iter().zip(stats2.per_cpu.iter()) {
        let usage = calculate_cpu_usage_percent(cpu1, cpu2);
        println!("    {:<6} {:>5.1}%", cpu2.name, usage.total);
    }
}

struct CpuUsagePercent {
    total: f64,
    user: f64,
    system: f64,
    iowait: f64,
}

fn calculate_cpu_usage_percent(
    before: &feos_proto::host_service::CpuTime,
    after: &feos_proto::host_service::CpuTime,
) -> CpuUsagePercent {
    let total_before = before.user
        + before.nice
        + before.system
        + before.idle
        + before.iowait
        + before.irq
        + before.softirq
        + before.steal;
    let total_after = after.user
        + after.nice
        + after.system
        + after.idle
        + after.iowait
        + after.irq
        + after.softirq
        + after.steal;

    let total_delta = total_after.saturating_sub(total_before) as f64;

    if total_delta == 0.0 {
        return CpuUsagePercent {
            total: 0.0,
            user: 0.0,
            system: 0.0,
            iowait: 0.0,
        };
    }

    let idle_delta = (after.idle + after.iowait).saturating_sub(before.idle + before.iowait) as f64;
    let user_delta = after.user.saturating_sub(before.user) as f64;
    let system_delta = after.system.saturating_sub(before.system) as f64;
    let iowait_delta = after.iowait.saturating_sub(before.iowait) as f64;

    CpuUsagePercent {
        total: ((total_delta - idle_delta) / total_delta) * 100.0,
        user: (user_delta / total_delta) * 100.0,
        system: (system_delta / total_delta) * 100.0,
        iowait: (iowait_delta / total_delta) * 100.0,
    }
}

fn format_boot_time(boot_time: u64) -> String {
    use chrono::{DateTime, TimeZone, Utc};
    let dt: DateTime<Utc> = Utc
        .timestamp_opt(boot_time as i64, 0)
        .single()
        .unwrap_or_default();
    dt.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use feos_proto::host_service::CpuTime;

    fn create_cpu_time(
        name: &str,
        user: u64,
        nice: u64,
        system: u64,
        idle: u64,
        iowait: u64,
    ) -> CpuTime {
        CpuTime {
            name: name.to_string(),
            user,
            nice,
            system,
            idle,
            iowait,
            irq: 0,
            softirq: 0,
            steal: 0,
            guest: 0,
            guest_nice: 0,
        }
    }

    #[test]
    fn test_calculate_cpu_usage_percent_basic() {
        // Sample 1: total = 1000 ticks, idle = 800, iowait = 50
        let before = create_cpu_time("cpu0", 100, 0, 50, 800, 50);
        // Sample 2: total = 1500 ticks, idle = 900, iowait = 150 (delta: 500 total, 200 idle+iowait)
        let after = create_cpu_time("cpu0", 200, 150, 100, 900, 150);

        let usage = calculate_cpu_usage_percent(&before, &after);

        // Total delta = 500, idle+iowait delta = (900+150) - (800+50) = 200
        // Busy = 500 - 200 = 300
        // Usage = 300/500 * 100 = 60%
        assert_eq!(usage.total, 60.0);

        // User delta = 100, system delta = 50
        assert_eq!(usage.user, 20.0); // 100/500 * 100
        assert_eq!(usage.system, 10.0); // 50/500 * 100
    }

    #[test]
    fn test_calculate_cpu_usage_percent_zero_delta() {
        let cpu = create_cpu_time("cpu0", 100, 0, 50, 800, 50);

        // Same values - no time passed
        let usage = calculate_cpu_usage_percent(&cpu, &cpu);

        assert_eq!(usage.total, 0.0);
        assert_eq!(usage.user, 0.0);
        assert_eq!(usage.system, 0.0);
        assert_eq!(usage.iowait, 0.0);
    }

    #[test]
    fn test_calculate_cpu_usage_percent_100_percent() {
        // All busy, no idle time
        let before = create_cpu_time("cpu0", 100, 0, 50, 50, 0);
        let after = create_cpu_time("cpu0", 200, 0, 150, 50, 0);

        let usage = calculate_cpu_usage_percent(&before, &after);

        // Total delta = 200, idle delta = 0
        // Usage should be 100%
        assert_eq!(usage.total, 100.0);
    }

    #[test]
    fn test_calculate_cpu_usage_percent_all_idle() {
        // All idle time
        let before = create_cpu_time("cpu0", 100, 0, 50, 800, 50);
        let after = create_cpu_time("cpu0", 100, 0, 50, 1300, 50);

        let usage = calculate_cpu_usage_percent(&before, &after);

        // Only idle increased by 500
        // Usage should be 0%
        assert_eq!(usage.total, 0.0);
    }

    #[test]
    fn test_calculate_cpu_usage_percent_high_iowait() {
        let before = create_cpu_time("cpu0", 100, 0, 50, 800, 50);
        let after = create_cpu_time("cpu0", 150, 0, 75, 900, 375);

        let usage = calculate_cpu_usage_percent(&before, &after);

        // IOWait delta = 325 out of 500 total
        assert_eq!(usage.iowait, 65.0);
    }

    #[test]
    fn test_format_boot_time() {
        // Test with known timestamp: 2024-01-15 12:00:00 UTC
        let timestamp = 1705320000u64;
        let formatted = format_boot_time(timestamp);

        assert_eq!(formatted, "2024-01-15 12:00:00 UTC");
    }

    #[test]
    fn test_format_boot_time_epoch() {
        let formatted = format_boot_time(0);
        assert_eq!(formatted, "1970-01-01 00:00:00 UTC");
    }
}
