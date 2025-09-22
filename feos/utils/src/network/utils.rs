// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use super::dhcpv6::*;
use futures::stream::TryStreamExt;
use log::{error, info, warn};
use netlink_packet_route::link::{LinkAttribute, LinkFlags, LinkMessage};
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
    let base_path = format!("/sys/class/net/{INTERFACE_NAME}/device");

    let autoprobe_path = format!("{base_path}/sriov_drivers_autoprobe");
    info!("Disabling sriov_drivers_autoprobe at {autoprobe_path}");
    let mut autoprobe_file = OpenOptions::new()
        .write(true)
        .open(&autoprobe_path)
        .await
        .map_err(|e| format!("Failed to open sriov_drivers_autoprobe: {e}"))?;
    autoprobe_file
        .write_all(b"0\n")
        .await
        .map_err(|e| format!("Failed to disable autoprobe: {e}"))?;

    let numvfs_path = format!("{base_path}/sriov_numvfs");
    let mut numvfs_file = OpenOptions::new()
        .write(true)
        .open(&numvfs_path)
        .await
        .map_err(|e| e.to_string())?;

    info!("Resetting VFs to 0 for {INTERFACE_NAME}");
    numvfs_file
        .write_all(b"0\n")
        .await
        .map_err(|e| format!("Failed to write 0 to sriov_numvfs: {e}"))?;
    sleep(Duration::from_secs(1)).await;

    info!("Creating {num_vfs} sriov virtual functions for {INTERFACE_NAME}");
    numvfs_file
        .write_all(format!("{num_vfs}\n").as_bytes())
        .await
        .map_err(|e| format!("Failed to write to sriov_numvfs: {e}"))?;
    sleep(Duration::from_secs(2)).await;

    let device_path = read_link(&base_path).await.map_err(|e| e.to_string())?;
    let pci_address = device_path
        .file_name()
        .ok_or("No PCI address found".to_string())?;
    let pci_address = pci_address.to_str().ok_or("No PCI address found")?;

    info!("Found PCI address of {INTERFACE_NAME}: {pci_address}");

    let sriov_offset = get_device_information(pci_address, "sriov_offset")
        .await
        .map_err(|e| e.to_string())?;

    let sriov_offset = sriov_offset.parse::<u32>().map_err(|e| e.to_string())?;

    let base_pci_address = parse_pci_address(pci_address)?;

    let virtual_funcs: Vec<String> = (0..num_vfs)
        .map(|x| nth_next_pci_address(base_pci_address, x + sriov_offset))
        .map(format_pci_address)
        .collect();

    for vf_pci in virtual_funcs.iter() {
        if let Err(e) = bind_vf_to_vfio(vf_pci).await {
            return Err(format!("Failed to bind VF {vf_pci} to vfio-pci: {e}"));
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
    format!("{domain:04x}:{bus:02x}:{slot:02x}.{function}")
}

async fn bind_vf_to_vfio(pci_address: &str) -> Result<(), io::Error> {
    let override_path = format!("/sys/bus/pci/devices/{pci_address}/driver_override");
    let mut override_file = OpenOptions::new().write(true).open(&override_path).await?;
    override_file.write_all(b"vfio-pci").await?;

    let bind_path = "/sys/bus/pci/drivers/vfio-pci/bind";
    let mut bind_file = OpenOptions::new().write(true).open(bind_path).await?;
    bind_file.write_all(pci_address.as_bytes()).await?;

    Ok(())
}

async fn get_device_information(pci: &str, field: &str) -> Result<String, io::Error> {
    let path = format!("/sys/bus/pci/devices/{pci}/{field}");
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

    let mut link_msg = LinkMessage::default();
    link_msg.header.index = link.header.index;
    link_msg.header.flags = link.header.flags | LinkFlags::Up;
    link_msg.header.change_mask = LinkFlags::Up;

    handle
        .link()
        .set(link_msg)
        .execute()
        .await
        .map_err(|e| format!("{interface_name} can not be set up: {e}"))?;

    info!("{interface_name}:");
    for attr in link.attributes {
        match attr {
            LinkAttribute::Address(mac_bytes) => {
                info!("  mac: {}", format_mac(mac_bytes.clone()));
            }
            LinkAttribute::Carrier(carrier) => {
                info!("  carrier: {carrier}");
            }
            LinkAttribute::Mtu(mtu) => {
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
