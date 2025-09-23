# Nested Virtualization Test Environment Setup Guide

This guide explains how to set up a development environment on a Linux host to run FeOS in a KVM virtual machine, where FeOS can, in turn, manage its own nested virtual machines. This is essential for testing the `vm-service` and its interaction with hypervisors like cloud-hypervisor.

The setup involves four key services running on the host and serving the main FeOS VM running on the `vm-br0` bridge:
1.  **radvd**: Provides IPv6 router advertisements for stateful address configuration.
2.  **DHCPv6 Server**: Assigns a stateful IPv6 address to the FeOS VM.
3.  **dnsmasq**: A DNS server that redirects OCI registry requests to a local mirror.
4.  **OCI Mirror**: A local pull-through registry cache to serve container images.

## 1. Prerequisites

### Enable Nested Virtualization

First, you must enable nested virtualization on your host machine. This is a kernel module setting for KVM.

**A. Check if Nested Virtualization is Enabled**

For Intel CPUs, run:
```bash
cat /sys/module/kvm_intel/parameters/nested
```

For AMD CPUs, run:
```bash
cat /sys/module/kvm_amd/parameters/nested
```

If the output is `Y` or `1`, it's already enabled, and you can skip to the next section. If it's `N` or `0`, proceed to the next step.

**B. Enable Nested Virtualization Persistently**

You need to create a `.conf` file in `/etc/modprobe.d/`.

For **Intel** CPUs, create `/etc/modprobe.d/kvm.conf` with the following content:

```
options kvm-intel nested=1
```

For **AMD** CPUs, create `/etc/modprobe.d/kvm.conf` with the following content:
```
options kvm-amd nested=1
```

**C. Apply the Changes**

Reload the KVM kernel module to apply the setting.
> **Warning:** This will stop all running VMs on your host.

For **Intel** CPUs:
```bash
sudo modprobe -r kvm_intel
sudo modprobe kvm_intel
```

For **AMD** CPUs:
```bash
sudo modprobe -r kvm_amd
sudo modprobe kvm_amd
```
Verify again that the setting is now active.

### Build FeOS and Prepare Network

Before setting up the services, ensure you have built the FeOS UKI and created the `vm-br0` bridge as described in the main `README.md`:

```bash
# Build the project
make uki

# Create the network bridge
make network
```

## 2. Environment Services Setup

The following services should be run in separate terminals. They will provide the necessary network infrastructure for the FeOS VM to boot and function correctly.

The IPv6 Range `2a10:afc0:e01f:1::/64` and its sub-ranges are used as an example here. You can choose a different /64 prefix, but ensure it does not conflict with existing networks. 

### RADV Setup (Router Advertisement)

This service advertises the network prefix and tells clients to obtain IPv6 addresses and other configuration (like DNS) from a DHCPv6 server.

1.  Create a configuration file named `radvd.conf`:
    ```
    # Configuration for the vm-br0 bridge interface
    interface vm-br0 {
        # We are sending router advertisements
        AdvSendAdvert on;

        # Set the RA interval between 10 and 30 seconds
        MaxRtrAdvInterval 30;
        MinRtrAdvInterval 10;

        # Set the 'Managed' (M) flag to 'on'.
        # This tells clients to get their IPv6 address from a DHCPv6 server (Stateful DHCPv6).
        AdvManagedFlag on;

        # Set the 'Other Configuration' (O) flag to 'on'.
        # This tells clients to get other info (like DNS servers) from a DHCPv6 server.
        AdvOtherConfigFlag on;

        # This is the prefix we are advertising to clients on the network.
        # It must be a /64 prefix.
        prefix 2a10:afc0:e01f:1::/64 {
            # This prefix is available on this link
            AdvOnLink on;

            # This turns OFF Stateless Address Autoconfiguration (SLAAC).
            # Clients MUST use DHCPv6 to get an address. This works with AdvManagedFlag on.
            AdvAutonomous off;
        };
    };
    ```

2.  Run `radvd` with this configuration:
    ```bash
    sudo /usr/sbin/radvd -n -C ./radvd.conf
    ```

### DHCPv6 Server Setup (ISC dhcpd)

The `radvd` service, with the `AdvManagedFlag` set, instructs clients to contact a DHCPv6 server for a stateful IPv6 address. The following ISC DHCP server configuration provides this service.

