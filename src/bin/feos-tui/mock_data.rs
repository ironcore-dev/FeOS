// Mock data structures that mirror the protobuf definitions

use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::atomic::{AtomicU64, Ordering};

// Global counters for dynamic data generation
static LOG_COUNTER: AtomicU64 = AtomicU64::new(0);
static UPDATE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct NetInterface {
    pub name: String,
    pub pci_address: String,
    pub mac_address: String,
}

#[derive(Debug, Clone)]
pub struct HostInfo {
    pub uptime: u64,           // seconds
    pub ram_total: u64,        // bytes
    pub ram_unused: u64,       // bytes
    pub num_cores: u32,
    pub ipv6_address: String,  // Host's IPv6 address
    pub delegated_prefix: String, // IPv6 delegated prefix
    pub net_interfaces: Vec<NetInterface>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VmStatus {
    Running,
    Stopped,
    Starting,
    Stopping,
    Error,
}

impl VmStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            VmStatus::Running => "Running",
            VmStatus::Stopped => "Stopped",
            VmStatus::Starting => "Starting",
            VmStatus::Stopping => "Stopping",
            VmStatus::Error => "Error",
        }
    }
}

#[derive(Debug, Clone)]
pub struct VmInfo {
    pub uuid: String,
    pub status: VmStatus,
    pub cpu_count: u32,
    pub memory_bytes: u64,
    pub image_uuid: String,
    pub image_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ContainerStatus {
    Running,
    Stopped,
    Starting,
    Stopping,
    Error,
    Exited,
}

impl ContainerStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContainerStatus::Running => "Running",
            ContainerStatus::Stopped => "Stopped",
            ContainerStatus::Starting => "Starting",
            ContainerStatus::Stopping => "Stopping",
            ContainerStatus::Error => "Error",
            ContainerStatus::Exited => "Exited",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub status: ContainerStatus,
    pub image: String,
    pub ports: Vec<String>,
    pub created: u64,        // Unix timestamp
    pub memory_limit: u64,   // bytes, 0 means no limit
    pub cpu_limit: f64,      // CPU cores, 0.0 means no limit
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: u64,        // Unix timestamp
    pub level: String,
    pub message: String,
}

// Dynamic mock data generators

pub fn get_mock_host_info() -> HostInfo {
    let update_count = UPDATE_COUNTER.load(Ordering::Relaxed);
    let base_uptime = 3600 * 24 * 7; // 1 week base
    
    // Simulate slightly changing RAM usage
    let ram_base_unused = 8 * 1024 * 1024 * 1024; // 8 GB base
    let ram_variation = (update_count % 20) * 100 * 1024 * 1024; // ±2GB variation
    let ram_unused = if update_count % 40 < 20 {
        ram_base_unused + ram_variation
    } else {
        ram_base_unused.saturating_sub(ram_variation)
    };

    HostInfo {
        uptime: base_uptime + (update_count * 5), // Increment uptime
        ram_total: 16 * 1024 * 1024 * 1024, // 16 GB
        ram_unused,
        num_cores: 8,
        ipv6_address: "2001:db8:1::1".to_string(),
        delegated_prefix: "2001:db8:1000::/48".to_string(),
        net_interfaces: vec![
            NetInterface {
                name: "eth0".to_string(),
                pci_address: "0000:00:03.0".to_string(),
                mac_address: "52:54:00:12:34:56".to_string(),
            },
            NetInterface {
                name: "eth1".to_string(),
                pci_address: "0000:00:04.0".to_string(),
                mac_address: "52:54:00:12:34:57".to_string(),
            },
        ],
    }
}

