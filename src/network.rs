use crate::dhcpv6::*;
use futures::stream::TryStreamExt;
use log::{info, warn};
use rtnetlink::new_connection;
use tokio::time::{self, Duration};

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

pub async fn configure_network_devices() -> Result<(), String> {
    let ignore_ra_flag = true; // Till the RA has the correct flags (O or M), ignore the flag
    let interface_name = String::from("eth0");
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

// Print all packets to the console for debugging purposes
async fn _capture_packets(interface_name: String) {
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
