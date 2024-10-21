use log::{debug, error, info};
use pnet::datalink::{self, Channel::Ethernet, NetworkInterface};
use pnet::packet::icmpv6::checksum as icmpv6_checksum;
use pnet::packet::icmpv6::{
    ndp::{MutableRouterAdvertPacket, NdpOption, NdpOptionTypes},
    Icmpv6Code, Icmpv6Packet, Icmpv6Types,
};
use pnet::packet::ipv6::MutableIpv6Packet;
use pnet::packet::{
    ethernet::{EtherTypes, EthernetPacket, MutableEthernetPacket},
    Packet,
};
use pnet::util::MacAddr;
use std::error::Error;
use std::net::Ipv6Addr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task;
const M_FLAG: u8 = 1 << 7;
const O_FLAG: u8 = 1 << 6;

pub async fn start_radv_server(
    interface_name: String,
    prefix: Ipv6Addr,
    prefix_length: u8,
) -> Result<(), Box<dyn std::error::Error>> {
    let interfaces = datalink::interfaces();
    let interface = interfaces
        .into_iter()
        .find(|iface| iface.name == interface_name)
        .ok_or("Interface not found")?;
    let interface = Arc::new(interface);

    let (mut tx, mut rx) = match datalink::channel(&interface, Default::default())? {
        Ethernet(tx, rx) => (tx, rx),
        _ => return Err("Unhandled channel type".into()),
    };

    info!("Listening for Router Solicitations on {}", interface.name);

    let interface_clone = Arc::clone(&interface);
    let prefix_clone = prefix;
    let prefix_length_clone = prefix_length;

    task::spawn_blocking(move || {
        let mut last_unsolicited = Instant::now();

        loop {
            // Handle incoming packets if available
            if let Ok(packet) = rx.next() {
                if let Err(e) = handle_packet(
                    &interface_clone,
                    &mut *tx,
                    packet,
                    prefix_clone,
                    prefix_length_clone,
                ) {
                    error!("Error handling packet: {}", e);
                }
            }

            if last_unsolicited.elapsed() >= Duration::from_secs(600) {
                if let Err(e) = send_router_advertisement(
                    &interface_clone,
                    &mut *tx,
                    Ipv6Addr::from_str("ff02::1").unwrap(),
                    prefix_clone,
                    prefix_length_clone,
                ) {
                    error!("Failed to send unsolicited Router Advertisement: {}", e);
                } else {
                    info!("Sent unsolicited Router Advertisement");
                }
                last_unsolicited = Instant::now();
            }

            std::thread::sleep(Duration::from_millis(100));
        }
    });

    Ok(())
}

fn handle_packet(
    interface: &NetworkInterface,
    tx: &mut dyn datalink::DataLinkSender,
    raw_packet: &[u8],
    prefix: Ipv6Addr,
    prefix_length: u8,
) -> Result<(), Box<dyn std::error::Error>> {
    let ethernet_packet =
        EthernetPacket::new(raw_packet).ok_or("Failed to parse Ethernet packet")?;

    if ethernet_packet.get_ethertype() == EtherTypes::Ipv6 {
        let ipv6_packet = pnet::packet::ipv6::Ipv6Packet::new(ethernet_packet.payload())
            .ok_or("Failed to parse IPv6 packet")?;

        if let Some(icmp_packet) = Icmpv6Packet::new(ipv6_packet.payload()) {
            if icmp_packet.get_icmpv6_type() == Icmpv6Types::RouterSolicit {
                debug!(
                    "Received Router Solicitation from {}",
                    ipv6_packet.get_source()
                );
                send_router_advertisement(
                    interface,
                    tx,
                    ipv6_packet.get_source(),
                    prefix,
                    prefix_length,
                )?;
            }
        }
    }
    Ok(())
}

