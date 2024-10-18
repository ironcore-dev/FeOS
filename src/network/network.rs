use std::io;

use crate::network::dhcpv6::*;
use futures::stream::TryStreamExt;
use log::{info, warn};
use rtnetlink::new_connection;
use tokio::fs::{read_link, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{self, sleep, Duration};

use pnet::datalink::{self, Channel::Ethernet, Config};
use pnet::packet::ethernet::EthernetPacket;
use pnet::packet::icmp::IcmpPacket;
use pnet::packet::icmpv6::{Icmpv6Packet, Icmpv6Types};
use pnet::packet::ipv4::Ipv4Packet;
use pnet::packet::ipv6::Ipv6Packet;
use pnet::packet::tcp::TcpPacket;
use pnet::packet::udp::UdpPacket;
use pnet::packet::Packet;

use netlink_packet_route::neighbour::*;

const INTERFACE_NAME: &str = "eth0";

pub async fn configure_network_devices() -> Result<(), String> {
    let ignore_ra_flag = true; // Till the RA has the correct flags (O or M), ignore the flag
    let interface_name = String::from(INTERFACE_NAME);
    let (connection, handle, _) = new_connection().unwrap();
    let mut mac_bytes_option: Option<Vec<u8>> = None;
    tokio::spawn(connection);

    let mut link_ts = handle
        .link()
        .get()
        .match_name(interface_name.clone())
        .execute();

    let link = link_ts
        .try_next()
        .await
        .map_err(|e| format!("{} not found: {}", interface_name, e))?
        .ok_or("option A empty".to_string())?;

    handle
        .link()
        .set(link.header.index)
        .up()
        .execute()
        .await
        .map_err(|e| format!("{} can not be set up: {}", interface_name, e))?;

    info!("{}:", interface_name);
    for attr in link.attributes {
        match attr {
            netlink_packet_route::link::LinkAttribute::Address(mac_bytes) => {
                info!("  mac: {}", format_mac(mac_bytes.clone()));
                mac_bytes_option = Some(mac_bytes);
            }
            netlink_packet_route::link::LinkAttribute::Carrier(carrier) => {
                info!("  carrier: {}", carrier);
            }
            netlink_packet_route::link::LinkAttribute::Mtu(mtu) => {
                info!("  mtu: {}", mtu);
            }
            netlink_packet_route::link::LinkAttribute::MaxMtu(max_mtu) => {
                info!("  max_mtu: {}", max_mtu);
            }
            netlink_packet_route::link::LinkAttribute::OperState(state) => {
                let state = match state {
                    netlink_packet_route::link::State::Unknown => String::from("unknown"),
                    netlink_packet_route::link::State::NotPresent => String::from("not present"),
                    netlink_packet_route::link::State::Down => String::from("down"),
                    netlink_packet_route::link::State::LowerLayerDown => {
                        String::from("lower layer down")
                    }
                    netlink_packet_route::link::State::Testing => String::from("testing"),
                    netlink_packet_route::link::State::Dormant => String::from("dormant"),
                    netlink_packet_route::link::State::Up => String::from("up"),
                    netlink_packet_route::link::State::Other(x) => {
                        format!("other ({})", x)
                    }
                    _ => String::from("unknown state"),
                };
                info!("  state: {}", state);
            }
            _ => (),
        }
    }

    if let Some(mac_bytes) = mac_bytes_option {
        match mac_to_ipv6_link_local(&mac_bytes) {
            Some(ipv6_ll_addr) => {
                let result = set_ipv6_address(&handle, &interface_name, ipv6_ll_addr, 64).await;
                if let Err(e) = result {
                    warn!(
                        "{} cannot set link local IPv6 address: {}",
                        interface_name, e
                    );
                }
            }
            None => warn!("Invalid MAC address length"),
        }
    } else {
        warn!("No MAC address found for IPv6 link-local address calculation");
    }

    if let Some(ipv6_gateway) = is_dhcpv6_needed(interface_name.clone(), ignore_ra_flag) {
        time::sleep(Duration::from_secs(4)).await;
        match run_dhcpv6_client(interface_name.clone()).await {
            Ok(addr) => send_neigh_solicitation(interface_name.clone(), &ipv6_gateway, &addr),
            Err(e) => warn!("Error: {}", e),
        }
    }

    let mut addr_ts = handle
        .address()
        .get()
        .set_link_index_filter(link.header.index)
        .execute();

    while let Some(addr_msg) = addr_ts
        .try_next()
        .await
        .map_err(|e| format!("Could not get addr: {}", e))?
    {
        for attr in addr_msg.attributes {
            if let netlink_packet_route::address::AddressAttribute::Address(addr) = attr {
                info!("- {}/{}", addr, addr_msg.header.prefix_len);
            }
        }
    }

    Ok(())
}

fn format_mac(bytes: Vec<u8>) -> String {
    bytes
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect::<Vec<String>>()
        .join(":")
}

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

// Print all packets to the console for debugging purposes
pub async fn _capture_packets(interface_name: String) {
    let interfaces = datalink::interfaces();
    let interface = interfaces
        .into_iter()
        .find(|iface| iface.name == interface_name)
        .expect("Network interface not found");

    let config = Config {
        promiscuous: true,
        ..Default::default()
    };

    let (_, mut rx) = match datalink::channel(&interface, config) {
        Ok(Ethernet(tx, rx)) => (tx, rx),
        Ok(_) => panic!("Unhandled channel type"),
        Err(e) => panic!(
            "An error occurred when creating the datalink channel: {}",
            e
        ),
    };

    info!("Capturing packets on interface: {}", interface_name);

    loop {
        match rx.next() {
            Ok(packet) => {
                let ethernet = EthernetPacket::new(packet).unwrap();
                info!("Ethernet packet: {:?}", ethernet);

                match ethernet.get_ethertype() {
                    pnet::packet::ethernet::EtherTypes::Ipv4 => {
                        if let Some(ipv4) = Ipv4Packet::new(ethernet.payload()) {
                            info!("IPv4 packet: {:?}", ipv4);
                            match ipv4.get_next_level_protocol() {
                                pnet::packet::ip::IpNextHeaderProtocols::Tcp => {
                                    if let Some(tcp) = TcpPacket::new(ipv4.payload()) {
                                        info!("TCP packet: {:?}", tcp);
                                    }
                                }
                                pnet::packet::ip::IpNextHeaderProtocols::Udp => {
                                    if let Some(udp) = UdpPacket::new(ipv4.payload()) {
                                        info!("UDP packet: {:?}", udp);
                                    }
                                }
                                pnet::packet::ip::IpNextHeaderProtocols::Icmp => {
                                    if let Some(icmp) = IcmpPacket::new(ipv4.payload()) {
                                        info!("ICMP packet: {:?}", icmp);
                                    }
                                }
                                _ => info!("Unknown IPv4 L4 protocol"),
                            }
                        }
                    }
                    pnet::packet::ethernet::EtherTypes::Ipv6 => {
                        if let Some(ipv6) = Ipv6Packet::new(ethernet.payload()) {
                            info!("IPv6 packet: {:?}", ipv6);
                            match ipv6.get_next_header() {
                                pnet::packet::ip::IpNextHeaderProtocols::Tcp => {
                                    if let Some(tcp) = TcpPacket::new(ipv6.payload()) {
                                        info!("TCP packet: {:?}", tcp);
                                    }
                                }
                                pnet::packet::ip::IpNextHeaderProtocols::Udp => {
                                    if let Some(udp) = UdpPacket::new(ipv6.payload()) {
                                        info!("UDP packet: {:?}", udp);
                                    }
                                }
                                pnet::packet::ip::IpNextHeaderProtocols::Icmpv6 => {
                                    if let Some(icmpv6) = Icmpv6Packet::new(ipv6.payload()) {
                                        info!("ICMPv6 packet: {:?}", icmpv6);
                                        match icmpv6.get_icmpv6_type() {
                                            Icmpv6Types::RouterSolicit => {
                                                info!("Router Solicitation")
                                            }
                                            Icmpv6Types::RouterAdvert => {
                                                info!("Router Advertisement")
                                            }
                                            Icmpv6Types::NeighborSolicit => {
                                                info!("Neighbor Solicitation")
                                            }
                                            Icmpv6Types::NeighborAdvert => {
                                                info!("Neighbor Advertisement")
                                            }
                                            Icmpv6Types::Redirect => info!("Redirect"),
                                            _ => info!("Other ICMPv6 type"),
                                        }
                                    }
                                }
                                pnet::packet::ip::IpNextHeaderProtocols::Hopopt => {
                                    info!("IPv6 Hop-by-Hop Options header");
                                }
                                _ => info!(
                                    "Unknown or unsupported next header: {:?}",
                                    ipv6.get_next_header()
                                ),
                            }
                        }
                    }
                    _ => info!("Unknown packet type"),
                }
            }
            Err(e) => {
                info!("An error occurred while reading: {}", e);
                tokio::task::yield_now().await;
            }
        }
    }
}

async fn _get_nd_cache() -> Result<(), Box<dyn std::error::Error>> {
    let (connection, handle, _) = new_connection().unwrap();
    tokio::spawn(connection);

    let mut neighbors = handle.neighbours().get().execute();

    while let Ok(Some(neigh)) = neighbors.try_next().await {
        for attr in &neigh.attributes {
            match attr {
                NeighbourAttribute::Destination(addr) => {
                    info!("IP Address: {:?}", addr);
                }
                NeighbourAttribute::LinkLocalAddress(lladdr) => {
                    let hex_address: String = lladdr
                        .iter()
                        .map(|byte| format!("{:02x}", byte))
                        .collect::<Vec<String>>()
                        .join(":");
                    info!("Link-layer Address: {:?}", hex_address);
                }
                NeighbourAttribute::CacheInfo(info) => {
                    info!("Cache Info: {:?}", info);
                }
                _ => {
                    info!("Other attribute: {:?}", attr);
                }
            }
        }
        info!("------------------------");
    }
    info!("");
    info!("");
    Ok(())
}
