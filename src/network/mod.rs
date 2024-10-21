use log::{debug, error, info};
use rtnetlink::new_connection;
use std::collections::HashMap;
use std::net::Ipv6Addr;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use tokio::spawn;
use tokio::task::JoinHandle;
use uuid::Uuid;

pub mod dhcpv6;
pub mod radv;
mod utils;

use crate::network::dhcpv6::{
    add_ipv6_route, add_to_ipv6, adjust_base_ip, run_dhcpv6_server, IpRange,
};
use radv::start_radv_server;
pub use utils::configure_network_devices;
pub use utils::configure_sriov;

#[derive(Debug)]
pub enum Error {
    Failed,
    AlreadyExists,
}

#[derive(Debug)]
pub struct Manager {
    ipv6_address: Ipv6Addr,
    prefix_length: u8,
    prefix_count: AtomicU16,
    instances: Mutex<HashMap<Uuid, Handles>>,
}

#[derive(Debug)]
struct Handles {
    pub radv_handle: Arc<JoinHandle<()>>,
    pub dhcpv6_handle: Arc<JoinHandle<()>>,
}

impl Default for Manager {
    fn default() -> Self {
        Self {
            ipv6_address: Ipv6Addr::UNSPECIFIED,
            prefix_length: 64,
            prefix_count: AtomicU16::new(1),
            instances: Mutex::new(HashMap::new()),
        }
    }
}

impl Manager {
    pub fn new(ipv6_address: Ipv6Addr, prefix_length: u8) -> Self {
        Self {
            ipv6_address,
            prefix_length,
            prefix_count: AtomicU16::new(1),
            instances: Mutex::new(HashMap::new()),
        }
    }

    pub fn vm_tap_name(id: &Uuid) -> String {
        format!("vmtap{}", &id.to_string()[..8])
    }

    fn exists(&self, id: Uuid) -> Result<(), Error> {
        let instances = self.instances.lock().unwrap();
        if instances.contains_key(&id) {
            return Err(Error::AlreadyExists);
        }
        Ok(())
    }

    pub async fn stop_dhcp(&self, id: Uuid) -> Result<(), Error> {
        let mut instances = self.instances.lock().unwrap();
        if let Some(handle) = instances.remove(&id) {
            handle.radv_handle.abort();
            handle.dhcpv6_handle.abort();
        }

        Ok(())
    }
    pub async fn start_dhcp(&self, id: Uuid) -> Result<(), Error> {
        self.exists(id)?;

        let interface_name = Manager::device_name(&id);

        info!("created tap device: {}", &interface_name);

        let (base_ip, prefix_length, prefix_count) = self.get_ipv6_info();
        let adjusted_base_ip = adjust_base_ip(base_ip, prefix_length, prefix_count);
        let new_prefix_length = prefix_length + 16;

        let ip_start = add_to_ipv6(adjusted_base_ip, new_prefix_length, 100);
        let ip_end = add_to_ipv6(adjusted_base_ip, new_prefix_length, 200);
        debug!("IP Range: {} - {}", ip_start, ip_end);

        let ip_range = IpRange {
            start: ip_start,
            end: ip_end,
        };

        let radv_handle = {
            let interface_name = interface_name.clone();
            spawn(async move {
                if let Err(e) = start_radv_server(
                    interface_name.to_string(),
                    adjusted_base_ip,
                    new_prefix_length,
                )
                .await
                {
                    error!("Failed to start RADV server: {}", e);
                }
            })
        };

        let dhcpv6_handle = {
            let interface_name = interface_name.clone();
            spawn(async move {
                if let Err(e) = run_dhcpv6_server(interface_name.to_string(), ip_range).await {
                    error!("Failed to run DHCPv6 server: {}", e);
                }
            })
        };

        let mut instances = self.instances.lock().unwrap();
        instances.insert(
            id,
            Handles {
                radv_handle: Arc::new(radv_handle),
                dhcpv6_handle: Arc::new(dhcpv6_handle),
            },
        );

        let (connection, handle, _) = new_connection().map_err(|_| Error::Failed)?;
        spawn(connection);

        spawn(async move {
            if let Err(e) = add_ipv6_route(
                &handle,
                &interface_name,
                adjusted_base_ip,
                new_prefix_length,
                None,
                1024,
            )
            .await
            {
                error!("Failed to add ipv6 route: {}", e);
            }
        });

        Ok(())
    }

    pub fn device_name(id: &Uuid) -> String {
        format!("vmtap{}", &id.to_string()[..8])
    }

    fn get_ipv6_info(&self) -> (Ipv6Addr, u8, u16) {
        let new_count = self.prefix_count.fetch_add(1, Ordering::SeqCst) + 1;

        (self.ipv6_address, self.prefix_length, new_count)
    }
}
