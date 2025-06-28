// Mock data structures that mirror the protobuf definitions

use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::atomic::{AtomicU64, Ordering};

// Global counters for dynamic data generation
static LOG_COUNTER: AtomicU64 = AtomicU64::new(0);
static UPDATE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct NetInterface {
    pub name: String,
    pub mac_address: String,
    pub ipv6_address: String,
}

#[derive(Debug, Clone)]
pub struct HostInfo {
    pub uptime: u64,           // seconds
    pub ram_total: u64,        // bytes
    pub ram_unused: u64,       // bytes
    pub num_cores: u32,
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

#[derive(Debug, Clone, PartialEq)]
pub enum IsolatedPodStatus {
    Running,
    Stopped,
    Starting,
    Stopping,
    Error,
}

impl IsolatedPodStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            IsolatedPodStatus::Running => "Running",
            IsolatedPodStatus::Stopped => "Stopped",
            IsolatedPodStatus::Starting => "Starting",
            IsolatedPodStatus::Stopping => "Stopping",
            IsolatedPodStatus::Error => "Error",
        }
    }
}

#[derive(Debug, Clone)]
pub struct IsolatedPodContainer {
    pub id: String,
    pub name: String,
    pub status: ContainerStatus,
    pub image: String,
    pub memory_limit: u64,   // bytes, 0 means no limit
    pub cpu_limit: f64,      // CPU cores, 0.0 means no limit
    pub restart_count: u32,
}

