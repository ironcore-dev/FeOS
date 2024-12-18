use dhcproto::v6::Status::{NoAddrsAvail, NoBinding};
use dhcproto::v6::*;
use futures::stream::TryStreamExt;
use log::{debug, error, info, warn};
use netlink_packet_route::route::{
    RouteAddress, RouteAttribute, RouteHeader, RouteProtocol, RouteScope, RouteType,
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
use std::collections::HashMap;
use std::io;
use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6};
use std::thread::sleep;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::task;

#[derive(Clone)]
pub struct IpRange {
    pub start: Ipv6Addr,
    pub end: Ipv6Addr,
}

#[derive(Debug, Clone)]
struct ClientInfo {
    duid: Vec<u8>,
    mac: Vec<u8>,
}

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

    let (mut tx, mut _rx) = match datalink::channel(&interface, Default::default()) {
        Ok(Ethernet(tx, rx)) => (tx, rx),
        Ok(_) => {
            error!("Unhandled channel type");
            return;
        }
        Err(e) => {
            error!("Error creating channel: {}", e);
            return;
        }
    };

    let mut packet_buffer = [0u8; 86];
    let mut ethernet_packet = match MutableEthernetPacket::new(&mut packet_buffer) {
        Some(packet) => packet,
        None => {
            error!("Failed to create Ethernet packet");
            return;
        }
    };

    ethernet_packet.set_destination(MacAddr::broadcast());
    ethernet_packet.set_source(match interface.mac {
        Some(mac) => mac,
        None => {
            error!("Interface MAC address not available");
            return;
        }
    });
    ethernet_packet.set_ethertype(EtherTypes::Ipv6);

    let mut ipv6_and_icmp_buffer = [0u8; 72];

    let mut ipv6_packet = match MutableIpv6Packet::new(&mut ipv6_and_icmp_buffer[..40]) {
        Some(packet) => packet,
        None => {
            error!("Failed to create IPv6 packet");
            return;
        }
    };
    ipv6_packet.set_version(6);
    ipv6_packet.set_next_header(IpNextHeaderProtocols::Icmpv6);
    ipv6_packet.set_payload_length(32);
    ipv6_packet.set_hop_limit(255);
    ipv6_packet.set_source(*src_address);
    ipv6_packet.set_destination(*target_address);

    let mut icmp_packet = match MutableIcmpv6Packet::new(&mut ipv6_and_icmp_buffer[40..]) {
        Some(packet) => packet,
        None => {
            error!("Failed to create ICMPv6 packet");
            return;
        }
    };
    icmp_packet.set_icmpv6_type(Icmpv6Types::NeighborSolicit);
    icmp_packet.set_icmpv6_code(Icmpv6Code(0));
    icmp_packet.set_checksum(0);

    let mut icmp_payload = [0u8; 28];
    icmp_payload[4..20].copy_from_slice(&target_address.octets());
    icmp_payload[20] = 1;
    icmp_payload[21] = 1;
    icmp_payload[22..28].copy_from_slice(&match interface.mac {
        Some(mac) => mac.octets(),
        None => {
            error!("Interface MAC address not available");
            return;
        }
    });
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
    sleep(Duration::from_secs(5));
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

pub struct PrefixInfo {
    pub prefix: Ipv6Addr,
    pub prefix_length: u8,
}

pub struct Dhcpv6Result {
    pub address: Ipv6Addr,
    pub prefix: Option<PrefixInfo>,
}

