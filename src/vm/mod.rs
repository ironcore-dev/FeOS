use log::{error, info};
use serde_json::json;
use std::{
    collections::HashMap,
    num::TryFromIntError,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::Mutex,
    thread::sleep,
    time,
};
use uuid::Uuid;
use vmm::vm_config;

use vmm::config::{
    ConsoleConfig, ConsoleOutputMode, CpusConfig, DiskConfig, MemoryConfig, NetConfig,
    PayloadConfig, PlatformConfig, VsockConfig,
};

use net_util::MacAddr;

use crate::network;
use pelite::pe64::{Pe, PeFile};
use std::fs;
use std::fs::{create_dir_all, File};
use std::io::{self, Write};
use std::net::Ipv6Addr;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::task::JoinHandle;

pub mod config;
pub mod image;

#[derive(Debug)]
pub enum Error {
    AlreadyExists,
    NotFound,
    SocketFailure(std::io::Error),
    InvalidInput(TryFromIntError),
    CHCommandFailure(std::io::Error),
    CHApiFailure(api_client::Error),
    Failed,
}

pub enum BootMode {
    FirmwareBoot(FirmwareBootMode),
    KernelBoot(KernelBootMode),
}

pub struct FirmwareBootMode {
    pub root_fs: PathBuf,
}

pub struct KernelBootMode {
    pub kernel: PathBuf,
    pub initramfs: PathBuf,
    pub cmdline: String,
}

pub enum NetworkMode {
    PCIAddress(String),
    MACAddress(String),
    TAPDeviceName(String),
}

#[derive(Debug)]
struct VmInfo {
    pub child: Child,
}

#[derive(Debug)]
pub struct Manager {
    pub ch_bin: String,
    vms: Mutex<HashMap<Uuid, VmInfo>>,
}

impl Default for Manager {
    fn default() -> Self {
        Self {
            ch_bin: String::default(),
            vms: Mutex::new(HashMap::new()),
        }
    }
}

impl Manager {
    pub fn new(ch_bin: String) -> Self {
        Self {
            ch_bin,
            vms: Mutex::new(HashMap::new()),
        }
    }

    pub fn init_vmm(&self, id: Uuid, wait: bool) -> Result<(), Error> {
        let mut vms = self.vms.lock().unwrap();
        if vms.contains_key(&id) {
            return Err(Error::AlreadyExists);
        }

        let vm = Command::new(self.ch_bin.clone())
            .arg("--api-socket")
            .arg(id.to_string())
            .spawn()
            .map_err(Error::CHCommandFailure)?;

        vms.insert(id, VmInfo { child: vm });

        info!("created vmm with id: {}", id.to_string());

        if !wait {
            return Ok(());
        }

        let tries = 3;
        for i in 0..tries {
            info!(
                "waiting until socket is open: vmm: {}, tries: {}/{}",
                id.to_string(),
                i + 1,
                tries
            );
            if Path::new(&id.to_string()).exists() {
                info!("socket open: vmm: {}", id.to_string());
                return Ok(());
            }

            sleep(time::Duration::from_millis(250));
        }
        info!("retries exceeded: vmm: {}", id.to_string());

        Ok(())
    }

