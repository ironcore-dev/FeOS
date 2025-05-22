// Integration test for run_dhcpv6_client using a veth pair and the mock DHCPv6 server
use feos::ringbuffer::{RingBuffer, init_logger};
use std::process::{Command, Child};

fn setup_veth_pair() {
    let del1 = Command::new("sudo").args(["ip", "link", "del", "veth-dhcp-cli"]).output();
    let del2 = Command::new("sudo").args(["ip", "link", "del", "veth-dhcp-srv"]).output();
    eprintln!("del veth-dhcp-cli: {:?}", del1);
    eprintln!("del veth-dhcp-srv: {:?}", del2);
    let add = Command::new("sudo").args(["ip", "link", "add", "veth-dhcp-cli", "type", "veth", "peer", "name", "veth-dhcp-srv"]).output();
    assert!(add.as_ref().map(|o| o.status.success()).unwrap_or(false), "Failed to add veth pair: {:?}", add);
    let up1 = Command::new("sudo").args(["ip", "link", "set", "veth-dhcp-cli", "up"]).output();
    assert!(up1.as_ref().map(|o| o.status.success()).unwrap_or(false), "Failed to set veth-dhcp-cli up: {:?}", up1);
    let up2 = Command::new("sudo").args(["ip", "link", "set", "veth-dhcp-srv", "up"]).output();
    assert!(up2.as_ref().map(|o| o.status.success()).unwrap_or(false), "Failed to set veth-dhcp-srv up: {:?}", up2);
    let addr1 = Command::new("sudo").args(["ip", "-6", "addr", "add", "fe80::1/64", "dev", "veth-dhcp-srv"]).output();
    assert!(addr1.as_ref().map(|o| o.status.success()).unwrap_or(false), "Failed to add IPv6 to veth-dhcp-srv: {:?}", addr1);
    let addr2 = Command::new("sudo").args(["ip", "-6", "addr", "add", "fe80::2/64", "dev", "veth-dhcp-cli"]).output();
    assert!(addr2.as_ref().map(|o| o.status.success()).unwrap_or(false), "Failed to add IPv6 to veth-dhcp-cli: {:?}", addr2);
}

fn cleanup_veth_pair() {
    let _ = Command::new("sudo").args(["ip", "link", "del", "veth-dhcp-cli"]).output();
    let _ = Command::new("sudo").args(["ip", "link", "del", "veth-dhcp-srv"]).output();
}

fn start_dhcpv6_server(interface: &str) -> Child {
    Command::new("sudo")
        .arg("python3")
        .arg("scripts/dhcpv6_server.py")
        .arg("-i")
        .arg(interface)
        .spawn()
        .expect("Failed to start DHCPv6 server")
}

#[tokio::test]
async fn test_run_dhcpv6_client_integration() {
    let buffer = RingBuffer::new(100);
    let _ = init_logger(buffer);
    setup_veth_pair();
    std::thread::sleep(std::time::Duration::from_secs(1)); // Wait for veth setup
    let mut server = start_dhcpv6_server("veth-dhcp-srv");
    std::thread::sleep(std::time::Duration::from_secs(2));

    // Import the function under test
    use feos::network::dhcpv6::run_dhcpv6_client;
    eprintln!("[TEST] Starting DHCPv6 client on veth-dhcp-cli");
    let result = run_dhcpv6_client("veth-dhcp-cli".to_string()).await;
    eprintln!("[TEST] DHCPv6 client result: {:?}", result);

    let _ = server.kill();
    cleanup_veth_pair();
    eprintln!("[TEST] Cleaned up veth pair and killed server");

    assert!(result.is_ok(), "DHCPv6 client did not succeed: {:?}", result);
    let dhcp_result = result.unwrap();
    assert!(dhcp_result.address != std::net::Ipv6Addr::UNSPECIFIED, "No address assigned");
}
