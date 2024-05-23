use vmm::config::{
    default_netconfig_ip, default_netconfig_mac, default_netconfig_mask, DebugConsoleConfig,
    RngConfig, VhostMode,
};
use vmm::{
    config::{
        ConsoleConfig, ConsoleOutputMode, CpusConfig, DiskConfig, MemoryConfig, PayloadConfig,
    },
    vm_config::{NetConfig, VmConfig},
};

pub fn default_vm_cfg() -> VmConfig {
    VmConfig {
        cpus: CpusConfig {
            ..Default::default()
        },
        memory: MemoryConfig {
            ..Default::default()
        },
        payload: Some(PayloadConfig {
            kernel: None,
            cmdline: None,
            initramfs: None,
            firmware: None,
        }),
        disks: None,
        serial: ConsoleConfig {
            socket: None,
            mode: ConsoleOutputMode::Off,
            file: None,
            iommu: false,
        },

        rate_limit_groups: None,
        net: None,
        rng: RngConfig {
            ..Default::default()
        },
        balloon: None,
        fs: None,
        pmem: None,
        console: ConsoleConfig {
            file: None,
            mode: ConsoleOutputMode::Off,
            iommu: false,
            socket: None,
        },
        debug_console: DebugConsoleConfig {
            file: None,
            mode: ConsoleOutputMode::Off,
            iobase: None,
        },
        devices: None,
        user_devices: None,
        vdpa: None,
        vsock: None,
        pvpanic: false,
        iommu: false,
        sgx_epc: None,
        numa: None,
        watchdog: false,
        pci_segments: None,
        platform: None,
        tpm: None,
        preserved_fds: None,
    }
}

pub fn default_disk_cfg() -> DiskConfig {
    DiskConfig {
        path: None,
        readonly: false,
        direct: false,
        iommu: false,
        num_queues: 1,
        queue_size: 128,
        vhost_user: false,
        vhost_socket: None,
        id: None,
        disable_io_uring: false,
        disable_aio: false,
        rate_limit_group: None,
        rate_limiter_config: None,
        pci_segment: 0,
        serial: None,
        queue_affinity: None,
    }
}

pub fn _default_net_cfg() -> NetConfig {
    NetConfig {
        tap: None,
        ip: default_netconfig_ip(),
        mask: default_netconfig_mask(),
        mac: default_netconfig_mac(),
        host_mac: None,
        mtu: None,
        iommu: false,
        num_queues: 2,
        queue_size: 256,
        vhost_user: false,
        vhost_socket: None,
        vhost_mode: VhostMode::Client,
        id: None,
        fds: None,
        rate_limiter_config: None,
        pci_segment: 0,
        offload_tso: true,
        offload_ufo: true,
        offload_csum: true,
    }
}
