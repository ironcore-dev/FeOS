use dhcproto::v6::*;
use futures::stream::TryStreamExt;
use log::{error, info, warn};
use netlink_packet_route::route::{
    RouteAddress, RouteAttribute, RouteProtocol, RouteScope, RouteType,
};
use netlink_packet_route::AddressFamily;
use nix::net::if_::if_nametoindex;
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
use rtnetlink::{new_connection, Error, Handle};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::io;
use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6};
use std::thread::sleep;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::task;

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
    let interface = match interfaces.into_iter().find(interface_names_match) {
        Some(iface) => iface,
        None => {
            error!("Error getting interface");
            return;
        }
    };

    let (mut tx, _rx) = match datalink::channel(&interface, Default::default()) {
        Ok(Ethernet(tx, rx)) => (tx, rx),
        Ok(_) => {
            error!("Unhandled channel type");
            return;
        }
        Err(e) => {
            error!("Error creating channel: {e}");
            return;
        }
    };

    let mut packet_buffer = [0u8; 86];
    let mut ethernet_packet = MutableEthernetPacket::new(&mut packet_buffer).unwrap();

    ethernet_packet.set_destination(MacAddr::broadcast());
    ethernet_packet.set_source(interface.mac.unwrap());
    ethernet_packet.set_ethertype(EtherTypes::Ipv6);

    let mut ipv6_and_icmp_buffer = [0u8; 72];
    let mut ipv6_packet = MutableIpv6Packet::new(&mut ipv6_and_icmp_buffer[..40]).unwrap();
    ipv6_packet.set_version(6);
    ipv6_packet.set_next_header(IpNextHeaderProtocols::Icmpv6);
    ipv6_packet.set_payload_length(32);
    ipv6_packet.set_hop_limit(255);
    ipv6_packet.set_source(*src_address);
    ipv6_packet.set_destination(*target_address);

    let mut icmp_packet = MutableIcmpv6Packet::new(&mut ipv6_and_icmp_buffer[40..]).unwrap();
    icmp_packet.set_icmpv6_type(Icmpv6Types::NeighborSolicit);
    icmp_packet.set_icmpv6_code(Icmpv6Code(0));
    icmp_packet.set_checksum(0);

    let mut icmp_payload = [0u8; 28];
    icmp_payload[4..20].copy_from_slice(&target_address.octets());
    icmp_payload[20] = 1;
    icmp_payload[21] = 1;
    icmp_payload[22..28].copy_from_slice(&interface.mac.unwrap().octets());
    icmp_packet.set_payload(&icmp_payload);

    let checksum = checksum(
        &Icmpv6Packet::new(icmp_packet.packet()).unwrap(),
        src_address,
        target_address,
    );
    icmp_packet.set_checksum(checksum);

    ethernet_packet.set_payload(&ipv6_and_icmp_buffer);

    if tx
        .send_to(ethernet_packet.packet(), Some(interface.clone()))
        .is_none()
    {
        error!("Failed to send neighbor solicitation");
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

    if tx
        .send_to(ethernet_packet.packet(), Some(interface.clone()))
        .is_none()
    {
        error!("Failed to send router solicitation");
    }
}