pub fn get_mock_vms() -> Vec<VmInfo> {
    let update_count = UPDATE_COUNTER.load(Ordering::Relaxed);
    
    // Simulate VM status changes over time
    let vm1_status = match (update_count / 10) % 4 {
        0 => VmStatus::Running,
        1 => VmStatus::Running,
        2 => VmStatus::Stopping,
        3 => VmStatus::Stopped,
        _ => VmStatus::Running,
    };
    
    let vm3_status = match (update_count / 15) % 5 {
        0 => VmStatus::Stopped,
        1 => VmStatus::Starting,
        2 => VmStatus::Running,
        3 => VmStatus::Running,
        4 => VmStatus::Running,
        _ => VmStatus::Stopped,
    };
    
    let vm4_status = if update_count % 30 < 25 {
        VmStatus::Error
    } else {
        VmStatus::Stopped
    };

    vec![
        VmInfo {
            uuid: "vm-001".to_string(),
            status: vm1_status,
            cpu_count: 2,
            memory_bytes: 2 * 1024 * 1024 * 1024, // 2 GB
            image_uuid: "img-ubuntu-001".to_string(),
            image_name: "Ubuntu 22.04".to_string(),
        },
        VmInfo {
            uuid: "vm-002".to_string(),
            status: VmStatus::Running, // Always running
            cpu_count: 4,
            memory_bytes: 4 * 1024 * 1024 * 1024, // 4 GB
            image_uuid: "img-fedora-001".to_string(),
            image_name: "Fedora 39".to_string(),
        },
        VmInfo {
            uuid: "vm-003".to_string(),
            status: vm3_status,
            cpu_count: 1,
            memory_bytes: 1 * 1024 * 1024 * 1024, // 1 GB
            image_uuid: "img-alpine-001".to_string(),
            image_name: "Alpine Linux".to_string(),
        },
        VmInfo {
            uuid: "vm-004".to_string(),
            status: vm4_status,
            cpu_count: 2,
            memory_bytes: 2 * 1024 * 1024 * 1024, // 2 GB
            image_uuid: "img-debian-001".to_string(),
            image_name: "Debian 12".to_string(),
        },
    ]
}

pub fn get_mock_containers() -> Vec<ContainerInfo> {
    let update_count = UPDATE_COUNTER.load(Ordering::Relaxed);
    let base_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    // Simulate container status changes over time
    let web_container_status = match (update_count / 8) % 4 {
        0 => ContainerStatus::Running,
        1 => ContainerStatus::Running,
        2 => ContainerStatus::Stopping,
        3 => ContainerStatus::Stopped,
        _ => ContainerStatus::Running,
    };
    
    let api_container_status = if update_count % 25 < 20 {
        ContainerStatus::Running
    } else {
        ContainerStatus::Error
    };
    
    let db_container_status = match (update_count / 12) % 5 {
        0 => ContainerStatus::Stopped,
        1 => ContainerStatus::Starting,
        2 => ContainerStatus::Running,
        3 => ContainerStatus::Running,
        4 => ContainerStatus::Running,
        _ => ContainerStatus::Stopped,
    };
    
    let temp_container_status = match (update_count / 6) % 6 {
        0 => ContainerStatus::Starting,
        1 => ContainerStatus::Running,
        2 => ContainerStatus::Running,
        3 => ContainerStatus::Stopping,
        4 => ContainerStatus::Exited,
        5 => ContainerStatus::Stopped,
        _ => ContainerStatus::Stopped,
    };

    vec![
        ContainerInfo {
            id: "c1a2b3c4d5e6".to_string(),
            name: "feos-web-server".to_string(),
            status: web_container_status,
            image: "nginx:alpine".to_string(),
            ports: vec!["80:8080".to_string(), "443:8443".to_string()],
            created: base_time - 7200, // 2 hours ago
            memory_limit: 512 * 1024 * 1024, // 512 MB
            cpu_limit: 1.0,
        },
        ContainerInfo {
            id: "f7g8h9i0j1k2".to_string(),
            name: "feos-api-service".to_string(),
            status: api_container_status,
            image: "feos/api:v1.2.3".to_string(),
            ports: vec!["3000:3000".to_string()],
            created: base_time - 14400, // 4 hours ago  
            memory_limit: 1024 * 1024 * 1024, // 1 GB
            cpu_limit: 2.0,
        },
        ContainerInfo {
            id: "l3m4n5o6p7q8".to_string(),
            name: "feos-database".to_string(),
            status: db_container_status,
            image: "postgres:15-alpine".to_string(),
            ports: vec!["5432:5432".to_string()],
            created: base_time - 86400, // 1 day ago
            memory_limit: 2048 * 1024 * 1024, // 2 GB
            cpu_limit: 1.5,
        },
        ContainerInfo {
            id: "r9s0t1u2v3w4".to_string(),
            name: "temp-worker".to_string(),
            status: temp_container_status,
            image: "alpine:latest".to_string(),
            ports: vec![],
            created: base_time - 300, // 5 minutes ago
            memory_limit: 0, // No limit
            cpu_limit: 0.0, // No limit
        },
        ContainerInfo {
            id: "x5y6z7a8b9c0".to_string(),
            name: "feos-monitoring".to_string(),
            status: ContainerStatus::Running, // Always running
            image: "prometheus:latest".to_string(),
            ports: vec!["9090:9090".to_string()],
            created: base_time - 43200, // 12 hours ago
            memory_limit: 256 * 1024 * 1024, // 256 MB
            cpu_limit: 0.5,
        },
    ]
}

