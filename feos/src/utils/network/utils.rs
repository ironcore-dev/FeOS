use super::dhcpv6::*;
use futures::stream::TryStreamExt;
use log::{error, info, warn};
use netlink_packet_route::route::RouteType;
use rtnetlink::new_connection;
use std::fs::File;
use std::io::Write;
use std::net::Ipv6Addr;
use tokio::time::{sleep, Duration};

pub const INTERFACE_NAME: &str = "eth0";

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