    pub fn create_vm(
        &self,
        id: Uuid,
        cpu: u32,
        memory: u64,
        boot_mode: BootMode,
        ignition: Option<String>,
    ) -> Result<(), Error> {
        let vms = self.vms.lock().unwrap();
        if !vms.contains_key(&id) {
            return Err(Error::NotFound);
        }

        let cpu = u8::try_from(cpu).map_err(Error::InvalidInput)?;

        let mut vm_config = config::default_vm_cfg();
        vm_config.cpus = CpusConfig {
            boot_vcpus: cpu,
            max_vcpus: cpu,
            ..Default::default()
        };
        vm_config.memory = MemoryConfig {
            size: memory,
            shared: true,
            ..Default::default()
        };

        let mut disks = vec![];
        match boot_mode {
            BootMode::FirmwareBoot(firmware_boot) => {
                vm_config.payload = Some(PayloadConfig {
                    firmware: Some(PathBuf::from("/usr/share/cloud-hypervisor/hypervisor-fw")),
                    kernel: None,
                    cmdline: None,
                    initramfs: None,
                });
                disks.push(DiskConfig {
                    path: Some(firmware_boot.root_fs),
                    ..config::default_disk_cfg()
                });
            }
            BootMode::KernelBoot(kernel_boot) => {
                vm_config.payload = Some(PayloadConfig {
                    kernel: Some(kernel_boot.kernel),
                    cmdline: Some(kernel_boot.cmdline.clone()),
                    initramfs: Some(kernel_boot.initramfs),
                    firmware: None,
                });
            }
        };

        vm_config.vsock = Some(VsockConfig {
            cid: 33,
            socket: PathBuf::from(format!("vsock{}.sock", network::Manager::device_name(&id))),
            id: None,
            iommu: false,
            pci_segment: 0,
        });
        // vm_config.net = Some(vec![NetConfig {
        //     tap: Some(Manager::vm_tap_name(&id)),
        //     ..config::_default_net_cfg()
        // }]);

        if disks.len() != 0 {
            vm_config.disks = Some(disks);
        }

        vm_config.serial = ConsoleConfig {
            socket: Some(PathBuf::from(id.to_string() + ".console")),
            mode: ConsoleOutputMode::Socket,
            file: None,
            iommu: false,
        };

        if let Some(ignition) = ignition {
            info!("configured ignition for vm {}", id.to_string());
            vm_config.platform = Some(PlatformConfig {
                num_pci_segments: vm_config::default_platformconfig_num_pci_segments(),
                iommu_segments: None,
                serial_number: None,
                uuid: None,
                oem_strings: Some(vec![ignition]),
            })
        }

        let vm_config = json!(vm_config);

        let mut socket = UnixStream::connect(id.to_string()).map_err(Error::SocketFailure)?;
        let response = api_client::simple_api_full_command_and_response(
            &mut socket,
            "PUT",
            "vm.create",
            Some(&vm_config.to_string()),
        )
        .map_err(Error::CHApiFailure)?;
        if let Some(response) = response {
            info!("create vm: id {}, response: {}", id.to_string(), response)
        }

        info!("created vm with id: {}", id.to_string());
        Ok(())
    }

    pub fn boot_vm(&self, id: Uuid) -> Result<(), Error> {
        let vms = self.vms.lock().unwrap();
        if !vms.contains_key(&id) {
            return Err(Error::NotFound);
        }

        let mut socket = UnixStream::connect(id.to_string()).map_err(Error::SocketFailure)?;
        let response =
            api_client::simple_api_full_command_and_response(&mut socket, "PUT", "vm.boot", None)
                .map_err(Error::CHApiFailure)?;
        if response.is_some() {
            info!(
                "boot vm: id {}, response: {}",
                id.to_string(),
                response.unwrap()
            )
        }

        info!("booted vm with id: {}", id.to_string());
        Ok(())
    }

    pub fn get_vm_console_path(&self, id: Uuid) -> Result<String, Error> {
        let vms = self.vms.lock().unwrap();
        if !vms.contains_key(&id) {
            return Err(Error::NotFound);
        }

        let socket_path = id.to_string() + ".console";
        if !Path::new(&socket_path).exists() {
            return Err(Error::NotFound);
        }

        Ok(socket_path)
    }

