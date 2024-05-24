use dhcproto::v6::*;
use futures::stream::TryStreamExt;
use log::{error, info, warn};
use netlink_packet_route::route::{
    RouteAddress, RouteAttribute, RouteProtocol, RouteScope, RouteType,
};
use netlink_packet_route::AddressFamily;
use pnet::packet::icmpv6::Icmpv6Code;
use pnet::{
    datalink::{self, Channel::Ethernet, NetworkInterface},
    packet::{
        ethernet::{EtherTypes, EthernetPacket, MutableEthernetPacket},
        icmpv6::{checksum, ndp::*, Icmpv6Packet, Icmpv6Types, MutableIcmpv6Packet},
        ip::IpNextHeaderProtocols,
        ipv6::{Ipv6Packet, MutableIpv6Packet},
        Packet,
    },
    util::MacAddr,
};
use rand::{thread_rng, Rng};
use rtnetlink::{new_connection, Error, Handle};
use std::net::{Ipv6Addr, SocketAddr};
use tokio::net::UdpSocket;

pub fn mac_to_ipv6_link_local(mac_address: &[u8]) -> Option<Ipv6Addr> {
    if mac_address.len() == 6 {
        let mut bytes = [0u8; 16];
        bytes[0] = 0xfe;
        bytes[1] = 0x80;
        bytes[8] = mac_address[0] ^ 0b00000010;
        bytes[9] = mac_address[1];
        bytes[10] = mac_address[2];
        bytes[11] = 0xff;
        bytes[12] = 0xfe;
        bytes[13] = mac_address[3];
        bytes[14] = mac_address[4];
        bytes[15] = mac_address[5];
        Some(Ipv6Addr::from(bytes))
    } else {
        None
    }
}

pub fn send_neigh_solicitation(
    interface_name: String,
    target_address: &Ipv6Addr,
    src_address: &Ipv6Addr,
) {
    let interface_names_match = |iface: &datalink::NetworkInterface| iface.name == interface_name;

    let interfaces = datalink::interfaces();
    let interface = interfaces
        .into_iter()
        .find(interface_names_match)
        .expect("Error getting interface");

    let (mut tx, mut _rx) = match datalink::channel(&interface, Default::default()) {
        Ok(Ethernet(tx, rx)) => (tx, rx),
        Ok(_) => panic!("Unhandled channel type"),
        Err(e) => panic!("Error creating channel: {}", e),
    };

    let mut packet_buffer = [0u8; 128];
    let mut ethernet_packet = MutableEthernetPacket::new(&mut packet_buffer).unwrap();

    ethernet_packet.set_destination(MacAddr::broadcast());
    ethernet_packet.set_source(interface.mac.unwrap());
    ethernet_packet.set_ethertype(EtherTypes::Ipv6);

    let mut ipv6_and_icmp_buffer = [0u8; 64];

    let mut ipv6_packet = MutableIpv6Packet::new(&mut ipv6_and_icmp_buffer[..40]).unwrap();
    ipv6_packet.set_version(6);
    ipv6_packet.set_next_header(IpNextHeaderProtocols::Icmpv6);
    ipv6_packet.set_payload_length(24);
    ipv6_packet.set_hop_limit(255);
    ipv6_packet.set_source(*src_address);
    ipv6_packet.set_destination(*target_address);

    let mut icmp_packet = MutableIcmpv6Packet::new(&mut ipv6_and_icmp_buffer[40..]).unwrap();
    icmp_packet.set_icmpv6_type(Icmpv6Types::NeighborSolicit);
    icmp_packet.set_icmpv6_code(Icmpv6Code(0));
    icmp_packet.set_checksum(0);

    let mut icmp_payload = [0u8; 20];
    icmp_payload[4..].copy_from_slice(&target_address.octets());
    icmp_packet.set_payload(&icmp_payload);

    let checksum = checksum(
        &Icmpv6Packet::new(icmp_packet.packet()).unwrap(),
        src_address,
        target_address,
    );
    icmp_packet.set_checksum(checksum);

    ethernet_packet.set_payload(&ipv6_and_icmp_buffer);

    match tx.send_to(ethernet_packet.packet(), Some(interface.clone())) {
        Some(Ok(_)) => info!("Neighbor solicitation sent."),
        Some(Err(e)) => error!("Failed to send neighbor solicitation: {}", e),
        None => error!("Failed to send neighbor solicitation: send_to returned None"),
    }
}