// Streaming log simulation - generates new log entries
pub fn get_new_feos_log_entries() -> Vec<LogEntry> {
    let log_count = LOG_COUNTER.fetch_add(1, Ordering::Relaxed);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    let messages = vec![
        "VM health check completed",
        "Network interface eth0 traffic: 1.2MB/s",
        "Memory cleanup completed",
        "VM vm-001 CPU usage: 45%",
        "Image cache updated",
        "gRPC connection established from 192.168.1.100",
        "VM vm-002 disk I/O: 150 IOPS",
        "System backup initiated",
        "Container registry sync completed",
        "Network bridge br0 status: UP",
        "Container feos-web-server started successfully",
        "Container feos-api-service health check passed",
        "Container feos-database connection pool initialized",
        "Container temp-worker exited with code 0",
        "Container image pulled: nginx:alpine",
        "Container port binding updated: 80:8080",
    ];
    
    // Generate 1-3 new log entries per call
    let num_entries = (log_count % 3) + 1;
    let mut entries = Vec::new();
    
    for i in 0..num_entries {
        let level = match (log_count + i) % 10 {
            0..=6 => "INFO",
            7..=8 => "WARN", 
            9 => "ERROR",
            _ => "INFO",
        };
        
        entries.push(LogEntry {
            timestamp: now + i,
            level: level.to_string(),
            message: messages[((log_count + i) as usize) % messages.len()].to_string(),
        });
    }
    
    entries
}

pub fn get_new_kernel_log_entries() -> Vec<LogEntry> {
    let log_count = LOG_COUNTER.load(Ordering::Relaxed);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    let messages = vec![
        "KVM: vcpu0 disabled perfctr wrmsr",
        "bridge: filtering via arp/ip/ip6tables is no longer available",
        "device eth0 entered promiscuous mode",
        "TCP: request_sock_TCP: Possible SYN flooding",
        "Out of memory: Kill process 1234 (stress) score 900",
        "block device sda1: I/O error, dev sda1, sector 12345",
        "CPU0: Package temperature above threshold",
        "EXT4-fs (sda1): mounted filesystem with ordered data mode",
        "systemd[1]: Started Network Manager",
        "kernel: [12345.678901] usb 1-1: new high-speed USB device",
    ];
    
    // Generate fewer kernel logs (0-2 per call)
    let num_entries = log_count % 3;
    let mut entries = Vec::new();
    
    for i in 0..num_entries {
        let level = match (log_count + i) % 8 {
            0..=5 => "INFO",
            6 => "WARN",
            7 => "ERROR",
            _ => "INFO",
        };
        
        entries.push(LogEntry {
            timestamp: now + i,
            level: level.to_string(),
            message: messages[((log_count + i) as usize) % messages.len()].to_string(),
        });
    }
    
    entries
}

