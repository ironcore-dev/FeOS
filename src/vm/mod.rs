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

#[derive(Error, Debug)]
pub enum ExtractionError {
    #[error("failed to read PE/COFF image")]
    ReadImage(#[from] pelite::Error),
    #[error("failed to write file")]
    WriteFile(#[from] io::Error),
    #[error("section not found in PE/COFF image")]
    SectionNotFound,
}

#[derive(Debug)]
pub enum Error {
    AlreadyExists,
    NotFound,
    SocketFailure(std::io::Error),
    InvalidInput(TryFromIntError),
    CHCommandFailure(std::io::Error),
    CHApiFailure(api_client::Error),
    ExtractionFailure(ExtractionError),
    Failed,
}

impl From<ExtractionError> for Error {
    fn from(err: ExtractionError) -> Self {
        Error::ExtractionFailure(err)
    }
}
#[derive(Debug)]
pub struct VmInfo {
    pub child: Child,
    pub radv_handle: Option<Arc<JoinHandle<()>>>,
    pub dhcpv6_handle: Option<Arc<JoinHandle<()>>>,
}

#[derive(Debug)]
pub struct Manager {
    pub ch_bin: String,
    vms: Mutex<HashMap<Uuid, VmInfo>>,
    pub is_nested: bool,
    pub test_mode: bool,
    ipv6_address: Ipv6Addr,
    prefix_length: u8,
    prefix_count: AtomicU16,
}

impl Default for Manager {
    fn default() -> Self {
        Self {
            ch_bin: String::default(),
            vms: Mutex::new(HashMap::new()),
            is_nested: false,
            test_mode: false,
            ipv6_address: Ipv6Addr::UNSPECIFIED,
            prefix_length: 64,
            prefix_count: AtomicU16::new(1),
        }
    }
}

impl Manager {
    pub fn new(
        ch_bin: String,
        is_nested: bool,
        test_mode: bool,
        ipv6_address: Ipv6Addr,
        prefix_length: u8,
    ) -> Self {
        Self {
            ch_bin,
            vms: Mutex::new(HashMap::new()),
            is_nested,
            test_mode,
            ipv6_address,
            prefix_length,
            prefix_count: AtomicU16::new(1),
        }
    }

    pub fn vm_tap_name(id: &Uuid) -> String {
        format!("vmtap{}", &id.to_string()[..8])
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

        vms.insert(
            id,
            VmInfo {
                child: vm,
                radv_handle: None,
                dhcpv6_handle: None,
            },
        );

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
        root_fs: PathBuf,
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

        if self.test_mode {
            // Fetched OCI image for FeOS doesnt boot with hypervisor-fw
            // For local development and integration tests, use the extracted UKI image
            let (kernel_path, cmdline_path, initramfs_path) = extract_uki_image(&root_fs)?;
            let mut cmdline_contents =
                fs::read_to_string(&cmdline_path).map_err(Error::SocketFailure)?;
            cmdline_contents = cmdline_contents
                .chars()
                .filter(|c| c.is_ascii_graphic() || c.is_whitespace())
                .collect();
            vm_config.payload = Some(PayloadConfig {
                kernel: Some(kernel_path),
                cmdline: Some(cmdline_contents.clone()),
                initramfs: Some(initramfs_path),
                firmware: None,
            });
        } else {
            vm_config.payload = Some(PayloadConfig {
                kernel: None,
                cmdline: None,
                initramfs: None,
                firmware: Some(PathBuf::from("/usr/share/cloud-hypervisor/hypervisor-fw")),
            });
        }

        vm_config.vsock = Some(VsockConfig {
            cid: 33,
            socket: PathBuf::from(format!("vsock{}.sock", Manager::vm_tap_name(&id))),
            id: None,
            iommu: false,
            pci_segment: 0,
        });
        vm_config.net = Some(vec![NetConfig {
            tap: Some(Manager::vm_tap_name(&id)),
            ..config::_default_net_cfg()
        }]);
        vm_config.disks = Some(vec![DiskConfig {
            path: Some(root_fs),
            ..config::default_disk_cfg()
        }]);
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

    pub fn get_radv_handle(&self, id: Uuid) -> Result<Option<Arc<JoinHandle<()>>>, Error> {
        let vms = self.vms.lock().unwrap();
        if let Some(vm_info) = vms.get(&id) {
            Ok(vm_info.radv_handle.clone())
        } else {
            Err(Error::NotFound)
        }
    }

    pub fn set_radv_handle(&self, id: Uuid, handle: JoinHandle<()>) -> Result<(), Error> {
        let mut vms = self.vms.lock().unwrap();
        if let Some(vm_info) = vms.get_mut(&id) {
            vm_info.radv_handle = Some(Arc::new(handle));
            Ok(())
        } else {
            Err(Error::NotFound)
        }
    }

    pub fn get_dhcpv6_handle(&self, id: Uuid) -> Result<Option<Arc<JoinHandle<()>>>, Error> {
        let vms = self.vms.lock().unwrap();
        if let Some(vm_info) = vms.get(&id) {
            Ok(vm_info.dhcpv6_handle.clone())
        } else {
            Err(Error::NotFound)
        }
    }

    pub fn set_dhcpv6_handle(&self, id: Uuid, handle: JoinHandle<()>) -> Result<(), Error> {
        let mut vms = self.vms.lock().unwrap();
        if let Some(vm_info) = vms.get_mut(&id) {
            vm_info.dhcpv6_handle = Some(Arc::new(handle));
            Ok(())
        } else {
            Err(Error::NotFound)
        }
    }

    pub fn get_ipv6_info(&self) -> (Ipv6Addr, u8, u16) {
        let new_count = self.prefix_count.fetch_add(1, Ordering::SeqCst) + 1;

        (self.ipv6_address, self.prefix_length, new_count)
    }

    pub fn _add_net_device(
        &self,
        id: Uuid,
        mac: Option<String>,
        pci: Option<String>,
    ) -> Result<(), Error> {
        let vms = self.vms.lock().unwrap();
        if !vms.contains_key(&id) {
            return Err(Error::NotFound);
        }

        let mut socket = UnixStream::connect(id.to_string()).map_err(Error::SocketFailure)?;

        if let Some(pci) = pci {
            // Check if the path exists
            let path = PathBuf::from(format!("/sys/bus/pci/devices/{}/", pci));
            info!("check if path exists {}", path.display());
            if !path.exists() {
                info!("The path {} does not exist.", path.display());
                return Err(Error::NotFound);
            }

            info!("add device");
            let device_config = json!(vm_config::DeviceConfig {
                path,
                iommu: false,
                id: None,
                pci_segment: 0,
                x_nv_gpudirect_clique: None,
            });

            let response = api_client::simple_api_full_command_and_response(
                &mut socket,
                "PUT",
                "vm.add-device",
                Some(&device_config.to_string()),
            )
            .map_err(Error::CHApiFailure)?;
            if response.is_some() {
                info!(
                    "add-device to vm: id {}, response: {}",
                    id.to_string(),
                    response.unwrap()
                )
            }

            return Ok(());
        }

        if let Some(mac) = mac {
            let mac = MacAddr::parse_str(&mac).map_err(Error::CHCommandFailure)?;

            let mut net_config = config::_default_net_cfg();
            net_config.host_mac = Some(mac);
            let net_config = json!(net_config);

            let response = api_client::simple_api_full_command_and_response(
                &mut socket,
                "PUT",
                "vm.add-net",
                Some(&net_config.to_string()),
            )
            .map_err(Error::CHApiFailure)?;
            if response.is_some() {
                info!(
                    "add_net_device to vm: id {}, response: {}",
                    id.to_string(),
                    response.unwrap()
                )
            }

            return Ok(());
        }

        Err(Error::Failed)
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

fn extract_section(
    pe: &PeFile,
    section_name: &str,
    output_path: &Path,
) -> Result<(), ExtractionError> {
    let section = pe
        .section_headers()
        .iter()
        .find(|header| header.Name.starts_with(section_name.as_bytes()))
        .ok_or(ExtractionError::SectionNotFound)?;

    let rva: u32 = section.VirtualAddress;
    let data = pe
        .slice(rva, section.VirtualSize as usize, 1)
        .map_err(|_| ExtractionError::SectionNotFound)?;

    let mut file = File::create(output_path)?;
    file.write_all(data)?;
    Ok(())
}

fn extract_uki_image(uki_path: &Path) -> Result<(PathBuf, PathBuf, PathBuf), ExtractionError> {
    let buffer = std::fs::read(uki_path)?;
    let pe = PeFile::from_bytes(&buffer)?;

    let extract_dir = std::path::PathBuf::from("extracted");
    create_dir_all(&extract_dir)?;

    let kernel_path = extract_dir.join("kernel");
    let cmdline_path = extract_dir.join("cmdline");
    let initramfs_path = extract_dir.join("initramfs");

    extract_section(&pe, ".linux", &kernel_path)?;
    extract_section(&pe, ".cmdline", &cmdline_path)?;
    extract_section(&pe, ".initrd", &initramfs_path)?;

    Ok((kernel_path, cmdline_path, initramfs_path))
}