    pub fn add_net_device(&self, id: Uuid, config: NetworkMode) -> Result<(), Error> {
        let vms = self.vms.lock().unwrap();
        if !vms.contains_key(&id) {
            return Err(Error::NotFound);
        }

        let mut socket = UnixStream::connect(id.to_string()).map_err(Error::SocketFailure)?;

        let mut request: (String, String);

        match config {
            NetworkMode::PCIAddress(pci) => {
                let path = PathBuf::from(format!("/sys/bus/pci/devices/{}/", pci));
                info!("check if path exists {}", path.display());
                if !path.exists() {
                    info!("The path {} does not exist.", path.display());
                    return Err(Error::NotFound);
                }

                info!("add device");
                request = (
                    "vm.add-device".to_string(),
                    json!(vm_config::DeviceConfig {
                        path,
                        iommu: false,
                        id: None,
                        pci_segment: 0,
                        x_nv_gpudirect_clique: None,
                    })
                    .to_string(),
                );
            }
            NetworkMode::MACAddress(mac) => {
                let mac = MacAddr::parse_str(&mac).map_err(Error::CHCommandFailure)?;
                let mut net_config = config::_default_net_cfg();
                net_config.host_mac = Some(mac);
                request = ("vm.add-net".to_string(), json!(net_config).to_string());
            }
            NetworkMode::TAPDeviceName(tap) => {
                let mut net_config = config::_default_net_cfg();
                net_config.tap = Some(tap);
                request = ("vm.add-net".to_string(), json!(net_config).to_string());
            }
        }

        let response = api_client::simple_api_full_command_and_response(
            &mut socket,
            "PUT",
            &request.0,
            Some(&request.1),
        )
        .map_err(Error::CHApiFailure)?;
        if response.is_some() {
            info!(
                "{} to vm: id {}, response: {}",
                request.0,
                id.to_string(),
                response.unwrap()
            )
        }

        return Ok(());
    }

    pub fn ping_vmm(&self, id: Uuid) -> Result<(), Error> {
        let vms = self.vms.lock().unwrap();
        if !vms.contains_key(&id) {
            return Err(Error::NotFound);
        }

        let mut socket = UnixStream::connect(id.to_string()).map_err(Error::SocketFailure)?;
        let response =
            api_client::simple_api_full_command_and_response(&mut socket, "GET", "vmm.ping", None)
                .map_err(Error::CHApiFailure)?;
        if response.is_some() {
            info!(
                "ping vmm: id {}, response: {}",
                id.to_string(),
                response.unwrap()
            );
        }

        Ok(())
    }

    pub fn get_vm(&self, id: Uuid) -> Result<String, Error> {
        let vms = self.vms.lock().unwrap();
        if !vms.contains_key(&id) {
            return Err(Error::NotFound);
        }

        let mut socket = UnixStream::connect(id.to_string()).map_err(Error::SocketFailure)?;
        let response =
            api_client::simple_api_full_command_and_response(&mut socket, "GET", "vm.info", None)
                .map_err(Error::CHApiFailure)?;

        if let Some(x) = response {
            info!("get vm: id {}, response: {}", id.to_string(), x);
            return Ok(x);
        }
        Ok(String::new())
    }

    pub fn shutdown_vm(&self, id: Uuid) -> Result<String, Error> {
        let mut vms = self.vms.lock().unwrap();
        let vm_info = match vms.get_mut(&id) {
            Some(info) => info,
            None => return Err(Error::NotFound),
        };

        let mut socket = UnixStream::connect(id.to_string()).map_err(Error::SocketFailure)?;
        let response = api_client::simple_api_full_command_and_response(
            &mut socket,
            "PUT",
            "vm.shutdown",
            None,
        )
        .map_err(Error::CHApiFailure)?;

        if let Some(x) = &response {
            info!("shutdown vm: id {}, response: {}", id, x);
        }

        if let Err(e) = vm_info.child.kill() {
            error!("Failed to kill child process for VM {}: {}", id, e);
        } else {
            info!("Sent kill signal to VM {}", id);
        }

        match vm_info.child.wait() {
            Ok(status) => info!("VM {} exited with status {}", id, status),
            Err(e) => error!("Failed to wait for VM {}: {}", id, e),
        }

        vms.remove(&id);

        Ok(String::new())
    }
}