// Static initial logs for testing
pub fn get_mock_feos_logs() -> Vec<LogEntry> {
    vec![
        LogEntry {
            timestamp: 1705314615, // 2024-01-15T10:30:15Z
            level: "INFO".to_string(),
            message: "FeOS daemon started successfully".to_string(),
        },
        LogEntry {
            timestamp: 1705314616, // 2024-01-15T10:30:16Z
            level: "INFO".to_string(),
            message: "Listening for gRPC connections on 0.0.0.0:50051".to_string(),
        },
        LogEntry {
            timestamp: 1705314682, // 2024-01-15T10:31:22Z
            level: "INFO".to_string(),
            message: "VM vm-001 started successfully".to_string(),
        },
        LogEntry {
            timestamp: 1705314730, // 2024-01-15T10:32:10Z
            level: "WARN".to_string(),
            message: "VM vm-004 failed to start: insufficient memory".to_string(),
        },
        LogEntry {
            timestamp: 1705314825, // 2024-01-15T10:33:45Z
            level: "INFO".to_string(),
            message: "Image fetch completed: img-ubuntu-001".to_string(),
        },
    ]
}

pub fn get_mock_kernel_logs() -> Vec<LogEntry> {
    vec![
        LogEntry {
            timestamp: 1705314598, // 2024-01-15T10:29:58Z
            level: "INFO".to_string(),
            message: "Linux version 6.12.21 (FeOS build system)".to_string(),
        },
        LogEntry {
            timestamp: 1705314599, // 2024-01-15T10:29:59Z
            level: "INFO".to_string(),
            message: "Command line: BOOT_IMAGE=/boot/vmlinuz root=/dev/sda1".to_string(),
        },
        LogEntry {
            timestamp: 1705314601, // 2024-01-15T10:30:01Z
            level: "INFO".to_string(),
            message: "Memory: 16384000K/16777216K available".to_string(),
        },
        LogEntry {
            timestamp: 1705314602, // 2024-01-15T10:30:02Z
            level: "INFO".to_string(),
            message: "CPU: Intel(R) Xeon(R) CPU E5-2686 v4 @ 2.30GHz".to_string(),
        },
        LogEntry {
            timestamp: 1705314603, // 2024-01-15T10:30:03Z
            level: "INFO".to_string(),
            message: "KVM: Hardware virtualization enabled".to_string(),
        },
    ]
}

// Helper function to advance time-based updates
pub fn tick_updates() {
    UPDATE_COUNTER.fetch_add(1, Ordering::Relaxed);
}