pub fn is_dhcpv6_needed(interface_name: String, ignore_ra_flag: bool) -> Option<Ipv6Addr> {
    let mut sender_ipv6_address: Option<Ipv6Addr> = None;
    let interfaces = datalink::interfaces();
    let interface = interfaces
        .into_iter()
        .find(|iface| iface.name == interface_name)?;
    let (mut tx, mut rx) = match datalink::channel(&interface, Default::default()) {
        Ok(Ethernet(tx, rx)) => (tx, rx),
        _ => return None,
    };

    info!("Sending Router Solicitation ...");
    sleep(Duration::from_secs(5));
    send_router_solicitation(&interface, &mut *tx);

    while let Ok(raw_packet) = rx.next() {
        if let Some(eth_packet) = EthernetPacket::new(raw_packet) {
            if eth_packet.get_ethertype() == EtherTypes::Ipv6 {
                if let Some(ipv6_packet) = Ipv6Packet::new(eth_packet.payload()) {
                    sender_ipv6_address = Some(ipv6_packet.get_source());
                    if let Some(icmp_packet) = Icmpv6Packet::new(ipv6_packet.payload()) {
                        if icmp_packet.get_icmpv6_type() == Icmpv6Types::RouterAdvert {
                            if let Some(ra_packet) = RouterAdvertPacket::new(ipv6_packet.payload())
                            {
                                if (ra_packet.get_flags() & 0xC0) == 0xC0 || ignore_ra_flag {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    sender_ipv6_address
}

#[derive(Debug)]
pub struct PrefixInfo {
    pub prefix: Ipv6Addr,
    pub prefix_length: u8,
}

#[derive(Debug)]
pub struct Dhcpv6Result {
    pub address: Ipv6Addr,
    pub prefix: Option<PrefixInfo>,
}

pub async fn run_dhcpv6_client(
    interface_name: String,
) -> Result<Dhcpv6Result, Box<dyn std::error::Error + Send + Sync>> {
    let chaddr = vec![
        29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44,
    ];
    let random_xid: [u8; 3] = [0x12, 0x34, 0x56];
    let multicast_address = "[FF02::1:2]:547".parse::<SocketAddr>().unwrap();
    let mut ia_addr_confirm: Option<DhcpOption> = None;
    let mut ia_pd_confirm: Option<IAPrefix> = None;

    let interface_index = get_interface_index(interface_name.clone()).await?;
    let socket = create_multicast_socket(&interface_name, interface_index, 546)?;

    let mut msg = Message::new(MessageType::Solicit);
    msg.opts_mut().insert(DhcpOption::ClientId(chaddr.clone()));
    msg.opts_mut().insert(DhcpOption::ElapsedTime(0));
    msg.set_xid(random_xid);

    msg.opts_mut().insert(DhcpOption::RapidCommit);

    let mut oro = ORO { opts: Vec::new() };
    oro.opts.push(OptionCode::DomainNameServers);
    oro.opts.push(OptionCode::DomainSearchList);
    oro.opts.push(OptionCode::ClientFqdn);
    oro.opts.push(OptionCode::SntpServers);
    oro.opts.push(OptionCode::RapidCommit);
    oro.opts.push(OptionCode::IAPD);
    oro.opts.push(OptionCode::IAPrefix);

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

    // Request Prefix Delegation
    let iaprefix_instance = IAPrefix {
        preferred_lifetime: 0,
        prefix_len: 80,
        opts: DhcpOptions::default(),
        valid_lifetime: 0,
        prefix_ip: Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0),
    };

    let mut iapd_opts = DhcpOptions::default();
    iapd_opts.insert(DhcpOption::IAPrefix(iaprefix_instance));

    let iapd_instance = IAPD {
        id: 456,
        t1: 3600,
        t2: 7200,
        opts: iapd_opts,
    };

    msg.opts_mut().insert(DhcpOption::IAPD(iapd_instance));

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
        let mut ia_pd: Option<&DhcpOption> = None;

        match response.msg_type() {
            MessageType::Advertise => {
                info!("DHCPv6 processing in progress...");
                if let Some(DhcpOption::IANA(iana)) = response.opts().get(OptionCode::IANA) {
                    if let Some(ia_addr_opt) = iana.opts.get(OptionCode::IAAddr) {
                        ia_addr = Some(ia_addr_opt);
                    }
                }
                if let Some(DhcpOption::IAPD(iapd)) = response.opts().get(OptionCode::IAPD) {
                    if let Some(iaprefix_opt) = iapd.opts.get(OptionCode::IAPrefix) {
                        ia_pd = Some(iaprefix_opt);
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
                    warn!("No IP was found in Advertise message");
                }

                if let Some(DhcpOption::IAPrefix(iaprefix)) = ia_pd {
                    let iapd_instance = IAPD {
                        id: 456,
                        t1: 3600,
                        t2: 7200,
                        opts: {
                            let mut opts = DhcpOptions::default();
                            opts.insert(DhcpOption::IAPrefix((*iaprefix).clone()));
                            opts
                        },
                    };
                    request_msg
                        .opts_mut()
                        .insert(DhcpOption::IAPD(iapd_instance));
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
                if let Some(DhcpOption::IAPD(iapd)) = response.opts().get(OptionCode::IAPD) {
                    if let Some(DhcpOption::IAPrefix(iaprefix)) =
                        iapd.opts.get(OptionCode::IAPrefix)
                    {
                        ia_pd_confirm = Some((*iaprefix).clone());
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
            "DHCPv6 processing finished, setting IPv6 address {}",
            ia_a.addr
        );

        let prefix_info = ia_pd_confirm.map(|iaprefix| PrefixInfo {
            prefix: iaprefix.prefix_ip,
            prefix_length: iaprefix.prefix_len,
        });

        if let Some(ref pfx) = prefix_info {
            info!(
                "Received delegated prefix {} with length {}",
                pfx.prefix, pfx.prefix_length
            );
        } else {
            info!("No prefix delegation received.");
        }

        return Ok(Dhcpv6Result {
            address: ia_a.addr,
            prefix: prefix_info,
        });
    }

    Err("No valid address received".into())
}

pub async fn set_ipv6_address(
    handle: &Handle,
    interface_name: &str,
    ipv6_addr: Ipv6Addr,
    pfx_len: u8,
) -> Result<(), Error> {
    let link = handle
        .link()
        .get()
        .match_name(interface_name.to_string())
        .execute()
        .try_next()
        .await?
        .ok_or(Error::RequestFailed)?;
    handle
        .address()
        .add(link.header.index, ipv6_addr.into(), pfx_len)
        .execute()
        .await
}

pub async fn get_interface_index(interface_name: String) -> io::Result<u32> {
    task::spawn_blocking(move || {
        if_nametoindex(interface_name.as_str())
            .map_err(|e| io::Error::other(format!("Error getting index: {e}")))
    })
    .await?
}

fn create_multicast_socket(
    interface_name: &str,
    interface_index: u32,
    lport: u16,
) -> Result<UdpSocket, Box<dyn std::error::Error + Send + Sync>> {
    let socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    socket.set_multicast_if_v6(interface_index)?;
    socket.join_multicast_v6(&"ff02::1:2".parse()?, interface_index)?;
    socket.bind(&SockAddr::from(SocketAddrV6::new(
        Ipv6Addr::UNSPECIFIED,
        lport,
        0,
        0,
    )))?;
    socket.bind_device(Some(interface_name.as_bytes()))?;
    socket.set_nonblocking(true)?;
    Ok(UdpSocket::from_std(socket.into())?)
}

pub async fn add_ipv6_route(
    handle: &Handle,
    interface_name: &str,
    destination: Ipv6Addr,
    prefix_length: u8,
    gateway: Option<Ipv6Addr>,
    metric: u32,
    route_type: RouteType,
) -> Result<(), Error> {
    let link = handle
        .link()
        .get()
        .match_name(interface_name.to_string())
        .execute()
        .try_next()
        .await?
        .ok_or(Error::RequestFailed)?;
    let mut req = handle.route().add();
    let msg = req.message_mut();
    msg.header.address_family = AddressFamily::Inet6;
    msg.header.scope = RouteScope::Universe;
    msg.header.protocol = RouteProtocol::Static;
    msg.header.kind = route_type;
    msg.header.destination_prefix_length = prefix_length;
    msg.attributes
        .push(RouteAttribute::Destination(RouteAddress::from(destination)));
    if route_type == RouteType::Unicast {
        if let Some(gw) = gateway {
            msg.attributes
                .push(RouteAttribute::Gateway(RouteAddress::from(gw)));
        }
    }
    msg.attributes.push(RouteAttribute::Oif(link.header.index));
    msg.attributes.push(RouteAttribute::Priority(metric));
    req.execute().await
}

pub async fn set_ipv6_gateway(
    handle: &Handle,
    interface_name: &str,
    ipv6_gateway: Ipv6Addr,
) -> Result<(), Error> {
    let link = handle
        .link()
        .get()
        .match_name(interface_name.to_string())
        .execute()
        .try_next()
        .await?
        .ok_or(Error::RequestFailed)?;
    let mut req = handle.route().add();
    let msg = req.message_mut();
    msg.header.address_family = AddressFamily::Inet6;
    msg.header.scope = RouteScope::Universe;
    msg.header.protocol = RouteProtocol::Static;
    msg.header.kind = RouteType::Unicast;
    msg.header.destination_prefix_length = 0;
    msg.attributes
        .push(RouteAttribute::Gateway(RouteAddress::Inet6(ipv6_gateway)));
    msg.attributes.push(RouteAttribute::Oif(link.header.index));
    req.execute().await
}