fn send_router_solicitation(interface: &NetworkInterface, tx: &mut dyn datalink::DataLinkSender) {
    let source_ip = Ipv6Addr::UNSPECIFIED;
    let destination_ip = "ff02::2".parse::<Ipv6Addr>().unwrap();

    let mut packet_buffer = [0u8; 128];
    let mut ethernet_packet = MutableEthernetPacket::new(&mut packet_buffer).unwrap();

    ethernet_packet.set_destination(MacAddr::broadcast());
    ethernet_packet.set_source(interface.mac.unwrap());
    ethernet_packet.set_ethertype(EtherTypes::Ipv6);

    let mut ipv6_and_icmp_buffer = [0u8; 48];

    let mut ipv6_packet = MutableIpv6Packet::new(&mut ipv6_and_icmp_buffer[..40]).unwrap();
    ipv6_packet.set_version(6);
    ipv6_packet.set_next_header(IpNextHeaderProtocols::Icmpv6);
    ipv6_packet.set_payload_length(8);
    ipv6_packet.set_hop_limit(255);
    ipv6_packet.set_source(source_ip);
    ipv6_packet.set_destination(destination_ip);

    let mut icmp_packet = MutableIcmpv6Packet::new(&mut ipv6_and_icmp_buffer[40..]).unwrap();
    icmp_packet.set_icmpv6_type(Icmpv6Types::RouterSolicit);

    let checksum = checksum(
        &Icmpv6Packet::new(icmp_packet.packet()).unwrap(),
        &source_ip,
        &destination_ip,
    );
    icmp_packet.set_checksum(checksum);

    ethernet_packet.set_payload(&ipv6_and_icmp_buffer);

    match tx.send_to(ethernet_packet.packet(), Some(interface.clone())) {
        Some(Ok(_)) => info!("Router solicitation sent."),
        Some(Err(e)) => error!("Failed to send router solicitation: {}", e),
        None => error!("Failed to send router solicitation: send_to returned None"),
    }
}

pub fn is_dhcpv6_needed(interface_name: String, ignore_ra_flag: bool) -> Option<Ipv6Addr> {
    let interface_names_match = |iface: &datalink::NetworkInterface| iface.name == interface_name;
    let mut sender_ipv6_address: Option<Ipv6Addr> = None;

    let interfaces = datalink::interfaces();
    let interface = interfaces
        .into_iter()
        .find(interface_names_match)
        .expect("Error getting interface");

    let (mut tx, mut rx) = match datalink::channel(&interface, Default::default()) {
        Ok(Ethernet(tx, rx)) => (tx, rx),
        Ok(_) => panic!("Unhandled channel type"),
        Err(e) => panic!("Error creating channel: {}", e),
    };

    info!("Sending Router Solicitation ...");
    send_router_solicitation(&interface, &mut *tx);

    while let Ok(raw_packet) = rx.next() {
        let ethernet_packet = EthernetPacket::new(raw_packet).unwrap();
        if ethernet_packet.get_ethertype() == EtherTypes::Ipv6 {
            info!("Router Advertisement processing starting ... ");
            let payload = ethernet_packet.payload();
            let ipv6_packet = Ipv6Packet::new(payload).unwrap();
            sender_ipv6_address = Some(ipv6_packet.get_source());
            info!("Router Address received: {}", sender_ipv6_address.unwrap());
            if let Some(icmp_packet) = Icmpv6Packet::new(ipv6_packet.payload()) {
                if icmp_packet.get_icmpv6_type() == Icmpv6Types::RouterAdvert {
                    if let Some(router_advert) = RouterAdvertPacket::new(ipv6_packet.payload()) {
                        info!("Router Flags: {}", router_advert.get_flags());
                        if (router_advert.get_flags() & 0xC0) == 0xC0 || ignore_ra_flag {
                            break;
                        }
                    } else {
                        warn!("Failed to parse Router Advertisement packet");
                    }
                } else {
                    warn!("Received ICMPv6 type: {:?}", icmp_packet.get_icmpv6_type());
                }
            } else {
                warn!("Failed to parse as ICMPv6 Packet");
            }
        }
    }
    sender_ipv6_address
}