pub async fn run_dhcpv6_client(
    interface_name: String,
) -> Result<Dhcpv6Result, Box<dyn std::error::Error>> {
    let chaddr = vec![
        29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44,
    ];
    let random_xid: [u8; 3] = [0x12, 0x34, 0x56];
    let multicast_address = "[FF02::1:2]:547".parse::<SocketAddr>().unwrap();
    let mut ia_addr_confirm: Option<DhcpOption> = None;
    let mut ia_pd_confirm: Option<IAPrefix> = None;

    let interface_index = get_interface_index(interface_name.clone()).await?;
    let socket = create_multicast_socket(interface_name.clone(), interface_index, 546)?;

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

pub async fn get_interface_index(interface_name: String) -> io::Result<u32> {
    task::spawn_blocking(move || {
        if_nametoindex(interface_name.as_str()).map_err(|e| {
            io::Error::new(io::ErrorKind::Other, format!("Error getting index: {}", e))
        })
    })
    .await?
}

fn create_multicast_socket(
    interface_name: String,
    interface_index: u32,
    lport: u16,
) -> Result<UdpSocket, Box<dyn std::error::Error>> {
    let multicast_addr: Ipv6Addr = "ff02::1:2".parse().unwrap();

    let socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    socket.set_reuse_port(true)?;
    socket.set_multicast_if_v6(interface_index)?;
    socket.join_multicast_v6(&multicast_addr, interface_index)?;

    socket.bind(&SockAddr::from(SocketAddr::V6(SocketAddrV6::new(
        Ipv6Addr::UNSPECIFIED,
        lport,
        0,
        0,
    ))))?;

    socket.bind_device(Some(interface_name.as_bytes()))?;

    socket.set_nonblocking(true)?;

    let udp_socket = UdpSocket::from_std(socket.into())?;

    Ok(udp_socket)
}

pub async fn run_dhcpv6_server(
    interface_name: String,
    ip_range: IpRange,
) -> Result<(), Box<dyn std::error::Error>> {
    let interface_index = get_interface_index(interface_name.clone()).await?;
    let socket = create_multicast_socket(interface_name.clone(), interface_index, 547)?;

    info!(
        "DHCPv6 server listening on interface {} [::]:547 and joined multicast group ff02::1:2",
        interface_name
    );

    let mut allocations: HashMap<Vec<u8>, Ipv6Addr> = HashMap::new();
    let mut available_addresses: Vec<Ipv6Addr> = generate_ip_pool(ip_range);
    let server_duid = vec![0x00, 0x01, 0x00, 0x01, 0x00, 0x0c, 0x29, 0x3e, 0x5c, 0x3d];
    let mut buf = [0u8; 1500];

    loop {
        let (size, client_addr) = socket.recv_from(&mut buf).await?;

        let message = match Message::decode(&mut Decoder::new(&buf[..size])) {
            Ok(msg) => msg,
            Err(e) => {
                info!("Failed to decode message: {}", e);
                continue;
            }
        };

        debug!("Received DHCPv6 message: {:?}", message.msg_type());

        let client_duid_option = message.opts().get(OptionCode::ClientId);
        let client_info = match client_duid_option {
            Some(DhcpOption::ClientId(duid)) => {
                let duid = duid.clone();
                let mac = extract_mac_from_duid(&duid).unwrap_or_else(|| duid.clone());
                ClientInfo { duid, mac }
            }
            _ => {
                debug!("Solicit/Request message without valid Client ID");
                continue;
            }
        };

        match message.msg_type() {
            MessageType::Solicit => {
                handle_solicit(
                    &socket,
                    &message,
                    &client_info,
                    &mut allocations,
                    &mut available_addresses,
                    &server_duid,
                    client_addr,
                )
                .await?;
            }
            MessageType::Request => {
                handle_request(
                    &socket,
                    &message,
                    &client_info,
                    &mut allocations,
                    &server_duid,
                    client_addr,
                )
                .await?;
            }
            _ => {
                info!("Unhandled DHCPv6 message type: {:?}", message.msg_type());
                continue;
            }
        }
    }
}

async fn handle_solicit(
    socket: &UdpSocket,
    message: &Message,
    client_info: &ClientInfo,
    allocations: &mut HashMap<Vec<u8>, Ipv6Addr>,
    available_addresses: &mut Vec<Ipv6Addr>,
    server_duid: &[u8],
    client_addr: SocketAddr,
) -> Result<(), Box<dyn std::error::Error>> {
    let iana_option = message.opts().get(OptionCode::IANA);
    let iana = match iana_option {
        Some(DhcpOption::IANA(iana)) => iana,
        _ => {
            info!("Solicit message without IA_NA option");
            return Ok(());
        }
    };

    let iaid = iana.id;

    let rapid_commit_requested = message.opts().get(OptionCode::RapidCommit).is_some();

    let option_request = message.opts().get(OptionCode::ORO);

    let rapid_commit_in_oro = if let Some(DhcpOption::ORO(option_codes)) = option_request {
        option_codes.opts.contains(&OptionCode::RapidCommit)
    } else {
        false
    };

    let rapid_commit = rapid_commit_requested && rapid_commit_in_oro;

    if rapid_commit {
        debug!(
            "Rapid Commit option detected in Solicit message from client DUID {:?}",
            client_info.duid
        );

        let allocated_ip = if let Some(ip) = allocations.get(&client_info.mac) {
            *ip
        } else if let Some(ip) = available_addresses.pop() {
            allocations.insert(client_info.mac.clone(), ip);
            ip
        } else {
            let reply_msg = create_reply_message(
                message.xid(),
                server_duid,
                &client_info.duid,
                None,
                Some(StatusCode {
                    status: NoAddrsAvail,
                    msg: "No addresses available".into(),
                }),
            );

            let mut send_buf = Vec::new();
            reply_msg.encode(&mut Encoder::new(&mut send_buf))?;
            socket.send_to(&send_buf, &client_addr).await?;

            debug!(
                "No available IPs. Sent Reply with NoAddrsAvail to client DUID {:?}",
                client_info.duid
            );
            return Ok(());
        };

        info!(
            "Assigning IP {} to client DUID {:?} via Rapid Commit",
            allocated_ip, client_info.duid
        );

        let ia_addr = IAAddr {
            addr: allocated_ip,
            preferred_life: 0xFFFFFFFF,
            valid_life: 0xFFFFFFFF,
            opts: DhcpOptions::default(),
        };

        let mut iana_opts = DhcpOptions::default();
        iana_opts.insert(DhcpOption::IAAddr(ia_addr));

        let iana = IANA {
            id: iaid,
            t1: 0xFFFFFFFF,
            t2: 0xFFFFFFFF,
            opts: iana_opts,
        };

        let mut reply_msg = create_reply_message(
            message.xid(),
            server_duid,
            &client_info.duid,
            Some(iana),
            Some(StatusCode {
                status: dhcproto::v6::Status::Success,
                msg: "Success".into(),
            }),
        );

        reply_msg.opts_mut().insert(DhcpOption::RapidCommit);

        let mut send_buf = Vec::new();
        reply_msg.encode(&mut Encoder::new(&mut send_buf))?;
        socket.send_to(&send_buf, &client_addr).await?;

        debug!(
            "Sent Reply message to client DUID {:?} with IP {} via Rapid Commit",
            client_info.duid, allocated_ip
        );

        return Ok(());
    }

    debug!(
        "Handling Solicit without Rapid Commit for client DUID (Not implemented yet) {:?}",
        client_info.duid
    );
    //TODO Advertise handling

    Ok(())
}

async fn handle_request(
    socket: &UdpSocket,
    message: &Message,
    client_info: &ClientInfo,
    allocations: &mut HashMap<Vec<u8>, Ipv6Addr>,
    server_duid: &[u8],
    client_addr: SocketAddr,
) -> Result<(), Box<dyn std::error::Error>> {
    let iana_option = message.opts().get(OptionCode::IANA);
    let iana = match iana_option {
        Some(DhcpOption::IANA(iana)) => iana,
        _ => {
            info!("Request message without IA_NA option");
            return Ok(());
        }
    };

    let iaid = iana.id;

    let ia_addr_option = iana.opts.get(OptionCode::IAAddr);
    let requested_ip = match ia_addr_option {
        Some(DhcpOption::IAAddr(ia_addr)) => ia_addr.addr,
        _ => {
            info!("IA_NA option without IAAddr");
            return Ok(());
        }
    };

    if let Some(allocated_ip) = allocations.get(&client_info.mac) {
        if *allocated_ip == requested_ip {
            let ia_addr = IAAddr {
                addr: requested_ip,
                preferred_life: 0xFFFFFFFF,
                valid_life: 0xFFFFFFFF,
                opts: DhcpOptions::default(),
            };

            let mut iana_opts = DhcpOptions::default();
            iana_opts.insert(DhcpOption::IAAddr(ia_addr));

            let iana = IANA {
                id: iaid,
                t1: 0xFFFFFFFF,
                t2: 0xFFFFFFFF,
                opts: iana_opts,
            };

            let reply_msg = create_reply_message(
                message.xid(),
                server_duid,
                &client_info.duid,
                Some(iana),
                Some(StatusCode {
                    status: dhcproto::v6::Status::Success,
                    msg: "Success".into(),
                }),
            );

            let mut send_buf = Vec::new();
            reply_msg.encode(&mut Encoder::new(&mut send_buf))?;
            socket.send_to(&send_buf, &client_addr).await?;

            info!(
                "Confirmed IP {} for client DUID {:?}",
                requested_ip, client_info.duid
            );
        } else {
            let reply_msg = create_reply_message(
                message.xid(),
                server_duid,
                &client_info.duid,
                None,
                Some(StatusCode {
                    status: NoBinding,
                    msg: "No binding for requested IP".into(),
                }),
            );

            let mut send_buf = Vec::new();
            reply_msg.encode(&mut Encoder::new(&mut send_buf))?;
            socket.send_to(&send_buf, &client_addr).await?;

            info!(
                "No binding for requested IP {} from client DUID {:?}",
                requested_ip, client_info.duid
            );
        }
    } else {
        let reply_msg = create_reply_message(
            message.xid(),
            server_duid,
            &client_info.duid,
            None,
            Some(StatusCode {
                status: NoBinding,
                msg: "No binding for requested IP".into(),
            }),
        );

        let mut send_buf = Vec::new();
        reply_msg.encode(&mut Encoder::new(&mut send_buf))?;
        socket.send_to(&send_buf, &client_addr).await?;

        info!(
            "No binding for requested IP {} from client DUID {:?}",
            requested_ip, client_info.duid
        );
    }

    Ok(())
}

fn create_reply_message(
    xid: [u8; 3],
    server_duid: &[u8],
    client_duid: &[u8],
    iana: Option<IANA>,
    status_code: Option<StatusCode>,
) -> Message {
    let mut reply_msg = Message::new(MessageType::Reply);
    reply_msg.set_xid(xid);

    reply_msg
        .opts_mut()
        .insert(DhcpOption::ServerId(server_duid.to_vec()));

    reply_msg
        .opts_mut()
        .insert(DhcpOption::ClientId(client_duid.to_vec()));

    if let Some(iana) = iana {
        reply_msg.opts_mut().insert(DhcpOption::IANA(iana));
    }

    if let Some(status) = status_code {
        reply_msg.opts_mut().insert(DhcpOption::StatusCode(status));
    }

    reply_msg
}

fn generate_ip_pool(ip_range: IpRange) -> Vec<Ipv6Addr> {
    let start_u128 = u128::from(ip_range.start);
    let end_u128 = u128::from(ip_range.end);

    let range_size = end_u128 - start_u128 + 1;
    if range_size > 1000 {
        error!("IP range too large");
    }

    let mut ips = Vec::new();
    for addr_u128 in start_u128..=end_u128 {
        let ip = Ipv6Addr::from(addr_u128);
        ips.push(ip);
    }
    ips
}

fn extract_mac_from_duid(duid: &[u8]) -> Option<Vec<u8>> {
    if duid.len() < 2 {
        return None;
    }
    let duid_type = u16::from_be_bytes([duid[0], duid[1]]);
    match duid_type {
        1 => {
            if duid.len() < 8 {
                return None;
            }
            let mac = duid[8..].to_vec();
            Some(mac)
        }
        3 => {
            if duid.len() < 4 {
                return None;
            }
            let mac = duid[4..].to_vec();
            Some(mac)
        }
        _ => {
            // Other DUID types
            None
        }
    }
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
    route_msg.header.kind = route_type;
    route_msg.header.destination_prefix_length = prefix_length;
    route_msg.header.table = RouteHeader::RT_TABLE_MAIN;

    route_msg
        .attributes
        .push(RouteAttribute::Destination(RouteAddress::from(destination)));

    if route_type == RouteType::Unicast {
        if let Some(gw) = gateway {
            route_msg
                .attributes
                .push(RouteAttribute::Gateway(RouteAddress::from(gw)));
        }
    }

    route_msg
        .attributes
        .push(RouteAttribute::Oif(link.header.index));

    route_msg.attributes.push(RouteAttribute::Priority(metric));

    route_add_request.execute().await
}

pub fn adjust_base_ip(base_ip: Ipv6Addr, prefix_length: u8, prefix_count: u16) -> Ipv6Addr {
    let base_ip_u128: u128 = base_ip.into();
    let subnet_shift = 128 - (prefix_length as u32 + 16);
    let subnet_mask: u128 = !(0xFFFFu128 << subnet_shift);
    let adjusted_base_ip_u128 =
        (base_ip_u128 & subnet_mask) | ((prefix_count as u128) << subnet_shift);
    Ipv6Addr::from(adjusted_base_ip_u128)
}

pub fn add_to_ipv6(addr: Ipv6Addr, prefix_length: u8, increment: u128) -> Ipv6Addr {
    let addr_u128: u128 = addr.into();
    let host_bits = 128 - prefix_length as usize;
    let host_mask: u128 = if host_bits == 0 {
        0
    } else {
        (1u128 << host_bits) - 1
    };
    let host_part = addr_u128 & host_mask;
    let new_host = host_part.wrapping_add(increment);

    if new_host > host_mask {
        error!("Host address overflow");
    }

    let new_addr = (addr_u128 & !host_mask) | (new_host & host_mask);
    Ipv6Addr::from(new_addr)
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
    route_msg.header.destination_prefix_length = 0;
    route_msg
        .attributes
        .push(RouteAttribute::Gateway(RouteAddress::Inet6(ipv6_gateway)));
    route_msg
        .attributes
        .push(RouteAttribute::Oif(link.header.index));

    route_add_request.execute().await
}