fn send_router_advertisement(
    interface: &NetworkInterface,
    tx: &mut dyn pnet::datalink::DataLinkSender,
    destination_ip: Ipv6Addr,
    prefix: Ipv6Addr,
    prefix_length: u8,
) -> Result<(), Box<dyn Error>> {
    let source_mac = interface.mac.ok_or("Interface has no MAC address")?;
    let dest_mac = ipv6_multicast_to_mac(destination_ip);

    let mut ethernet_buffer = [0u8; 1500];
    let mut ethernet_packet = MutableEthernetPacket::new(&mut ethernet_buffer).unwrap();
    ethernet_packet.set_destination(dest_mac);
    ethernet_packet.set_source(source_mac);
    ethernet_packet.set_ethertype(EtherTypes::Ipv6);

    let source_ip = get_link_local_addr(interface)?;
    let mut ipv6_buffer = [0u8; 1280];
    let mut ipv6_packet = MutableIpv6Packet::new(&mut ipv6_buffer).unwrap();
    ipv6_packet.set_version(6);
    ipv6_packet.set_source(source_ip);
    ipv6_packet.set_destination(destination_ip);
    ipv6_packet.set_next_header(pnet::packet::ip::IpNextHeaderProtocols::Icmpv6);
    ipv6_packet.set_hop_limit(255);

    const RA_LENGTH: usize = 16 + 32; // 16 bytes fixed fields + 32 bytes Prefix Information
    let mut ra_buffer = [0u8; RA_LENGTH];
    let mut ra_packet =
        MutableRouterAdvertPacket::new(&mut ra_buffer).ok_or("Failed to create RA packet")?;
    ra_packet.set_icmpv6_type(Icmpv6Types::RouterAdvert);
    ra_packet.set_icmpv6_code(Icmpv6Code(0));
    ra_packet.set_hop_limit(64);
    ra_packet.set_flags(M_FLAG | O_FLAG); // Set M and O flags
    ra_packet.set_lifetime(1800);
    ra_packet.set_reachable_time(0);
    ra_packet.set_retrans_time(0);

    let mut pi_data = [0u8; 30];
    pi_data[0] = prefix_length;
    pi_data[1] = 0b10000000;
    pi_data[2..6].copy_from_slice(&1800u32.to_be_bytes());
    pi_data[6..10].copy_from_slice(&0u32.to_be_bytes());
    pi_data[14..30].copy_from_slice(&prefix.octets());

    let prefix_option = NdpOption {
        option_type: NdpOptionTypes::PrefixInformation,
        length: 4,
        data: pi_data.to_vec(),
    };

    ra_packet.set_options(&[prefix_option]);

    let checksum = icmpv6_checksum(
        &Icmpv6Packet::new(ra_packet.packet())
            .ok_or("Failed to create ICMPv6 packet for checksum")?,
        &source_ip,
        &destination_ip,
    );
    ra_packet.set_checksum(checksum);

    ipv6_packet.set_payload_length(RA_LENGTH as u16);
    ipv6_packet.set_payload(&ra_packet.packet()[..RA_LENGTH]);

    ethernet_packet.set_payload(ipv6_packet.packet());
    let ethernet_size = RA_LENGTH + 14 + 40; // ethernet + ipv6 header size

    tx.send_to(
        &ethernet_packet.packet()[..ethernet_size],
        Some(interface.clone()),
    )
    .ok_or("Failed to send Router Advertisement")??;

    debug!("Sent Router Advertisement to {}", destination_ip);
    Ok(())
}

fn get_link_local_addr(
    interface: &NetworkInterface,
) -> Result<Ipv6Addr, Box<dyn std::error::Error>> {
    for ip in &interface.ips {
        if let std::net::IpAddr::V6(ipv6) = ip.ip() {
            if is_unicast_link_local(&ipv6) {
                return Ok(ipv6);
            }
        }
    }
    Err("No link-local address found on interface".into())
}

fn ipv6_multicast_to_mac(ipv6: Ipv6Addr) -> MacAddr {
    let segments = ipv6.octets();
    MacAddr::new(
        0x33,
        0x33,
        segments[12],
        segments[13],
        segments[14],
        segments[15],
    )
}

fn is_unicast_link_local(addr: &Ipv6Addr) -> bool {
    addr.segments()[0] & 0xffc0 == 0xfe80
}
