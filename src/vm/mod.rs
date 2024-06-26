use log::info;
use serde_json::json;
use std::io::{BufRead, BufReader, Write};
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
use std::{fs, io};
use uuid::Uuid;
use vmm::vm_config;

use vmm::config::{
    ConsoleConfig, ConsoleOutputMode, CpusConfig, DiskConfig, MemoryConfig, PayloadConfig,
};

use net_util::MacAddr;

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

#[derive(Debug)]
pub struct Manager {
    pub ch_bin: String,
    vms: Mutex<HashMap<Uuid, Child>>,
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

        vms.insert(id, vm);

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
        vm_config.payload = Some(PayloadConfig {
            kernel: None,
            cmdline: None,
            initramfs: None,
            // TODO: fix hardcoded path
            firmware: Some(PathBuf::from("/usr/share/cloud-hypervisor/hypervisor-fw")),
        });
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
        let vm_config = json!(vm_config);

        let mut socket = UnixStream::connect(id.to_string()).map_err(Error::SocketFailure)?;
        let response = api_client::simple_api_full_command_and_response(
            &mut socket,
            "PUT",
            "vm.create",
            Some(&vm_config.to_string()),
        )
        .map_err(Error::CHApiFailure)?;
        if response.is_some() {
            info!(
                "create vm: id {}, response: {}",
                id.to_string(),
                response.unwrap()
            )
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

    pub fn console(&self, id: Uuid) -> Result<(), Error> {
        let vms = self.vms.lock().unwrap();
        if !vms.contains_key(&id) {
            return Err(Error::NotFound);
        }

        let socket_path = id.to_string() + ".console";
        if !Path::new(&socket_path).exists() {
            return Err(Error::NotFound);
        }

        // TODO: stream over HTTP
        std::thread::spawn(move || {
            match UnixStream::connect(socket_path).map_err(Error::SocketFailure) {
                Ok(stream) => {
                    let mut buffer = Vec::new();

                    let mut reader = BufReader::new(stream);
                    loop {
                        match reader.read_until(b'\n', &mut buffer) {
                            Ok(0) => {
                                // Connection was closed
                                println!("Connection closed");
                                break;
                            }
                            Ok(_) => {
                                if let Ok(line) = String::from_utf8(buffer.clone()) {
                                    println!("Received: {}", line);
                                } else {
                                    eprintln!("Received invalid UTF-8 data");
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to read from stream: {}", e);
                                break;
                            }
                        }
                    }
                }
                Err(e) => eprintln!("Failed to accept connection: {:?}", e),
            }
        });

        Ok(())
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
            let vendor = self.get_vendor(&pci).unwrap_or_default();
            let device = self.get_device(&pci).unwrap_or_default();
            info!("{} - {}", vendor, device);

            self.bind(&vendor, &device).map_err(Error::SocketFailure)?;

            let device_config = json!(vm_config::DeviceConfig {
                path: PathBuf::from(format!("/sys/bus/pci/devices/{}/", pci)),
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

    fn get_vendor(&self, mac: &str) -> Option<String> {
        let path = format!("/sys/bus/pci/devices/{}/vendor", mac);
        if let Ok(vendor) = fs::read_to_string(path) {
            return Some(vendor[2..].trim().to_string());
        } else {
            None
        }
    }

    fn get_device(&self, mac: &str) -> Option<String> {
        let path: String = format!("/sys/bus/pci/devices/{}/device", mac);
        if let Ok(device) = fs::read_to_string(path) {
            return Some(device[2..].trim().to_string());
        } else {
            None
        }
    }

    fn bind(&self, vendor: &str, device: &str) -> Result<(), io::Error> {
        let path = Path::new("/sys/bus/pci/drivers/vfio-pci/new_id");

        let content = format!("{} {}", vendor, device);

        let mut file = fs::OpenOptions::new().write(true).open(path)?;

        // Write the content to the file
        file.write_all(content.as_bytes())?;

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
}