1.  Create a configuration file named `dhcpd6.conf`:
    ```
    default-lease-time 2592000;
    preferred-lifetime 604800;

    # T1, the delay before Renew (1 hour)
    option dhcp-renewal-time 3600;
    
    # T2, the delay before Rebind (2 hours)
    option dhcp-rebinding-time 7200;
    
    # Enable RFC 5007 support
    allow leasequery;
    
    # Global definitions for name server and domain search
    option dhcp6.name-servers 3ffe:501:ffff:100:200:ff:fe00:3f3e;
    option dhcp6.domain-search "test.example.com", "example.com";
    
    # Delay before information-request refresh (6 hours)
    option dhcp6.info-refresh-time 21600;
    
    # Subnet for the vm-br0 bridge
    subnet6 2a10:afc0:e01f:1::ffff:0/112 {
        # The range of addresses to lease to clients
        range6 2a10:afc0:e01f:1::ffff:1 2a10:afc0:e01f:1::ffff:ffff;
    
        # Example of prefix delegation (optional)
        prefix6 2001:db8:0:100:: 2001:db8:0:1ff:: /64;
    
        # Provide DNS servers to clients in this subnet
        option dhcp6.name-servers 2001:4860:4860::6464;
    }
    ```

2.  Run the DHCPv6 server, attaching it to the `vm-br0` interface:
    ```bash
    sudo /usr/sbin/dhcpd -6 -q -cf ./dhcpd6.conf vm-br0
    ```

### DNS Setup (dnsmasq)

This `dnsmasq` instance will act as a DNS server on the `vm-br0` bridge. Its primary role is to intercept DNS queries for `ghcr.io` and return the IP address of our local OCI mirror. All other queries are forwarded to public DNS servers.

1.  Create a configuration file named `dnsmasq.conf`:
    ```
    # Listen only on the bridge interface for security
    interface=vm-br0
    bind-interfaces

    # Don't read /etc/resolv.conf
    no-resolv

    # Intercept requests for ghcr.io and return our local bridge IP.
    # The format is address=/<domain>/<ip_address>
    address=/ghcr.io/2a10:afc0:e01f:1::ffff:0

    # You can add more mirrors easily!
    # address=/docker.io/2a10:afc0:e01f:1::ffff:0
    # address=/quay.io/2a10:afc0:e01f:1::ffff:0

    # Forward all other DNS requests to public DNS servers
    server=2001:4860:4860::8888
    server=2606:4700:4700::1111
    ```

2.  Run `dnsmasq`:
    ```bash
    sudo dnsmasq --no-daemon --log-queries --conf-file=./dnsmasq.conf
    ```

### OCI Mirror Setup

This is a pull-through container registry cache. When the FeOS VM requests an image (e.g., `ghcr.io/some/image`), the request is redirected by `dnsmasq` to this local registry. The local registry then pulls the image from the real `ghcr.io`, caches it locally, and serves it to the FeOS VM.

1.  Run the OCI mirror using Docker:
    ```bash
    sudo docker run -d --restart=always --name oci-mirror \
      -v /opt/registry-cache:/var/lib/registry \
      -e REGISTRY_PROXY_REMOTEURL=https://ghcr.io \
      -p "[2a10:afc0:e01f:1::ffff:0]:5000:5000" \
      registry:2
    ```
    *   `--name oci-mirror`: Gives the container a recognizable name.
    *   `-v /opt/registry-cache...`: Persists the image cache on the host at `/opt/registry-cache`.
    *   `-e REGISTRY_PROXY_REMOTEURL`: Tells the registry to act as a pull-through cache for `ghcr.io`.
    *   `-p "[...]"`: Binds the container's port 5000 to the specific IPv6 address `2a10:afc0:e01f:1::ffff:0` on the host, which is the same address we configured in `dnsmasq`.

## 3. Running the FeOS VM

With all services running, you can now start the FeOS VM:

```bash
make virsh-start virsh-console virsh-stop
```

The FeOS instance will boot, receive its network configuration, and be ready for testing. When its `vm-service` is asked to create a VM using an image from `ghcr.io`, the networking environment will transparently redirect the pull request to the local OCI mirror, allowing for fast, reliable, and offline-capable integration testing.