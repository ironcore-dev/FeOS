use std::path::PathBuf;

use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub cpus: CpusConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    pub payload: PayloadConfig,
    pub disks: Option<Vec<DiskConfig>>,
    #[serde(default = "default_serial")]
    pub serial: ConsoleConfig,
    #[serde(default = "default_console")]
    pub console: ConsoleConfig,
    pub devices: Option<Vec<DeviceConfig>>,
    pub user_devices: Option<Vec<UserDeviceConfig>>,
    pub debug_console: DebugConsoleConfig,
}

pub fn default_serial() -> ConsoleConfig {
    ConsoleConfig {
        file: None,
        mode: ConsoleOutputMode::Null,
        iommu: false,
        socket: None,
    }
}

pub fn default_console() -> ConsoleConfig {
    ConsoleConfig {
        file: None,
        mode: ConsoleOutputMode::Tty,
        iommu: false,
        socket: None,
    }
}

#[derive(Serialize, Deserialize)]
pub struct CpusConfig {
    pub boot_vcpus: u8,
    pub max_vcpus: u8,
}

pub const DEFAULT_VCPUS: u8 = 1;
impl Default for CpusConfig {
    fn default() -> Self {
        CpusConfig {
            boot_vcpus: DEFAULT_VCPUS,
            max_vcpus: DEFAULT_VCPUS,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct MemoryConfig {
    pub size: u64,
    #[serde(default)]
    pub mergeable: bool,
    #[serde(default)]
    pub shared: bool,
    #[serde(default)]
    pub hugepages: bool,
    #[serde(default)]
    pub hugepage_size: Option<u64>,
}

pub const DEFAULT_MEMORY_MB: u64 = 512;
impl Default for MemoryConfig {
    fn default() -> Self {
        MemoryConfig {
            size: DEFAULT_MEMORY_MB << 20,
            mergeable: false,
            shared: false,
            hugepages: false,
            hugepage_size: None,
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
pub struct PayloadConfig {
    #[serde(default)]
    pub firmware: Option<PathBuf>,
    #[serde(default)]
    pub kernel: Option<PathBuf>,
    #[serde(default)]
    pub cmdline: Option<String>,
    #[serde(default)]
    pub initramfs: Option<PathBuf>,
}

#[derive(Deserialize, Serialize)]
pub struct DiskConfig {
    pub path: Option<PathBuf>,
    #[serde(default)]
    pub readonly: bool,
    #[serde(default)]
    pub direct: bool,
}

#[derive(Serialize, Deserialize)]
pub struct ConsoleConfig {
    #[serde(default = "default_consoleconfig_file")]
    pub file: Option<PathBuf>,
    pub mode: ConsoleOutputMode,
    #[serde(default)]
    pub iommu: bool,
    pub socket: Option<PathBuf>,
}

impl Default for ConsoleConfig {
    fn default() -> Self {
        Self {
            file: None,
            mode: ConsoleOutputMode::Off,
            iommu: false, 
            socket: None,
        }
    }
}

pub fn default_consoleconfig_file() -> Option<PathBuf> {
    None
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct DebugConsoleConfig {
    #[serde(default)]
    pub file: Option<PathBuf>,
    pub mode: ConsoleOutputMode,
    /// Optionally dedicated I/O-port, if the default port should not be used.
    pub iobase: Option<u16>,
}

#[cfg(target_arch = "x86_64")]
impl Default for DebugConsoleConfig {
    fn default() -> Self {
        Self {
            file: None,
            mode: ConsoleOutputMode::Off,
            iobase: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum ConsoleOutputMode {
    Off,
    Pty,
    Tty,
    File,
    Socket,
    Null,
}

#[derive(Deserialize, Serialize)]
pub struct UserDeviceConfig {
    pub socket: PathBuf,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub pci_segment: u16,
}

#[derive(Deserialize, Serialize)]
pub struct DeviceConfig {
    pub path: PathBuf,
    #[serde(default)]
    pub iommu: bool,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub pci_segment: u16,
    #[serde(default)]
    pub x_nv_gpudirect_clique: Option<u8>,
}