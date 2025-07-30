use super::dhcpv6::*;
use futures::stream::TryStreamExt;
use log::{error, info, warn};
use netlink_packet_route::route::RouteType;
use rtnetlink::new_connection;
use std::fs::File;
use std::io;
use std::io::Write;
use std::net::Ipv6Addr;
use tokio::fs::{read_link, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{sleep, Duration};

pub const INTERFACE_NAME: &str = "eth0";

pub async fn configure_sriov(num_vfs: u32) -> Result<(), String> {
    let base_path = format!("/sys/class/net/{}/device", INTERFACE_NAME);

    let file_path = format!("{}/sriov_numvfs", base_path);
    let mut file = OpenOptions::new()
        .write(true)
        .open(&file_path)
        .await
        .map_err(|e| e.to_string())?;

    let value = format!("{}\n", num_vfs);
    if let Err(e) = file.write_all(value.as_bytes()).await {
        return Err(format!("Failed to write to the file: {}", e));
    }
    info!("Created {} sriov virtual functions", num_vfs);

    let device_path = read_link(base_path).await.map_err(|e| e.to_string())?;
    let pci_address = device_path
        .file_name()
        .ok_or("No PCI address found".to_string())?;
    let pci_address = pci_address.to_str().ok_or("No PCI address found")?;

    info!("Found PCI address of {}: {}", INTERFACE_NAME, pci_address);

    let sriov_offset = get_device_information(pci_address, "sriov_offset")
        .await
        .map_err(|e| e.to_string())?;

    let sriov_offset = match sriov_offset.parse::<u32>() {
        Ok(n) => n,
        Err(e) => return Err(e.to_string()),
    };

    let base_pci_address = parse_pci_address(pci_address)?;

    let virtual_funcs: Vec<String> = (0..num_vfs)
        .map(|x| nth_next_pci_address(base_pci_address, x + sriov_offset))
        .map(format_pci_address)
        .collect();

    const RETRIES: i32 = 5;
    for (index, vf) in virtual_funcs.iter().enumerate() {
        for i in 1..RETRIES {
            info!("try to unbind device {}: {:?}/{}", vf, i, RETRIES);
            if let Err(e) = unbind_device(vf).await {
                warn!("failed to unbind device {}: {}", vf, e.to_string());
                sleep(Duration::from_secs(2)).await;
            } else {
                info!("successfull unbound device {}", vf);

                if let Err(e) = bind_device(index, vf).await {
                    warn!("failed to bind devices: {}", e.to_string())
                }
                break;
            }
        }
    }

    Ok(())
}

fn parse_pci_address(address: &str) -> Result<(u16, u8, u8, u8), String> {
    let parts: Vec<&str> = address.split(&[':', '.', ' '][..]).collect();
    if parts.len() != 4 {
        return Err("Invalid PCI address format".to_string());
    }

    let domain = u16::from_str_radix(parts[0], 16).map_err(|_| "Invalid domain".to_string())?;
    let bus = u8::from_str_radix(parts[1], 16).map_err(|_| "Invalid bus".to_string())?;
    let slot = u8::from_str_radix(parts[2], 16).map_err(|_| "Invalid slot".to_string())?;
    let function = u8::from_str_radix(parts[3], 16).map_err(|_| "Invalid function".to_string())?;

    Ok((domain, bus, slot, function))
}

fn nth_next_pci_address(address: (u16, u8, u8, u8), n: u32) -> (u16, u8, u8, u8) {
    let (domain, bus, slot, function) = address;
    let total_functions = (domain as u32) * 256 * 32 * 8
        + (bus as u32) * 32 * 8
        + (slot as u32) * 8
        + function as u32
        + n;

    let new_domain = (total_functions / (256 * 32 * 8)) as u16;
    let remaining = total_functions % (256 * 32 * 8);
    let new_bus = (remaining / (32 * 8)) as u8;
    let remaining = remaining % (32 * 8);
    let new_slot = (remaining / 8) as u8;
    let new_function = (remaining % 8) as u8;

    (new_domain, new_bus, new_slot, new_function)
}

fn format_pci_address(address: (u16, u8, u8, u8)) -> String {
    let (domain, bus, slot, function) = address;
    format!("{:04x}:{:02x}:{:02x}.{}", domain, bus, slot, function)
}

async fn unbind_device(pci: &str) -> Result<(), io::Error> {
    let unbind_path = format!("/sys/bus/pci/devices/{}/driver/unbind", pci);
    let mut file = OpenOptions::new().write(true).open(&unbind_path).await?;

    file.write_all(pci.as_bytes()).await?;
    info!("unbound device: {}", pci);
    Ok(())
}

async fn bind_device(index: usize, pci_address: &str) -> Result<(), io::Error> {
    info!("try to bind device to vfio: {}", pci_address);
    if index == 0 {
        vfio_new_id(pci_address).await
    } else {
        vfio_bind(pci_address).await
    }
}

async fn vfio_new_id(pci_address: &str) -> Result<(), io::Error> {
    let vendor = get_device_information(pci_address, "vendor").await?;
    let vendor = vendor[2..].to_string();

    let device = get_device_information(pci_address, "device").await?;
    let device = device[2..].to_string();

    let mut file = OpenOptions::new()
        .write(true)
        .open("/sys/bus/pci/drivers/vfio-pci/new_id")
        .await?;

    let content = format!("{} {}", vendor, device);
    file.write_all(content.as_bytes()).await?;
    info!("bound devices ({}) to vfio-pci", pci_address);
    Ok(())
}

async fn vfio_bind(pci_address: &str) -> Result<(), io::Error> {
    let mut file = OpenOptions::new()
        .write(true)
        .open("/sys/bus/pci/drivers/vfio-pci/bind")
        .await?;

    file.write_all(pci_address.as_bytes()).await?;
    info!("bound devices ({}) to vfio-pci", pci_address);
    Ok(())
}

async fn get_device_information(pci: &str, field: &str) -> Result<String, io::Error> {
    let path = format!("/sys/bus/pci/devices/{}/{}", pci, field);
    let mut file = OpenOptions::new().read(true).open(&path).await?;

    let mut dst = String::new();
    file.read_to_string(&mut dst).await?;

    Ok(dst.trim().to_string())
}

pub async fn configure_network_devices() -> Result<Option<(Ipv6Addr, u8)>, String> {
    let ignore_ra_flag = true; // Till the RA has the correct flags (O or M), ignore the flag
    let interface_name = String::from(INTERFACE_NAME);
    let (connection, handle, _) = new_connection().unwrap();
    let mut delegated_prefix_option: Option<(Ipv6Addr, u8)> = None;
    tokio::spawn(connection);

    enable_ipv6_forwarding().map_err(|e| format!("Failed to enable ipv6 forwarding: {e}"))?;

    let mut link_ts = handle
        .link()
        .get()
        .match_name(interface_name.clone())
        .execute();

    let link = link_ts
        .try_next()
        .await
        .map_err(|e| format!("{interface_name} not found: {e}"))?
        .ok_or("Link not found".to_string())?;

    handle
        .link()
        .set(link.header.index)
        .up()
        .execute()
        .await
        .map_err(|e| format!("{interface_name} can not be set up: {e}"))?;

    info!("{interface_name}:");
    for attr in link.attributes {
        match attr {
            netlink_packet_route::link::LinkAttribute::Address(mac_bytes) => {
                info!("  mac: {}", format_mac(mac_bytes.clone()));
            }
            netlink_packet_route::link::LinkAttribute::Carrier(carrier) => {
                info!("  carrier: {carrier}");
            }
            netlink_packet_route::link::LinkAttribute::Mtu(mtu) => {
                info!("  mtu: {mtu}");
            }
            _ => (),
        }
    }

    if let Some(ipv6_gateway) = is_dhcpv6_needed(interface_name.clone(), ignore_ra_flag) {
        sleep(Duration::from_secs(4)).await;
        match run_dhcpv6_client(interface_name.clone()).await {
            Ok(result) => {
                send_neigh_solicitation(interface_name.clone(), &ipv6_gateway, &result.address);
                if let Some(prefix_info) = result.prefix {
                    let delegated_prefix = prefix_info.prefix;
                    let prefix_length = prefix_info.prefix_length;
                    info!(
                        "Received delegated prefix {delegated_prefix} with length {prefix_length}"
                    );
                    delegated_prefix_option = Some((delegated_prefix, prefix_length));
                    if let Err(e) = add_ipv6_route(
                        &handle,
                        INTERFACE_NAME,
                        delegated_prefix,
                        prefix_length,
                        None,
                        1024,
                        RouteType::Unreachable,
                    )
                    .await
                    {
                        error!("Failed to add unreachable IPv6 route: {e}");
                    }
                } else {
                    info!("No prefix delegation received.");
                }
                info!("Setting IPv6 gateway to {ipv6_gateway} on interface {interface_name}");
                if let Err(e) = set_ipv6_gateway(&handle, &interface_name, ipv6_gateway).await {
                    warn!("Failed to set IPv6 gateway: {e}");
                }
            }
            Err(e) => warn!("Error running DHCPv6 client: {e}"),
        }
    }

    Ok(delegated_prefix_option)
}

pub fn enable_ipv6_forwarding() -> Result<(), std::io::Error> {
    File::create("/proc/sys/net/ipv6/conf/all/forwarding")?.write_all(b"1")?;
    Ok(())
}

fn format_mac(bytes: Vec<u8>) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<String>>()
        .join(":")
}