pub async fn run_dhcpv6_client(
    interface_name: String,
) -> Result<Ipv6Addr, Box<dyn std::error::Error>> {
    let chaddr = vec![
        29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44,
    ];
    let mut rng = thread_rng();
    let random_xid: [u8; 3] = rng.gen();
    let local_address = format!("[{}]:546", Ipv6Addr::UNSPECIFIED)
        .parse::<SocketAddr>()
        .unwrap();
    let multicast_address = "[FF02::1:2]:547".parse::<SocketAddr>().unwrap();
    let mut ia_addr_confirm: Option<DhcpOption> = None;

    let socket = UdpSocket::bind(local_address).await?;

    let mut msg = Message::new(MessageType::Solicit);
    msg.opts_mut().insert(DhcpOption::ClientId(chaddr.clone()));
    msg.opts_mut().insert(DhcpOption::ElapsedTime(0));
    msg.set_xid(random_xid);

    let mut oro = ORO { opts: Vec::new() };
    oro.opts.push(OptionCode::DomainNameServers);
    oro.opts.push(OptionCode::DomainSearchList);
    oro.opts.push(OptionCode::ClientFqdn);
    oro.opts.push(OptionCode::SntpServers);

    msg.opts_mut().insert(DhcpOption::ORO(oro));

    let ia_addr_instance = IAAddr {
        addr: Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0),
        preferred_life: 3000,
        valid_life: 5000,
        opts: DhcpOptions::default(),
    };

    let mut iana_opts = DhcpOptions::default();
    iana_opts.insert(DhcpOption::IAAddr(ia_addr_instance));

    let iana_instance = IANA {
        id: 123,
        t1: 3600,
        t2: 7200,
        opts: iana_opts,
    };

    msg.opts_mut().insert(DhcpOption::IANA(iana_instance));

    let mut buf = Vec::new();
    let mut encoder = Encoder::new(&mut buf);
    msg.encode(&mut encoder)?;
    socket.send_to(&buf, multicast_address).await?;

    let mut recv_buf = [0; 1500];
    loop {
        let (size, _) = socket.recv_from(&mut recv_buf).await?;
        let response = Message::decode(&mut dhcproto::v6::Decoder::new(&recv_buf[..size]))?;
        let mut serverid: Option<&DhcpOption> = None;
        let mut ia_addr: Option<&DhcpOption> = None;

        match response.msg_type() {
            MessageType::Advertise => {
                info!("DHCPv6 processing in progress...");
                if let Some(DhcpOption::IANA(iana)) = response.opts().get(OptionCode::IANA) {
                    if let Some(ia_addr_opt) = iana.opts.get(OptionCode::IAAddr) {
                        ia_addr = Some(ia_addr_opt);
                    }
                }
                if let Some(server_option) = response.opts().get(OptionCode::ServerId) {
                    serverid = Some(server_option);
                }

                let mut request_msg = Message::new(MessageType::Request);
                request_msg.set_xid(random_xid);
                request_msg
                    .opts_mut()
                    .insert(DhcpOption::ClientId(chaddr.clone()));
                request_msg.opts_mut().insert(DhcpOption::ElapsedTime(0));
                if let Some(DhcpOption::ServerId(duid)) = serverid {
                    request_msg
                        .opts_mut()
                        .insert(DhcpOption::ServerId((*duid).clone()));
                } else {
                    warn!("Server ID was not found or not a ServerId type.");
                }

                if let Some(DhcpOption::IAAddr(ia_a)) = ia_addr {
                    let ia_addr_instance = IAAddr {
                        addr: ia_a.addr,
                        preferred_life: 3000,
                        valid_life: 5000,
                        opts: DhcpOptions::default(),
                    };
                    let mut iana_opts = DhcpOptions::default();
                    iana_opts.insert(DhcpOption::IAAddr(ia_addr_instance));

                    let iana_instance = IANA {
                        id: 123,
                        t1: 3600,
                        t2: 7200,
                        opts: iana_opts,
                    };
                    request_msg
                        .opts_mut()
                        .insert(DhcpOption::IANA(iana_instance));
                } else {
                    warn!("No ip was not found in Advertise message");
                }

                buf.clear();
                request_msg.encode(&mut Encoder::new(&mut buf))?;
                socket.send_to(&buf, multicast_address).await?;
            }
            MessageType::Reply => {
                if let Some(DhcpOption::IANA(iana)) = response.opts().get(OptionCode::IANA) {
                    if let Some(ia_addr_opt) = iana.opts.get(OptionCode::IAAddr) {
                        ia_addr_confirm = Some((*ia_addr_opt).clone());
                    }
                }

                let mut confirm_msg = Message::new(MessageType::Confirm);
                confirm_msg.set_xid(random_xid);
                buf.clear();
                confirm_msg.encode(&mut Encoder::new(&mut buf))?;
                socket.send_to(&buf, multicast_address).await?;

                break;
            }
            _ => {
                // Ignore other message types
                continue;
            }
        }
    }

    if let Some(DhcpOption::IAAddr(ia_a)) = ia_addr_confirm {
        let (connection, handle, _) = new_connection()?;
        tokio::spawn(connection);

        set_ipv6_address(&handle, &interface_name, ia_a.addr, 128).await?;
        info!(
            "DHCPv6 processing finished, setting ipv6 address {}",
            ia_a.addr
        );
        return Ok(ia_a.addr);
    }

    Err("No valid address received".into())
}

