use log::info;
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

pub mod image;
mod vm_config;

#[derive(Debug)]
pub enum Error {
    AlreadyExists,
    NotFound,
    SocketFailure(std::io::Error),
    InvalidInput(TryFromIntError),
    CHCommandFailure(std::io::Error),
    CHApiFailure(api_client::Error),
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

        let cfg = json!(vm_config::Config {
            cpus: vm_config::CpusConfig {
                boot_vcpus: cpu,
                max_vcpus: cpu,
            },
            memory: vm_config::MemoryConfig {
                size: memory,
                shared: true,
                ..Default::default()
            },
            payload: vm_config::PayloadConfig {
                // TODO: fix hardcoded path
                firmware: Some(PathBuf::from("/lib/firmware/hypervisor-fw")),
                ..Default::default()
            },
            disks: Some(vec![vm_config::DiskConfig {
                path: Some(root_fs),
                readonly: false,
                direct: false,
            },]),
            serial: vm_config::ConsoleConfig {
                // mode: vm_config::ConsoleOutputMode::Tty,
                ..Default::default()
            },

            ..Default::default()
        });

        let mut socket = UnixStream::connect(id.to_string()).map_err(Error::SocketFailure)?;
        let response = api_client::simple_api_full_command_and_response(
            &mut socket,
            "PUT",
            "vm.create",
            Some(&cfg.to_string()),
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
}