// Helper functions for resource calculations
pub fn get_ram_usage_percentage(host_info: &HostInfo) -> f64 {
    let used = host_info.ram_total - host_info.ram_unused;
    (used as f64 / host_info.ram_total as f64) * 100.0
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;
    
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    
    if unit_index == 0 {
        format!("{} {}", size as u64, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

pub fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;
    
    if days > 0 {
        format!("{}d {}h {}m", days, hours, minutes)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    }
}

// Dynamic CPU usage simulation
pub fn get_mock_cpu_history() -> Vec<u64> {
    let update_count = UPDATE_COUNTER.load(Ordering::Relaxed);
    
    // Generate 60 data points for better sparkline density
    let mut history = Vec::with_capacity(60);
    
    // Always include baseline values to ensure proper 0-100% scaling
    history.push(0);  // First point at 0% to anchor the scale
    
    // Create a more complex pattern with multiple waves for the middle points
    for i in 1..59 {
        let base_wave1 = 40.0 + 20.0 * ((i as f64 * 0.2).sin()); // Primary wave
        let base_wave2 = 15.0 * ((i as f64 * 0.5).sin()); // Secondary wave  
        let base_wave3 = 8.0 * ((i as f64 * 1.2).cos()); // Tertiary wave
        
        let base_value = base_wave1 + base_wave2 + base_wave3;
        
        // Add dynamic variation based on update counter
        let variation = ((update_count + i as u64) % 15) as i64 - 7; // ±7% variation
        let final_value = (base_value as i64 + variation).max(5).min(95) as u64;
        
        history.push(final_value);
    }
    
    history
}

// Dynamic memory usage simulation  
pub fn get_mock_memory_history() -> Vec<u64> {
    let update_count = UPDATE_COUNTER.load(Ordering::Relaxed);
    
    // Generate 60 data points for better sparkline density
    let mut history = Vec::with_capacity(60);
    
    // Always include baseline values to ensure proper 0-100% scaling
    history.push(0);  // First point at 0% to anchor the scale
    
    // Create a different pattern for memory (generally higher, more stable)
    for i in 1..59 {
        let base_wave1 = 55.0 + 15.0 * ((i as f64 * 0.15).sin()); // Primary wave
        let base_wave2 = 8.0 * ((i as f64 * 0.7).cos()); // Secondary wave
        let base_wave3 = 5.0 * ((i as f64 * 2.1).sin()); // Tertiary wave
        
        let base_value = base_wave1 + base_wave2 + base_wave3;
        
        // Add dynamic variation based on update counter  
        let variation = ((update_count * 3 + i as u64) % 12) as i64 - 6; // ±6% variation
        let final_value = (base_value as i64 + variation).max(20).min(85) as u64;
        
        history.push(final_value);
    }
    
    history
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1048576), "1.0 MB");
        assert_eq!(format_bytes(1073741824), "1.0 GB");
        assert_eq!(format_bytes(17179869184), "16.0 GB");
    }

    #[test]
    fn test_mock_host_info() {
        let host_info = get_mock_host_info();
        
        assert_eq!(host_info.num_cores, 8);
        assert_eq!(host_info.ram_total, 17179869184); // 16 GB
        assert!(host_info.ram_unused < host_info.ram_total);
        assert!(host_info.uptime > 0);
        assert_eq!(host_info.net_interfaces.len(), 2);
        
        // Check network interfaces have valid data
        for interface in &host_info.net_interfaces {
            assert!(!interface.name.is_empty());
            assert!(!interface.pci_address.is_empty());
            assert!(!interface.mac_address.is_empty());
        }
    }

    #[test]
    fn test_mock_vms() {
        let vms = get_mock_vms();
        
        assert_eq!(vms.len(), 4);
        
        // Check each VM has valid data
        for vm in &vms {
            assert!(!vm.uuid.is_empty());
            assert!(!vm.image_name.is_empty());
            assert!(vm.cpu_count > 0);
            assert!(vm.memory_bytes > 0);
            assert!(!vm.image_uuid.is_empty());
        }
        
        // Check we have different statuses
        let statuses: Vec<_> = vms.iter().map(|vm| &vm.status).collect();
        assert!(statuses.contains(&&VmStatus::Running));
        assert!(statuses.contains(&&VmStatus::Stopped));
    }

    #[test]
    fn test_vm_status_display() {
        assert_eq!(VmStatus::Running.as_str(), "Running");
        assert_eq!(VmStatus::Stopped.as_str(), "Stopped");
        assert_eq!(VmStatus::Starting.as_str(), "Starting");
        assert_eq!(VmStatus::Stopping.as_str(), "Stopping");
        assert_eq!(VmStatus::Error.as_str(), "Error");
    }

    #[test]
    fn test_mock_history_data() {
        let cpu_history = get_mock_cpu_history();
        let memory_history = get_mock_memory_history();
        
        assert_eq!(cpu_history.len(), 60);
        assert_eq!(memory_history.len(), 60);
        
        // Check values are within reasonable ranges
        for &cpu in &cpu_history {
            assert!(cpu <= 100); // CPU percentage should be 0-100
        }
        
        for &memory in &memory_history {
            assert!(memory <= 100); // Memory percentage should be 0-100
        }
    }

    #[test]
    fn test_mock_logs() {
        let feos_logs = get_mock_feos_logs();
        let kernel_logs = get_mock_kernel_logs();
        
        assert!(!feos_logs.is_empty());
        assert!(!kernel_logs.is_empty());
        
        // Check log entries have valid data
        for log in &feos_logs {
            assert!(!log.message.is_empty());
            assert!(log.timestamp > 0);
        }
        
        for log in &kernel_logs {
            assert!(!log.message.is_empty());  
            assert!(log.timestamp > 0);
        }
    }
} 