pub async fn set_ipv6_address(
    handle: &Handle,
    interface_name: &str,
    ipv6_addr: Ipv6Addr,
    pfx_len: u8,
) -> Result<(), Error> {
    let mut links = handle
        .link()
        .get()
        .match_name(interface_name.to_string())
        .execute();
    let link = match links.try_next().await {
        Ok(Some(link)) => link,
        Ok(None) => return Err(Error::RequestFailed),
        Err(e) => return Err(e),
    };

    let address = ipv6_addr;

    handle
        .address()
        .add(link.header.index, address.into(), pfx_len)
        .execute()
        .await
}

pub async fn set_ipv6_gateway(
    handle: &Handle,
    interface_name: &str,
    ipv6_gateway: Ipv6Addr,
) -> Result<(), Error> {
    let mut links = handle
        .link()
        .get()
        .match_name(interface_name.to_string())
        .execute();
    let link = match links.try_next().await {
        Ok(Some(link)) => link,
        Ok(None) => return Err(Error::RequestFailed),
        Err(e) => return Err(e),
    };

    let mut route_add_request = handle.route().add();

    let route_msg = route_add_request.message_mut();
    route_msg.header.address_family = AddressFamily::Inet6;
    route_msg.header.scope = RouteScope::Universe;
    route_msg.header.protocol = RouteProtocol::Static;
    route_msg.header.kind = RouteType::Unicast;
    route_msg.header.destination_prefix_length = 0; // Default route
    route_msg
        .attributes
        .push(RouteAttribute::Gateway(RouteAddress::Inet6(ipv6_gateway)));
    route_msg
        .attributes
        .push(RouteAttribute::Oif(link.header.index));

    route_add_request.execute().await
}