#[derive(Debug, Clone)]
pub struct IsolatedPodInfo {
    pub id: String,
    pub name: String,
    pub status: IsolatedPodStatus,
    pub microvm_id: String,
    pub cpu_count: u32,
    pub memory_bytes: u64,
    pub kernel_version: String,
    pub created: u64,        // Unix timestamp
    pub containers: Vec<IsolatedPodContainer>,
    pub kernel_logs: Vec<LogEntry>,
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
        delegated_prefix: "2001:db8:1000::/48".to_string(),
        net_interfaces: vec![
            NetInterface {
                name: "eth0".to_string(),
                mac_address: "52:54:00:12:34:56".to_string(),
                ipv6_address: "2001:db8:1::10".to_string(),
            },
            NetInterface {
                name: "eth1".to_string(),
                mac_address: "52:54:00:12:34:57".to_string(),
                ipv6_address: "2001:db8:1::11".to_string(),
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

pub fn get_mock_isolated_pods() -> Vec<IsolatedPodInfo> {
    let update_count = UPDATE_COUNTER.load(Ordering::Relaxed);
    let base_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    // Simulate pod status changes over time
    let web_pod_status = match (update_count / 7) % 4 {
        0 => IsolatedPodStatus::Running,
        1 => IsolatedPodStatus::Running,
        2 => IsolatedPodStatus::Stopping,
        3 => IsolatedPodStatus::Stopped,
        _ => IsolatedPodStatus::Running,
    };
    
    let data_pod_status = if update_count % 20 < 16 {
        IsolatedPodStatus::Running
    } else {
        IsolatedPodStatus::Error
    };
    
    let ml_pod_status = match (update_count / 10) % 5 {
        0 => IsolatedPodStatus::Stopped,
        1 => IsolatedPodStatus::Starting,
        2 => IsolatedPodStatus::Running,
        3 => IsolatedPodStatus::Running,
        4 => IsolatedPodStatus::Running,
        _ => IsolatedPodStatus::Stopped,
    };

    let monitoring_pod_status = match (update_count / 9) % 3 {
        0 => IsolatedPodStatus::Running,
        1 => IsolatedPodStatus::Running,
        2 => IsolatedPodStatus::Starting,
        _ => IsolatedPodStatus::Running,
    };

    let gaming_pod_status = match (update_count / 13) % 4 {
        0 => IsolatedPodStatus::Stopped,
        1 => IsolatedPodStatus::Starting,
        2 => IsolatedPodStatus::Running,
        3 => IsolatedPodStatus::Running,
        _ => IsolatedPodStatus::Stopped,
    };

    let dev_pod_status = if update_count % 18 < 14 {
        IsolatedPodStatus::Running
    } else {
        IsolatedPodStatus::Error
    };

    // Generate kernel logs for each pod
    let web_pod_kernel_logs = generate_pod_kernel_logs("microvm-web-001", base_time - 7200, "web");
    let data_pod_kernel_logs = generate_pod_kernel_logs("microvm-data-001", base_time - 14400, "data");
    let ml_pod_kernel_logs = generate_pod_kernel_logs("microvm-ml-001", base_time - 3600, "ml");
    let monitoring_pod_kernel_logs = generate_pod_kernel_logs("microvm-monitoring-001", base_time - 10800, "monitoring");
    let gaming_pod_kernel_logs = generate_pod_kernel_logs("microvm-gaming-001", base_time - 5400, "gaming");
    let dev_pod_kernel_logs = generate_pod_kernel_logs("microvm-dev-001", base_time - 1800, "dev");

    vec![
        IsolatedPodInfo {
            id: "pod-web-001".to_string(),
            name: "web-application-pod".to_string(),
            status: web_pod_status.clone(),
            microvm_id: "microvm-web-001".to_string(),
            cpu_count: 4,
            memory_bytes: 4 * 1024 * 1024 * 1024, // 4 GB
            kernel_version: "6.12.21-feos".to_string(),
            created: base_time - 7200, // 2 hours ago
            containers: vec![
                IsolatedPodContainer {
                    id: "c-nginx-001".to_string(),
                    name: "nginx-frontend".to_string(),
                    status: if web_pod_status.clone() == IsolatedPodStatus::Running { ContainerStatus::Running } else { ContainerStatus::Stopped },
                    image: "nginx:1.25-alpine".to_string(),
                    memory_limit: 512 * 1024 * 1024, // 512 MB
                    cpu_limit: 1.0,
                    restart_count: 0,
                },
                IsolatedPodContainer {
                    id: "c-app-001".to_string(),
                    name: "web-app".to_string(),
                    status: if web_pod_status.clone() == IsolatedPodStatus::Running { ContainerStatus::Running } else { ContainerStatus::Stopped },
                    image: "node:18-alpine".to_string(),
                    memory_limit: 1024 * 1024 * 1024, // 1 GB
                    cpu_limit: 2.0,
                    restart_count: 1,
                },
                IsolatedPodContainer {
                    id: "c-redis-001".to_string(),
                    name: "redis-cache".to_string(),
                    status: if web_pod_status.clone() == IsolatedPodStatus::Running { ContainerStatus::Running } else { ContainerStatus::Stopped },
                    image: "redis:7-alpine".to_string(),
                    memory_limit: 256 * 1024 * 1024, // 256 MB
                    cpu_limit: 0.5,
                    restart_count: 0,
                },
            ],
            kernel_logs: web_pod_kernel_logs,
        },
        IsolatedPodInfo {
            id: "pod-data-001".to_string(),
            name: "data-processing-pod".to_string(),
            status: data_pod_status.clone(),
            microvm_id: "microvm-data-001".to_string(),
            cpu_count: 8,
            memory_bytes: 8 * 1024 * 1024 * 1024, // 8 GB
            kernel_version: "6.12.21-feos".to_string(),
            created: base_time - 14400, // 4 hours ago
            containers: vec![
                IsolatedPodContainer {
                    id: "c-postgres-001".to_string(),
                    name: "postgres-db".to_string(),
                    status: if data_pod_status.clone() == IsolatedPodStatus::Running { ContainerStatus::Running } else { ContainerStatus::Error },
                    image: "postgres:15".to_string(),
                    memory_limit: 4 * 1024 * 1024 * 1024, // 4 GB
                    cpu_limit: 4.0,
                    restart_count: 2,
                },
                IsolatedPodContainer {
                    id: "c-etl-001".to_string(),
                    name: "data-processor".to_string(),
                    status: if data_pod_status.clone() == IsolatedPodStatus::Running { ContainerStatus::Running } else { ContainerStatus::Exited },
                    image: "python:3.11-slim".to_string(),
                    memory_limit: 2 * 1024 * 1024 * 1024, // 2 GB
                    cpu_limit: 3.0,
                    restart_count: 5,
                },
            ],
            kernel_logs: data_pod_kernel_logs,
        },
        IsolatedPodInfo {
            id: "pod-ml-001".to_string(),
            name: "machine-learning-pod".to_string(),
            status: ml_pod_status.clone(),
            microvm_id: "microvm-ml-001".to_string(),
            cpu_count: 6,
            memory_bytes: 12 * 1024 * 1024 * 1024, // 12 GB
            kernel_version: "6.12.21-feos".to_string(),
            created: base_time - 3600, // 1 hour ago
            containers: vec![
                IsolatedPodContainer {
                    id: "c-jupyter-001".to_string(),
                    name: "jupyter-notebook".to_string(),
                    status: if ml_pod_status.clone() == IsolatedPodStatus::Running { ContainerStatus::Running } else { ContainerStatus::Stopped },
                    image: "jupyter/tensorflow-notebook:latest".to_string(),
                    memory_limit: 8 * 1024 * 1024 * 1024, // 8 GB
                    cpu_limit: 4.0,
                    restart_count: 0,
                },
                IsolatedPodContainer {
                    id: "c-model-001".to_string(),
                    name: "model-server".to_string(),
                    status: if ml_pod_status == IsolatedPodStatus::Running { ContainerStatus::Starting } else { ContainerStatus::Stopped },
                    image: "tensorflow/serving:latest".to_string(),
                    memory_limit: 2 * 1024 * 1024 * 1024, // 2 GB
                    cpu_limit: 2.0,
                    restart_count: 3,
                },
            ],
            kernel_logs: ml_pod_kernel_logs,
        },
        IsolatedPodInfo {
            id: "pod-monitoring-001".to_string(),
            name: "monitoring-observability-pod".to_string(),
            status: monitoring_pod_status.clone(),
            microvm_id: "microvm-monitoring-001".to_string(),
            cpu_count: 4,
            memory_bytes: 6 * 1024 * 1024 * 1024, // 6 GB
            kernel_version: "6.12.21-feos".to_string(),
            created: base_time - 10800, // 3 hours ago
            containers: vec![
                IsolatedPodContainer {
                    id: "c-prometheus-001".to_string(),
                    name: "prometheus-server".to_string(),
                    status: if monitoring_pod_status.clone() == IsolatedPodStatus::Running { ContainerStatus::Running } else { ContainerStatus::Stopped },
                    image: "prom/prometheus:v2.45.0".to_string(),
                    memory_limit: 2 * 1024 * 1024 * 1024, // 2 GB
                    cpu_limit: 1.5,
                    restart_count: 0,
                },
                IsolatedPodContainer {
                    id: "c-grafana-001".to_string(),
                    name: "grafana-dashboard".to_string(),
                    status: if monitoring_pod_status.clone() == IsolatedPodStatus::Running { ContainerStatus::Running } else { ContainerStatus::Stopped },
                    image: "grafana/grafana:10.0.3".to_string(),
                    memory_limit: 1 * 1024 * 1024 * 1024, // 1 GB
                    cpu_limit: 1.0,
                    restart_count: 1,
                },
                IsolatedPodContainer {
                    id: "c-alertmanager-001".to_string(),
                    name: "alertmanager".to_string(),
                    status: if monitoring_pod_status.clone() == IsolatedPodStatus::Running { ContainerStatus::Running } else { ContainerStatus::Stopped },
                    image: "prom/alertmanager:v0.25.0".to_string(),
                    memory_limit: 512 * 1024 * 1024, // 512 MB
                    cpu_limit: 0.5,
                    restart_count: 0,
                },
                IsolatedPodContainer {
                    id: "c-loki-001".to_string(),
                    name: "loki-logs".to_string(),
                    status: if monitoring_pod_status.clone() == IsolatedPodStatus::Running { ContainerStatus::Running } else { ContainerStatus::Stopped },
                    image: "grafana/loki:2.9.0".to_string(),
                    memory_limit: 1536 * 1024 * 1024, // 1.5 GB
                    cpu_limit: 1.0,
                    restart_count: 2,
                },
            ],
            kernel_logs: monitoring_pod_kernel_logs,
        },
        IsolatedPodInfo {
            id: "pod-gaming-001".to_string(),
            name: "gaming-media-server-pod".to_string(),
            status: gaming_pod_status.clone(),
            microvm_id: "microvm-gaming-001".to_string(),
            cpu_count: 8,
            memory_bytes: 16 * 1024 * 1024 * 1024, // 16 GB
            kernel_version: "6.12.21-feos".to_string(),
            created: base_time - 5400, // 1.5 hours ago
            containers: vec![
                IsolatedPodContainer {
                    id: "c-minecraft-001".to_string(),
                    name: "minecraft-server".to_string(),
                    status: if gaming_pod_status.clone() == IsolatedPodStatus::Running { ContainerStatus::Running } else { ContainerStatus::Stopped },
                    image: "itzg/minecraft-server:latest".to_string(),
                    memory_limit: 8 * 1024 * 1024 * 1024, // 8 GB
                    cpu_limit: 4.0,
                    restart_count: 1,
                },
                IsolatedPodContainer {
                    id: "c-plex-001".to_string(),
                    name: "plex-media-server".to_string(),
                    status: if gaming_pod_status.clone() == IsolatedPodStatus::Running { ContainerStatus::Running } else { ContainerStatus::Stopped },
                    image: "plexinc/pms-docker:latest".to_string(),
                    memory_limit: 4 * 1024 * 1024 * 1024, // 4 GB
                    cpu_limit: 2.0,
                    restart_count: 0,
                },
                IsolatedPodContainer {
                    id: "c-discord-001".to_string(),
                    name: "discord-bot".to_string(),
                    status: if gaming_pod_status.clone() == IsolatedPodStatus::Running { ContainerStatus::Running } else { ContainerStatus::Stopped },
                    image: "node:18-alpine".to_string(),
                    memory_limit: 256 * 1024 * 1024, // 256 MB
                    cpu_limit: 0.5,
                    restart_count: 3,
                },
            ],
            kernel_logs: gaming_pod_kernel_logs,
        },
        IsolatedPodInfo {
            id: "pod-dev-001".to_string(),
            name: "development-cicd-pod".to_string(),
            status: dev_pod_status.clone(),
            microvm_id: "microvm-dev-001".to_string(),
            cpu_count: 6,
            memory_bytes: 10 * 1024 * 1024 * 1024, // 10 GB
            kernel_version: "6.12.21-feos".to_string(),
            created: base_time - 1800, // 30 minutes ago
            containers: vec![
                IsolatedPodContainer {
                    id: "c-jenkins-001".to_string(),
                    name: "jenkins-master".to_string(),
                    status: if dev_pod_status.clone() == IsolatedPodStatus::Running { ContainerStatus::Running } else { ContainerStatus::Error },
                    image: "jenkins/jenkins:lts".to_string(),
                    memory_limit: 4 * 1024 * 1024 * 1024, // 4 GB
                    cpu_limit: 2.0,
                    restart_count: 1,
                },
                IsolatedPodContainer {
                    id: "c-gitlab-001".to_string(),
                    name: "gitlab-runner".to_string(),
                    status: if dev_pod_status.clone() == IsolatedPodStatus::Running { ContainerStatus::Running } else { ContainerStatus::Exited },
                    image: "gitlab/gitlab-runner:latest".to_string(),
                    memory_limit: 2 * 1024 * 1024 * 1024, // 2 GB
                    cpu_limit: 2.0,
                    restart_count: 4,
                },
                IsolatedPodContainer {
                    id: "c-registry-001".to_string(),
                    name: "docker-registry".to_string(),
                    status: if dev_pod_status.clone() == IsolatedPodStatus::Running { ContainerStatus::Running } else { ContainerStatus::Stopped },
                    image: "registry:2".to_string(),
                    memory_limit: 1 * 1024 * 1024 * 1024, // 1 GB
                    cpu_limit: 1.0,
                    restart_count: 0,
                },
                IsolatedPodContainer {
                    id: "c-sonarqube-001".to_string(),
                    name: "sonarqube-analysis".to_string(),
                    status: if dev_pod_status.clone() == IsolatedPodStatus::Running { ContainerStatus::Starting } else { ContainerStatus::Stopped },
                    image: "sonarqube:community".to_string(),
                    memory_limit: 2 * 1024 * 1024 * 1024, // 2 GB
                    cpu_limit: 1.0,
                    restart_count: 2,
                },
            ],
            kernel_logs: dev_pod_kernel_logs,
        },
    ]
}

fn generate_pod_kernel_logs(microvm_id: &str, start_time: u64, pod_type: &str) -> Vec<LogEntry> {
    let mut messages = vec![
        format!("Linux version 6.12.21-feos booting in {}", microvm_id),
        format!("{}: Command line: root=/dev/vda1 console=ttyS0 quiet", microvm_id),
        format!("{}: Memory: 4096MB available, 256MB reserved", microvm_id),
        format!("{}: CPU: 4 vCPUs initialized (Intel Xeon)", microvm_id),
        format!("{}: virtio_net: Ethernet address: 52:54:00:XX:XX:XX", microvm_id),
        format!("{}: virtio_blk: registered block device vda", microvm_id),
        format!("{}: EXT4-fs (vda1): mounted filesystem with ordered data mode", microvm_id),
        format!("{}: systemd[1]: Starting system initialization", microvm_id),
        format!("{}: systemd[1]: Started containerd container runtime", microvm_id),
        format!("{}: systemd[1]: Started networking service", microvm_id),
        format!("{}: bridge: filtering via iptables enabled", microvm_id),
        format!("{}: IPv6: ADDRCONF(NETDEV_CHANGE): eth0: link becomes ready", microvm_id),
        format!("{}: OOM killer disabled for system containers", microvm_id),
        format!("{}: TCP congestion control: bbr registered", microvm_id),
        format!("{}: vsock device initialized (CID: auto)", microvm_id),
        format!("{}: KVM: Hardware virtualization support enabled", microvm_id),
        format!("{}: cgroup: memory controller enabled", microvm_id),
        format!("{}: cgroup: cpu controller enabled", microvm_id),
        format!("{}: firewall: iptables v1.8.9 initialized", microvm_id),
        format!("{}: audit: audit enabled (pid=1)", microvm_id),
    ];

    // Add pod-type specific kernel logs
    match pod_type {
        "web" => {
            messages.extend(vec![
                format!("{}: nginx: loading nginx configuration", microvm_id),
                format!("{}: nginx: worker process started", microvm_id),
                format!("{}: redis: server started, port 6379", microvm_id),
                format!("{}: node: application server listening on port 3000", microvm_id),
                format!("{}: systemd[1]: nginx.service: Started nginx HTTP server", microvm_id),
                format!("{}: systemd[1]: redis.service: Started redis in-memory database", microvm_id),
                format!("{}: HTTP/1.1 200 OK responses: 1247 requests served", microvm_id),
                format!("{}: SSL certificate loaded: /etc/ssl/certs/server.crt", microvm_id),
                format!("{}: load balancer: upstream server pool initialized", microvm_id),
                format!("{}: cache: redis connection pool established (10 connections)", microvm_id),
            ]);
        },
        "data" => {
            messages.extend(vec![
                format!("{}: postgres: database system is ready to accept connections", microvm_id),
                format!("{}: postgres: checkpoint completed, LSN 0/15000000", microvm_id),
                format!("{}: postgres: autovacuum launcher started", microvm_id),
                format!("{}: python: ETL pipeline initializing", microvm_id),
                format!("{}: python: connecting to data sources", microvm_id),
                format!("{}: systemd[1]: postgresql.service: Started PostgreSQL database", microvm_id),
                format!("{}: disk I/O: completed 2547 read operations, 1823 write operations", microvm_id),
                format!("{}: ETL: processed 15,429 records in batch job", microvm_id),
                format!("{}: postgres: slow query detected (2.3s): SELECT * FROM large_table", microvm_id),
                format!("{}: data pipeline: scheduler running, next job in 15 minutes", microvm_id),
            ]);
        },
        "ml" => {
            messages.extend(vec![
                format!("{}: tensorflow: GPU device not found, using CPU", microvm_id),
                format!("{}: jupyter: notebook server started on port 8888", microvm_id),
                format!("{}: tensorflow: loading model 'customer_churn_v2.h5'", microvm_id),
                format!("{}: model server: TensorFlow Serving 2.11.0 started", microvm_id),
                format!("{}: python: scikit-learn 1.3.0 initialized", microvm_id),
                format!("{}: systemd[1]: jupyter.service: Started Jupyter notebook server", microvm_id),
                format!("{}: model inference: processed 847 prediction requests", microvm_id),
                format!("{}: training job: epoch 15/100, loss: 0.0234, accuracy: 0.9156", microvm_id),
                format!("{}: memory usage: model cache using 3.2GB", microvm_id),
                format!("{}: ML pipeline: feature extraction completed (45,231 features)", microvm_id),
            ]);
        },
        "monitoring" => {
            messages.extend(vec![
                format!("{}: prometheus: server started on port 9090", microvm_id),
                format!("{}: grafana: HTTP server listening on port 3000", microvm_id),
                format!("{}: alertmanager: listening on port 9093", microvm_id),
                format!("{}: loki: started on port 3100", microvm_id),
                format!("{}: systemd[1]: prometheus.service: Started Prometheus monitoring", microvm_id),
                format!("{}: systemd[1]: grafana.service: Started Grafana visualization", microvm_id),
                format!("{}: prometheus: scraping 23 targets, 1,247 metrics collected", microvm_id),
                format!("{}: grafana: dashboard loaded: 'FeOS System Overview'", microvm_id),
                format!("{}: alertmanager: alert fired: 'HighMemoryUsage' on vm-002", microvm_id),
                format!("{}: loki: ingested 5,432 log entries from 12 sources", microvm_id),
            ]);
        },
        "gaming" => {
            messages.extend(vec![
                format!("{}: minecraft: server started on port 25565", microvm_id),
                format!("{}: minecraft: world 'survival_world' loaded", microvm_id),
                format!("{}: plex: media server started on port 32400", microvm_id),
                format!("{}: plex: library scan completed, 1,247 media files indexed", microvm_id),
                format!("{}: discord: bot connected to Discord API", microvm_id),
                format!("{}: systemd[1]: minecraft.service: Started Minecraft game server", microvm_id),
                format!("{}: systemd[1]: plex.service: Started Plex Media Server", microvm_id),
                format!("{}: minecraft: player 'Steve' joined the game", microvm_id),
                format!("{}: minecraft: player 'Alex' left the game", microvm_id),
                format!("{}: plex: transcoding H.264 to H.265 for client playback", microvm_id),
                format!("{}: discord: handled 156 slash commands", microvm_id),
                format!("{}: gaming: average server tick rate: 19.8 TPS", microvm_id),
            ]);
        },
        "dev" => {
            messages.extend(vec![
                format!("{}: jenkins: started on port 8080", microvm_id),
                format!("{}: jenkins: loaded 15 build jobs", microvm_id),
                format!("{}: gitlab-runner: registered with GitLab instance", microvm_id),
                format!("{}: docker: registry started on port 5000", microvm_id),
                format!("{}: sonarqube: quality gate analysis started", microvm_id),
                format!("{}: systemd[1]: jenkins.service: Started Jenkins automation server", microvm_id),
                format!("{}: systemd[1]: docker-registry.service: Started Docker Registry", microvm_id),
                format!("{}: jenkins: build job 'feos-web-app' #47 started", microvm_id),
                format!("{}: jenkins: build job 'feos-web-app' #47 completed successfully", microvm_id),
                format!("{}: gitlab-runner: job 'test-unit' completed in 2m 34s", microvm_id),
                format!("{}: sonarqube: code quality analysis: 0 bugs, 2 code smells", microvm_id),
                format!("{}: docker: pushed image 'feos/web-app:v1.2.4' to registry", microvm_id),
            ]);
        },
        _ => {
            // Default case - no additional messages
        }
    }

    // Add some dynamic runtime messages based on update counter
    let update_count = UPDATE_COUNTER.load(Ordering::Relaxed);
    let runtime_messages = match (update_count / 5) % 4 {
        0 => vec![
            format!("{}: systemd[1]: logrotate.service: Started log rotation", microvm_id),
            format!("{}: cron: daily backup job started", microvm_id),
        ],
        1 => vec![
            format!("{}: systemd[1]: systemd-tmpfiles-clean.service: Started cleanup", microvm_id),
            format!("{}: kernel: memory compaction completed", microvm_id),
        ],
        2 => vec![
            format!("{}: systemd[1]: apt-daily.service: Started daily package updates", microvm_id),
            format!("{}: security: system hardening check passed", microvm_id),
        ],
        3 => vec![
            format!("{}: systemd[1]: fstrim.service: Started file system trim", microvm_id),
            format!("{}: network: interface statistics updated", microvm_id),
        ],
        _ => vec![],
    };
    
    messages.extend(runtime_messages);

    messages.into_iter().enumerate().map(|(i, message)| {
        let level = match i % 20 {
            0..=15 => "INFO",
            16..=18 => "WARN",
            19 => "ERROR",
            _ => "INFO",
        };
        
        LogEntry {
            timestamp: start_time + (i as u64 * 8), // 8 seconds apart
            level: level.to_string(),
            message,
        }
    }).collect()
}

// Streaming log simulation - generates new log entries
pub fn get_new_feos_log_entries() -> Vec<LogEntry> {
    let log_count = LOG_COUNTER.fetch_add(1, Ordering::Relaxed);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    let messages = vec![
        // System-level messages
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
        
        // Container lifecycle messages
        "Container feos-web-server started successfully",
        "Container feos-api-service health check passed",
        "Container feos-database connection pool initialized",
        "Container temp-worker exited with code 0",
        "Container image pulled: nginx:alpine",
        "Container port binding updated: 80:8080",
        "Container feos-monitoring restarted after memory limit exceeded",
        "Container volume mounted: /data/persistent",
        "Container network attached: feos-bridge",
        "Container resource limits updated: 2GB RAM, 1.5 CPU",
        
        // Isolated pod messages
        "Isolated pod pod-web-001 microVM started",
        "Isolated pod pod-data-001 kernel boot completed in 2.3s",
        "Isolated pod pod-ml-001 container orchestration ready",
        "Isolated pod pod-monitoring-001 vsock connection established",
        "Isolated pod pod-gaming-001 network bridge configured",
        "Isolated pod pod-dev-001 storage volume attached",
        "MicroVM microvm-web-001 memory usage: 2.1GB/4GB",
        "MicroVM microvm-data-001 disk I/O: 2.5MB/s read, 1.8MB/s write",
        "MicroVM microvm-ml-001 CPU utilization: 78%",
        "MicroVM microvm-monitoring-001 network packets: 1,247 in, 956 out",
        
        // Service-specific messages
        "Prometheus metrics scraping completed: 23 targets, 5,432 metrics",
        "Grafana dashboard 'System Overview' rendered in 234ms",
        "AlertManager alert resolved: 'HighCPUUsage' on pod-web-001",
        "Loki log ingestion: 3,456 entries/minute",
        "Jenkins build pipeline 'feos-api' completed successfully",
        "GitLab CI/CD runner job 'test-integration' started",
        "Docker registry push: feos/web-app:v1.3.1 (234MB)",
        "SonarQube code analysis: 98.5% coverage, 2 code smells",
        "Minecraft server: 12 players online, 19.7 TPS",
        "Plex media server: 3 active streams, transcoding H.264→H.265",
        "Discord bot: handled 47 commands, 156 interactions",
        "PostgreSQL: checkpoint completed in 1.2s",
        "Redis cache: 15,432 keys, 87% hit rate",
        "Jupyter notebook server: 5 active sessions",
        "TensorFlow model inference: 234 predictions/second",
        
        // Network and security messages
        "Firewall rule updated: allow port 8080 from 192.168.1.0/24",
        "IPv6 address assigned: 2001:db8:1::15/64",
        "SSL certificate renewal: 45 days remaining",
        "VPN connection established: peer 192.168.100.5",
        "DNS query resolved: feos.local → 192.168.1.10",
        "Load balancer: upstream server health check passed",
        "Rate limiting: 156 requests/minute from 192.168.1.100",
        "Security scan completed: 0 vulnerabilities found",
        "SSH key authentication: user@192.168.1.50",
        "DHCP lease renewed: 192.168.1.25 (24h)",
        
        // Storage and backup messages
        "Filesystem usage: /data 72% full (45GB/62GB)",
        "Backup job completed: 2.3GB archived to remote storage",
        "Snapshot created: pod-data-001-20240115-1430",
        "Volume resize: /var/lib/containers expanded to 50GB",
        "Storage pool utilization: 156GB/500GB (31%)",
        "File system check: ext4 /dev/vda1 clean",
        "LVM snapshot: data-volume-snap-001 created",
        "NFS mount: /shared/data mounted successfully",
        "S3 sync: uploaded 47 files (234MB) to backup bucket",
        "Deduplication: saved 1.2GB storage space",
        
        // Performance and monitoring messages
        "CPU temperature: 67°C (normal range)",
        "Memory pressure: 2.1GB available, no swap usage",
        "Disk latency: average 1.2ms read, 2.1ms write",
        "Network throughput: 45Mbps inbound, 28Mbps outbound",
        "Cache hit ratio: 94% (Redis), 87% (application cache)",
        "Database connection pool: 15/50 connections active",
        "API response time: p95 234ms, p99 456ms",
        "Load average: 1.23, 1.45, 1.67 (1min, 5min, 15min)",
        "Memory fragmentation: 12% (acceptable)",
        "Thread pool: 23/100 threads active",
    ];
    
    // Generate 1-4 new log entries per call
    let num_entries = (log_count % 4) + 1;
    let mut entries = Vec::new();
    
    for i in 0..num_entries {
        let level = match (log_count + i) % 15 {
            0..=10 => "INFO",
            11..=13 => "WARN", 
            14 => "ERROR",
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
        // Core kernel messages
        "KVM: vcpu0 disabled perfctr wrmsr",
        "bridge: filtering via arp/ip/ip6tables is no longer available",
        "device eth0 entered promiscuous mode",
        "TCP: request_sock_TCP: Possible SYN flooding on port 80",
        "device eth0 left promiscuous mode",
        "CPU0: Package temperature above threshold, cpu clock throttled",
        "CPU0: Package temperature/speed normal",
        "EXT4-fs (sda1): mounted filesystem with ordered data mode",
        "systemd[1]: Started Network Manager",
        "kernel: usb 1-1: new high-speed USB device using ehci-pci",
        
        // Memory management
        "Out of memory: Kill process 1234 (stress-test) score 900 or sacrifice child",
        "oom-killer: Killed process 5678 (heavy-app) total-vm:2GB, anon-rss:1.5GB",
        "Memory compaction completed successfully",
        "kswapd0: low memory condition resolved",
        "hugepages: allocated 512 pages (1GB total)",
        "meminfo: MemAvailable: 4567890 kB",
        "page allocation stalls: 0, page reclaim: 1234",
        "transparent hugepage: enabled for all applications",
        "SLUB: unable to allocate memory on node -1, gfp=0x280da",
        "memory: usage 67%, swap usage 0%",
        
        // Block device and filesystem
        "block device sda1: I/O error, dev sda1, sector 12345 op 0x1",
        "EXT4-fs (sda1): recovery complete",
        "EXT4-fs (sda1): mounted filesystem with ordered data mode, opts: (null)",
        "device mapper: multipath: queue_if_no_path enabled",
        "JBD2: recovery complete for journal sda1-8",
        "XFS (sdb1): Mounting V5 Filesystem",
        "XFS (sdb1): Ending clean mount",
        "block device sdb: partition table created",
        "device sda: optimal transfer size 1048576 bytes",
        "I/O scheduler mq-deadline registered",
        
        // Network subsystem
        "IPv6: ADDRCONF(NETDEV_CHANGE): eth0: link becomes ready",
        "IPv6: ADDRCONF(NETDEV_UP): eth0: link is not ready",
        "bridge: port 1(veth0) entered forwarding state",
        "bridge: port 1(veth0) entered disabled state",
        "netfilter: iptables rule added: ACCEPT tcp dpt:80",
        "TCP: request_sock_TCP: hash table order 7, 32768 entries",
        "UDP hash table entries: 2048 (order: 4, 65536 bytes)",
        "TCP established hash table entries: 8192 (order: 4, 65536 bytes)",
        "NET: Registered protocol family 10 (IPv6)",
        "bridge: filtering via iptables enabled",
        
        // Virtualization and containers
        "KVM: Hardware virtualization enabled",
        "containerd: runtime ready",
        "systemd[1]: Started containerd container runtime",
        "docker0: port 1(veth1234567) entered forwarding state",
        "cgroup: memory controller enabled",
        "cgroup: cpu controller enabled",
        "overlayfs: filesystem mounted: /var/lib/containers/overlay",
        "VFIO - User Level meta-driver version: 0.3",
        "KVM: nested virtualization enabled",
        "vhost-vsock: probe of vhost-vsock-pci.0 succeeded",
        
        // Security and audit
        "audit: enabled (pid=1)",
        "SELinux: initialized (disabled by user)",
        "capability: warning: 'runc' uses deprecated v2 capability",
        "audit: type=1400 audit(1642123456.789:123): avc: denied",
        "LSM: Security Framework initialized",
        "AppArmor: AppArmor filesystem support enabled",
        "TOMOYO: Linux Security Module initialized",
        "integrity: Loaded X.509 cert 'Build time autogenerated kernel key'",
        "random: crng init done",
        "Kernel is locked down from module loading",
        
        // Performance and monitoring
        "perf: interrupt took too long (12345 > 12000), lowering",
        "RCU: Adjusting geometry for rcu_fanout_leaf=16, nr_cpu_ids=8",
        "sched: RT throttling activated",
        "workqueue: max_active 256 requested for ordered workqueue",
        "CPU frequency scaling: governor performance enabled",
        "intel_pstate: CPU model not supported",
        "thermal: CPU0: temperature 45°C, fan speed 2000 RPM",
        "cpufreq: CPU0 scaling frequency 2400 MHz",
        "timer: spurious interrupt detected on CPU0",
        "clocksource: tsc unstable (delta = 12345 ns)",
    ];
    
    // Generate fewer kernel logs (0-2 per call)
    let num_entries = log_count % 3;
    let mut entries = Vec::new();
    
    for i in 0..num_entries {
        let level = match (log_count + i) % 12 {
            0..=8 => "INFO",
            9..=10 => "WARN",
            11 => "ERROR",
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

// New function to generate container-specific log entries
pub fn get_new_container_log_entries(container_name: &str) -> Vec<LogEntry> {
    let log_count = LOG_COUNTER.load(Ordering::Relaxed);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    let base_messages = vec![
        format!("{}: application started successfully", container_name),
        format!("{}: health check endpoint /health responding", container_name),
        format!("{}: memory usage: 234MB/512MB (45%)", container_name),
        format!("{}: CPU usage: 23%", container_name),
        format!("{}: network connections: 15 active", container_name),
        format!("{}: cache cleared, 1,234 entries removed", container_name),
        format!("{}: configuration reloaded", container_name),
        format!("{}: graceful shutdown initiated", container_name),
        format!("{}: signal SIGTERM received", container_name),
        format!("{}: cleanup completed", container_name),
    ];
    
    // Container-type specific messages
    let specific_messages = if container_name.contains("nginx") || container_name.contains("web") {
        vec![
            format!("{}: 192.168.1.100 GET /api/status HTTP/1.1 200", container_name),
            format!("{}: 192.168.1.101 POST /api/users HTTP/1.1 201", container_name),
            format!("{}: SSL handshake completed for 192.168.1.102", container_name),
            format!("{}: worker process 1234 started", container_name),
            format!("{}: served 1,456 requests in last minute", container_name),
            format!("{}: gzip compression enabled for response", container_name),
            format!("{}: upstream server pool health check passed", container_name),
            format!("{}: rate limiting: 234 requests/minute from 192.168.1.50", container_name),
        ]
    } else if container_name.contains("postgres") || container_name.contains("database") {
        vec![
            format!("{}: database system is ready to accept connections", container_name),
            format!("{}: checkpoint starting: time", container_name),
            format!("{}: checkpoint complete: wrote 156 buffers", container_name),
            format!("{}: autovacuum: processing table 'public.users'", container_name),
            format!("{}: connection received: host=192.168.1.10 port=5432", container_name),
            format!("{}: connection authorized: user=app_user database=production", container_name),
            format!("{}: slow query: SELECT * FROM large_table (2.3s)", container_name),
            format!("{}: query cache hit ratio: 94%", container_name),
        ]
    } else if container_name.contains("redis") || container_name.contains("cache") {
        vec![
            format!("{}: server started, port 6379", container_name),
            format!("{}: DB loaded from disk: 2.345 seconds", container_name),
            format!("{}: 1000 changes in 60 seconds. Saving...", container_name),
            format!("{}: background saving started by pid 1234", container_name),
            format!("{}: DB saved on disk", container_name),
            format!("{}: keyspace: db0 keys=15432,expires=234", container_name),
            format!("{}: client connected from 192.168.1.20:45678", container_name),
            format!("{}: memory usage: 67MB peak, 45MB current", container_name),
        ]
    } else if container_name.contains("prometheus") || container_name.contains("monitoring") {
        vec![
            format!("{}: server started on port 9090", container_name),
            format!("{}: scraping target 'node-exporter' (1/23)", container_name),
            format!("{}: metrics ingestion: 5,432 samples/sec", container_name),
            format!("{}: TSDB compaction completed in 234ms", container_name),
            format!("{}: query executed in 45ms: rate(http_requests_total[5m])", container_name),
            format!("{}: retention policy: keeping 30 days of data", container_name),
            format!("{}: storage: 2.3GB used, 45GB available", container_name),
            format!("{}: remote write: sent 1,234 samples to endpoint", container_name),
        ]
    } else if container_name.contains("grafana") {
        vec![
            format!("{}: HTTP server listening on port 3000", container_name),
            format!("{}: database migration completed", container_name),
            format!("{}: user 'admin' logged in from 192.168.1.100", container_name),
            format!("{}: dashboard 'System Overview' rendered in 234ms", container_name),
            format!("{}: alert rule 'High CPU Usage' evaluated", container_name),
            format!("{}: plugin 'prometheus' loaded successfully", container_name),
            format!("{}: query executed: prometheus query in 123ms", container_name),
            format!("{}: provisioning: loading dashboards from /etc/grafana/provisioning", container_name),
        ]
    } else if container_name.contains("jenkins") {
        vec![
            format!("{}: Jenkins started on port 8080", container_name),
            format!("{}: build job 'web-app-build' #47 started", container_name),
            format!("{}: build job 'web-app-build' #47 completed: SUCCESS", container_name),
            format!("{}: executor 1 started build 'api-tests' #23", container_name),
            format!("{}: SCM polling: checking for changes in Git repository", container_name),
            format!("{}: plugin 'pipeline-stage-view' loaded", container_name),
            format!("{}: workspace cleanup: removed 234MB of old artifacts", container_name),
            format!("{}: agent 'linux-worker-01' connected", container_name),
        ]
    } else if container_name.contains("minecraft") {
        vec![
            format!("{}: server started on port 25565", container_name),
            format!("{}: world 'survival_world' loaded in 2.3s", container_name),
            format!("{}: player 'Steve' joined the game", container_name),
            format!("{}: player 'Alex' left the game", container_name),
            format!("{}: saving world 'survival_world'", container_name),
            format!("{}: tick rate: 19.8 TPS (target: 20 TPS)", container_name),
            format!("{}: memory usage: 1.2GB/8GB", container_name),
            format!("{}: 12 players online, 145 chunks loaded", container_name),
        ]
    } else if container_name.contains("plex") {
        vec![
            format!("{}: media server started on port 32400", container_name),
            format!("{}: library scan: Movies - 1,247 items", container_name),
            format!("{}: transcoding: H.264 → H.265 for client 192.168.1.50", container_name),
            format!("{}: stream started: 1080p → 720p for mobile client", container_name),
            format!("{}: metadata updated for 'Movie Title (2023)'", container_name),
            format!("{}: thumbnail generation: 45 images processed", container_name),
            format!("{}: 3 active streams, 2 direct play, 1 transcode", container_name),
            format!("{}: client 'Living Room TV' connected", container_name),
        ]
    } else {
        vec![]
    };
    
    let mut all_messages = base_messages;
    all_messages.extend(specific_messages);
    
    // Generate 0-2 entries per call
    let num_entries = log_count % 3;
    let mut entries = Vec::new();
    
    for i in 0..num_entries {
        let level = match (log_count + i) % 10 {
            0..=7 => "INFO",
            8 => "WARN",
            9 => "ERROR",
            _ => "INFO",
        };
        
        entries.push(LogEntry {
            timestamp: now + i,
            level: level.to_string(),
            message: all_messages[((log_count + i) as usize) % all_messages.len()].to_string(),
        });
    }
    
    entries
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
    
    // Generate 120 data points for much denser sparkline visualization
    let mut history = Vec::with_capacity(120);
    
    // Always include baseline values to ensure proper 0-100% scaling
    history.push(0);  // First point at 0% to anchor the scale
    
    // Create a more complex pattern with multiple waves for the middle points
    for i in 1..119 {
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
    
    // Generate 120 data points for much denser sparkline visualization
    let mut history = Vec::with_capacity(120);
    
    // Always include baseline values to ensure proper 0-100% scaling
    history.push(0);  // First point at 0% to anchor the scale
    
    // Create a different pattern for memory (generally higher, more stable)
    for i in 1..119 {
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
        LogEntry {
            timestamp: 1705314900, // 2024-01-15T10:35:00Z
            level: "INFO".to_string(),
            message: "Container feos-web-server restarted successfully".to_string(),
        },
        LogEntry {
            timestamp: 1705314950, // 2024-01-15T10:35:50Z
            level: "ERROR".to_string(),
            message: "Failed to connect to database: timeout".to_string(),
        },
        LogEntry {
            timestamp: 1705315000, // 2024-01-15T10:36:40Z
            level: "INFO".to_string(),
            message: "Backup completed successfully".to_string(),
        },
        LogEntry {
            timestamp: 1705315050, // 2024-01-15T10:37:30Z
            level: "WARN".to_string(),
            message: "High memory usage detected on VM vm-002".to_string(),
        },
        LogEntry {
            timestamp: 1705315100, // 2024-01-15T10:38:20Z
            level: "INFO".to_string(),
            message: "New container image pulled: feos/api:v1.3.0".to_string(),
        },
        LogEntry {
            timestamp: 1705315150, // 2024-01-15T10:39:10Z
            level: "ERROR".to_string(),
            message: "Disk space low on /var/lib/containers".to_string(),
        },
        LogEntry {
            timestamp: 1705315200, // 2024-01-15T10:40:00Z
            level: "INFO".to_string(),
            message: "Network configuration updated for eth0".to_string(),
        },
        LogEntry {
            timestamp: 1705315250, // 2024-01-15T10:40:50Z
            level: "WARN".to_string(),
            message: "Potential security threat detected: unusual login pattern".to_string(),
        },
        LogEntry {
            timestamp: 1705315300, // 2024-01-15T10:41:40Z
            level: "INFO".to_string(),
            message: "System maintenance completed successfully".to_string(),
        },
        LogEntry {
            timestamp: 1705315350, // 2024-01-15T10:42:30Z
            level: "INFO".to_string(),
            message: "Isolated pod pod-web-001 started successfully".to_string(),
        },
        LogEntry {
            timestamp: 1705315400, // 2024-01-15T10:43:20Z
            level: "ERROR".to_string(),
            message: "Container feos-database failed health check".to_string(),
        },
        LogEntry {
            timestamp: 1705315450, // 2024-01-15T10:44:10Z
            level: "INFO".to_string(),
            message: "VM vm-003 migration completed to host node-02".to_string(),
        },
        LogEntry {
            timestamp: 1705315500, // 2024-01-15T10:45:00Z
            level: "WARN".to_string(),
            message: "Network latency spike detected: 250ms to external gateway".to_string(),
        },
        LogEntry {
            timestamp: 1705315550, // 2024-01-15T10:45:50Z
            level: "INFO".to_string(),
            message: "Certificate renewal scheduled for feos.local".to_string(),
        },
        LogEntry {
            timestamp: 1705315600, // 2024-01-15T10:46:40Z
            level: "ERROR".to_string(),
            message: "Failed to allocate resources for new VM: quota exceeded".to_string(),
        },
        LogEntry {
            timestamp: 1705315650, // 2024-01-15T10:47:30Z
            level: "INFO".to_string(),
            message: "Load balancer configuration updated: 3 backend servers".to_string(),
        },
        LogEntry {
            timestamp: 1705315700, // 2024-01-15T10:48:20Z
            level: "WARN".to_string(),
            message: "CPU throttling activated on VM vm-002 due to thermal limits".to_string(),
        },
        LogEntry {
            timestamp: 1705315750, // 2024-01-15T10:49:10Z
            level: "INFO".to_string(),
            message: "Storage pool expanded: +500GB available space".to_string(),
        },
        LogEntry {
            timestamp: 1705315800, // 2024-01-15T10:50:00Z
            level: "ERROR".to_string(),
            message: "Container registry authentication failed for user deploy-bot".to_string(),
        },
        LogEntry {
            timestamp: 1705315850, // 2024-01-15T10:50:50Z
            level: "INFO".to_string(),
            message: "Scheduled backup job completed: 2.3GB archived".to_string(),
        },
        LogEntry {
            timestamp: 1705315900, // 2024-01-15T10:51:40Z
            level: "WARN".to_string(),
            message: "Firewall rule conflict detected: duplicate port 8080 mapping".to_string(),
        },
        LogEntry {
            timestamp: 1705315950, // 2024-01-15T10:52:30Z
            level: "INFO".to_string(),
            message: "MicroVM microvm-data-001 kernel updated to 6.12.21-feos".to_string(),
        },
        LogEntry {
            timestamp: 1705316000, // 2024-01-15T10:53:20Z
            level: "ERROR".to_string(),
            message: "DNS resolution failed for external service api.example.com".to_string(),
        },
        LogEntry {
            timestamp: 1705316050, // 2024-01-15T10:54:10Z
            level: "INFO".to_string(),
            message: "Container orchestration: scaling up web-tier to 5 replicas".to_string(),
        },
        LogEntry {
            timestamp: 1705316100, // 2024-01-15T10:55:00Z
            level: "WARN".to_string(),
            message: "Memory pressure detected: initiating garbage collection".to_string(),
        },
        LogEntry {
            timestamp: 1705316150, // 2024-01-15T10:55:50Z
            level: "INFO".to_string(),
            message: "VPN tunnel established to remote datacenter dc-west-02".to_string(),
        },
        LogEntry {
            timestamp: 1705316200, // 2024-01-15T10:56:40Z
            level: "ERROR".to_string(),
            message: "Snapshot creation failed: insufficient disk space".to_string(),
        },
        LogEntry {
            timestamp: 1705316250, // 2024-01-15T10:57:30Z
            level: "INFO".to_string(),
            message: "Security scan completed: 0 critical vulnerabilities found".to_string(),
        },
        LogEntry {
            timestamp: 1705316300, // 2024-01-15T10:58:20Z
            level: "WARN".to_string(),
            message: "Unusual network traffic pattern detected from 192.168.1.100".to_string(),
        },
        LogEntry {
            timestamp: 1705316350, // 2024-01-15T10:59:10Z
            level: "INFO".to_string(),
            message: "System telemetry uploaded to monitoring service".to_string(),
        },
        LogEntry {
            timestamp: 1705316400, // 2024-01-15T11:00:00Z
            level: "ERROR".to_string(),
            message: "Cluster node node-03 unreachable: network partition detected".to_string(),
        },
        LogEntry {
            timestamp: 1705316450, // 2024-01-15T11:00:50Z
            level: "INFO".to_string(),
            message: "Automated failover completed: services migrated to node-01".to_string(),
        },
        LogEntry {
            timestamp: 1705316500, // 2024-01-15T11:01:40Z
            level: "WARN".to_string(),
            message: "Rate limiting activated: 1000 req/min threshold exceeded".to_string(),
        },
        LogEntry {
            timestamp: 1705316550, // 2024-01-15T11:02:30Z
            level: "INFO".to_string(),
            message: "Container image garbage collection freed 1.2GB space".to_string(),
        },
        LogEntry {
            timestamp: 1705316600, // 2024-01-15T11:03:20Z
            level: "ERROR".to_string(),
            message: "Failed to mount NFS share: connection timeout".to_string(),
        },
        LogEntry {
            timestamp: 1705316650, // 2024-01-15T11:04:10Z
            level: "INFO".to_string(),
            message: "Log rotation completed: archived 500MB of historical logs".to_string(),
        },
        LogEntry {
            timestamp: 1705316700, // 2024-01-15T11:05:00Z
            level: "INFO".to_string(),
            message: "FeOS system health check completed successfully".to_string(),
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
        LogEntry {
            timestamp: 1705314604, // 2024-01-15T10:30:04Z
            level: "WARN".to_string(),
            message: "CPU0: Package temperature above threshold, cpu clock throttled".to_string(),
        },
        LogEntry {
            timestamp: 1705314605, // 2024-01-15T10:30:05Z
            level: "INFO".to_string(),
            message: "EXT4-fs (sda1): mounted filesystem with ordered data mode".to_string(),
        },
        LogEntry {
            timestamp: 1705314606, // 2024-01-15T10:30:06Z
            level: "ERROR".to_string(),
            message: "Out of memory: Kill process 1234 (stress-test) score 900 or sacrifice child".to_string(),
        },
        LogEntry {
            timestamp: 1705314607, // 2024-01-15T10:30:07Z
            level: "INFO".to_string(),
            message: "Network interface eth0 up and running".to_string(),
        },
        LogEntry {
            timestamp: 1705314608, // 2024-01-15T10:30:08Z
            level: "WARN".to_string(),
            message: "Disk I/O error on /dev/sda1, sector 123456".to_string(),
        },
        LogEntry {
            timestamp: 1705314609, // 2024-01-15T10:30:09Z
            level: "INFO".to_string(),
            message: "Systemd: Starting system initialization".to_string(),
        },
        LogEntry {
            timestamp: 1705314610, // 2024-01-15T10:30:10Z
            level: "ERROR".to_string(),
            message: "Kernel panic - not syncing: VFS: Unable to mount root fs on unknown-block(0,0)".to_string(),
        },
        LogEntry {
            timestamp: 1705314611, // 2024-01-15T10:30:11Z
            level: "INFO".to_string(),
            message: "Rebooting in 1 seconds..".to_string(),
        },
        LogEntry {
            timestamp: 1705314612, // 2024-01-15T10:30:12Z
            level: "INFO".to_string(),
            message: "PCI: Using configuration type 1 for base access".to_string(),
        },
        LogEntry {
            timestamp: 1705314613, // 2024-01-15T10:30:13Z
            level: "WARN".to_string(),
            message: "ACPI: BIOS _OSI(Linux) query ignored".to_string(),
        },
        LogEntry {
            timestamp: 1705314614, // 2024-01-15T10:30:14Z
            level: "INFO".to_string(),
            message: "RTC time: 10:30:14, date: 01/15/24".to_string(),
        },
        LogEntry {
            timestamp: 1705314615, // 2024-01-15T10:30:15Z
            level: "ERROR".to_string(),
            message: "ACPI Error: Could not enable RealTimeClock event".to_string(),
        },
        LogEntry {
            timestamp: 1705314616, // 2024-01-15T10:30:16Z
            level: "INFO".to_string(),
            message: "TCP: Hash tables configured (established 8192 bind 8192)".to_string(),
        },
        LogEntry {
            timestamp: 1705314617, // 2024-01-15T10:30:17Z
            level: "WARN".to_string(),
            message: "IPv6: ADDRCONF(NETDEV_UP): eth0: link is not ready".to_string(),
        },
        LogEntry {
            timestamp: 1705314618, // 2024-01-15T10:30:18Z
            level: "INFO".to_string(),
            message: "bridge: filtering via iptables enabled".to_string(),
        },
        LogEntry {
            timestamp: 1705314619, // 2024-01-15T10:30:19Z
            level: "ERROR".to_string(),
            message: "block device sda: I/O error, dev sda, sector 789012".to_string(),
        },
        LogEntry {
            timestamp: 1705314620, // 2024-01-15T10:30:20Z
            level: "INFO".to_string(),
            message: "device-mapper: multipath service started".to_string(),
        },
        LogEntry {
            timestamp: 1705314621, // 2024-01-15T10:30:21Z
            level: "WARN".to_string(),
            message: "audit: backlog limit exceeded".to_string(),
        },
        LogEntry {
            timestamp: 1705314622, // 2024-01-15T10:30:22Z
            level: "INFO".to_string(),
            message: "XFS (sdb1): Mounting V5 Filesystem".to_string(),
        },
        LogEntry {
            timestamp: 1705314623, // 2024-01-15T10:30:23Z
            level: "ERROR".to_string(),
            message: "EXT4-fs (sdc1): bad geometry: block count 2097152 exceeds size of device".to_string(),
        },
        LogEntry {
            timestamp: 1705314624, // 2024-01-15T10:30:24Z
            level: "INFO".to_string(),
            message: "thermal LNXTHERM:00: registered as thermal_zone0".to_string(),
        },
        LogEntry {
            timestamp: 1705314625, // 2024-01-15T10:30:25Z
            level: "WARN".to_string(),
            message: "CPU frequency scaling: governor performance enabled".to_string(),
        },
        LogEntry {
            timestamp: 1705314626, // 2024-01-15T10:30:26Z
            level: "INFO".to_string(),
            message: "random: crng init done".to_string(),
        },
        LogEntry {
            timestamp: 1705314627, // 2024-01-15T10:30:27Z
            level: "ERROR".to_string(),
            message: "usb 1-1: device descriptor read/64, error -71".to_string(),
        },
        LogEntry {
            timestamp: 1705314628, // 2024-01-15T10:30:28Z
            level: "INFO".to_string(),
            message: "clocksource: tsc: mask: 0xffffffffffffffff max_cycles: 0x1cd42e4dffb".to_string(),
        },
        LogEntry {
            timestamp: 1705314629, // 2024-01-15T10:30:29Z
            level: "WARN".to_string(),
            message: "intel_pstate: CPU model not supported".to_string(),
        },
        LogEntry {
            timestamp: 1705314630, // 2024-01-15T10:30:30Z
            level: "INFO".to_string(),
            message: "workqueue: max_active 256 requested for ordered workqueue".to_string(),
        },
        LogEntry {
            timestamp: 1705314631, // 2024-01-15T10:30:31Z
            level: "ERROR".to_string(),
            message: "DMAR: DRHD handling fault status reg 3".to_string(),
        },
        LogEntry {
            timestamp: 1705314632, // 2024-01-15T10:30:32Z
            level: "INFO".to_string(),
            message: "VFIO - User Level meta-driver version: 0.3".to_string(),
        },
        LogEntry {
            timestamp: 1705314633, // 2024-01-15T10:30:33Z
            level: "WARN".to_string(),
            message: "perf: interrupt took too long (2500 > 2495), lowering kernel.perf_event_max_sample_rate to 79750".to_string(),
        },
        LogEntry {
            timestamp: 1705314634, // 2024-01-15T10:30:34Z
            level: "INFO".to_string(),
            message: "overlayfs: filesystem mounted".to_string(),
        },
        LogEntry {
            timestamp: 1705314635, // 2024-01-15T10:30:35Z
            level: "ERROR".to_string(),
            message: "SLUB: unable to allocate memory on node -1, gfp=0x280da".to_string(),
        },
        LogEntry {
            timestamp: 1705314636, // 2024-01-15T10:30:36Z
            level: "INFO".to_string(),
            message: "cgroup: memory controller enabled".to_string(),
        },
        LogEntry {
            timestamp: 1705314637, // 2024-01-15T10:30:37Z
            level: "WARN".to_string(),
            message: "RCU: Adjusting geometry for rcu_fanout_leaf=16, nr_cpu_ids=8".to_string(),
        },
        LogEntry {
            timestamp: 1705314638, // 2024-01-15T10:30:38Z
            level: "INFO".to_string(),
            message: "containerd: runtime ready".to_string(),
        },
        LogEntry {
            timestamp: 1705314639, // 2024-01-15T10:30:39Z
            level: "ERROR".to_string(),
            message: "TCP: request_sock_TCP: Possible SYN flooding on port 22. Dropping request.".to_string(),
        },
        LogEntry {
            timestamp: 1705314640, // 2024-01-15T10:30:40Z
            level: "INFO".to_string(),
            message: "vhost-vsock: probe of vhost-vsock-pci.0 succeeded".to_string(),
        },
        LogEntry {
            timestamp: 1705314641, // 2024-01-15T10:30:41Z
            level: "WARN".to_string(),
            message: "capability: warning: 'runc' uses deprecated v2 capability".to_string(),
        },
        LogEntry {
            timestamp: 1705314642, // 2024-01-15T10:30:42Z
            level: "INFO".to_string(),
            message: "AppArmor: AppArmor filesystem support enabled".to_string(),
        },
        LogEntry {
            timestamp: 1705314643, // 2024-01-15T10:30:43Z
            level: "ERROR".to_string(),
            message: "audit: type=1400 audit(1705314643.123:456): avc: denied { read } for pid=1234".to_string(),
        },
        LogEntry {
            timestamp: 1705314644, // 2024-01-15T10:30:44Z
            level: "INFO".to_string(),
            message: "integrity: Loaded X.509 cert 'Build time autogenerated kernel key'".to_string(),
        },
        LogEntry {
            timestamp: 1705314645, // 2024-01-15T10:30:45Z
            level: "WARN".to_string(),
            message: "clocksource: tsc unstable (delta = -123456789 ns)".to_string(),
        },
        LogEntry {
            timestamp: 1705314646, // 2024-01-15T10:30:46Z
            level: "INFO".to_string(),
            message: "NET: Registered protocol family 10 (IPv6)".to_string(),
        },
        LogEntry {
            timestamp: 1705314647, // 2024-01-15T10:30:47Z
            level: "ERROR".to_string(),
            message: "kvm: disabled by bios".to_string(),
        },
        LogEntry {
            timestamp: 1705314648, // 2024-01-15T10:30:48Z
            level: "INFO".to_string(),
            message: "IPv6: ADDRCONF(NETDEV_CHANGE): eth0: link becomes ready".to_string(),
        },
        LogEntry {
            timestamp: 1705314649, // 2024-01-15T10:30:49Z
            level: "WARN".to_string(),
            message: "sched: RT throttling activated".to_string(),
        },
        LogEntry {
            timestamp: 1705314650, // 2024-01-15T10:30:50Z
            level: "INFO".to_string(),
            message: "docker0: port 1(veth1234567) entered forwarding state".to_string(),
        },
        LogEntry {
            timestamp: 1705314651, // 2024-01-15T10:30:51Z
            level: "ERROR".to_string(),
            message: "memory: page allocation failure: order:3, mode:0x40c0(GFP_NOIO|__GFP_COMP)".to_string(),
        },
        LogEntry {
            timestamp: 1705314652, // 2024-01-15T10:30:52Z
            level: "INFO".to_string(),
            message: "systemd[1]: Started Network Manager".to_string(),
        },
        LogEntry {
            timestamp: 1705314653, // 2024-01-15T10:30:53Z
            level: "WARN".to_string(),
            message: "kernel: usb 1-1: new high-speed USB device using ehci-pci".to_string(),
        },
        LogEntry {
            timestamp: 1705314654, // 2024-01-15T10:30:54Z
            level: "INFO".to_string(),
            message: "JBD2: recovery complete for journal sda1-8".to_string(),
        },
        LogEntry {
            timestamp: 1705314655, // 2024-01-15T10:30:55Z
            level: "ERROR".to_string(),
            message: "device mapper: multipath: queue_if_no_path enabled".to_string(),
        },
        LogEntry {
            timestamp: 1705314656, // 2024-01-15T10:30:56Z
            level: "INFO".to_string(),
            message: "I/O scheduler mq-deadline registered".to_string(),
        },
        LogEntry {
            timestamp: 1705314657, // 2024-01-15T10:30:57Z
            level: "WARN".to_string(),
            message: "device sda: optimal transfer size 1048576 bytes".to_string(),
        },
        LogEntry {
            timestamp: 1705314658, // 2024-01-15T10:30:58Z
            level: "INFO".to_string(),
            message: "block device sdb: partition table created".to_string(),
        },
        LogEntry {
            timestamp: 1705314659, // 2024-01-15T10:30:59Z
            level: "ERROR".to_string(),
            message: "XFS (sdb1): Ending clean mount".to_string(),
        },
        LogEntry {
            timestamp: 1705314660, // 2024-01-15T10:31:00Z
            level: "INFO".to_string(),
            message: "LSM: Security Framework initialized".to_string(),
        },
    ]
}

/// Generate mock container logs based on container type
pub fn generate_container_logs(container_name: &str, image: &str) -> Vec<LogEntry> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    // Generate logs based on container type
    let logs = if image.contains("nginx") {
        vec![
            format!("{}: starting nginx", container_name),
            format!("{}: nginx configuration loaded", container_name),
            format!("{}: listening on port 80", container_name),
            format!("{}: GET /health 200", container_name),
            format!("{}: worker process started", container_name),
            format!("{}: GET /api/status 200", container_name),
            format!("{}: access log rotated", container_name),
            format!("{}: POST /api/data 201", container_name),
        ]
    } else if image.contains("postgres") {
        vec![
            format!("{}: PostgreSQL init process complete", container_name),
            format!("{}: database system is ready to accept connections", container_name),
            format!("{}: listening on port 5432", container_name),
            format!("{}: checkpoint starting", container_name),
            format!("{}: connection received: host=app port=34567", container_name),
            format!("{}: SELECT query executed in 2.4ms", container_name),
            format!("{}: transaction committed", container_name),
        ]
    } else if image.contains("redis") {
        vec![
            format!("{}: Redis server started", container_name),
            format!("{}: ready to accept connections", container_name),
            format!("{}: DB loaded from disk", container_name),
            format!("{}: RDB: 0 keys in 0 databases", container_name),
            format!("{}: client connected", container_name),
            format!("{}: SET key executed", container_name),
        ]
    } else if image.contains("node") {
        vec![
            format!("{}: npm start", container_name),
            format!("{}: server listening on port 3000", container_name),
            format!("{}: connected to database", container_name),
            format!("{}: middleware loaded", container_name),
            format!("{}: API routes configured", container_name),
            format!("{}: user authentication successful", container_name),
        ]
    } else if image.contains("python") {
        vec![
            format!("{}: starting Python application", container_name),
            format!("{}: loading configuration", container_name),
            format!("{}: connecting to data source", container_name),
            format!("{}: ETL pipeline initialized", container_name),
            format!("{}: processing batch job", container_name),
            format!("{}: data transformation complete", container_name),
        ]
    } else if image.contains("jupyter") {
        vec![
            format!("{}: starting Jupyter server", container_name),
            format!("{}: notebook server is running", container_name),
            format!("{}: kernel started", container_name),
            format!("{}: loading ML libraries", container_name),
        ]
    } else if image.contains("tensorflow") {
        vec![
            format!("{}: TensorFlow serving started", container_name),
            format!("{}: model loaded successfully", container_name),
            format!("{}: serving on port 8501", container_name),
            format!("{}: prediction request processed", container_name),
        ]
    } else {
        vec![
            format!("{}: container started", container_name),
            format!("{}: application initialized", container_name),
            format!("{}: ready to serve requests", container_name),
            format!("{}: health check passed", container_name),
            format!("{}: processing request", container_name),
        ]
    };

    let logs_len = logs.len();
    logs.into_iter().enumerate().map(|(i, message)| {
        LogEntry {
            timestamp: now - (logs_len - i) as u64 * 10, // Space logs 10 seconds apart
            level: match i % 5 {
                0 => "INFO".to_string(),
                4 => "WARN".to_string(),
                _ => "INFO".to_string(),
            },
            message,
        }
    }).collect()
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
            assert!(!interface.mac_address.is_empty());
            assert!(!interface.ipv6_address.is_empty());
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