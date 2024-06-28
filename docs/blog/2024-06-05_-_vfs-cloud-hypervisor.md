2024-06-25 How to bind PCIe VFs to cloud-hypervisor VMs to use dpservice
========================================================================

## Prepare Bluefield with dpservice on SmartNIC side

### Preconditions

Allocate hugepages for dpservice to consume.

```
echo 6 > /sys/kernel/mm/hugepages/hugepages-1048576kB/nr_hugepages
```

### Start dpservice

```
sudo podman run --rm --privileged -p1337:1337 -v /dev/:/dev/ -v /tmp/:/tmp/ --net=host ghcr.io/ironcore-dev/dpservice:latest -l 0,1 --  --no-stats --no-offload --nic-type=bluefield2
```

### Create dpservice network interface connected to a VF

```
sudo podman run --rm --entrypoint dpservice-cli --privileged --net=host ghcr.io/ironcore-dev/dpservice:latest create interface --id=vm5 --ipv4=10.200.1.6 --ipv6=:: --vni=200 --device=0000:03:00.0_representor_vf0
```

More info about the command line options of *dpservice-cli* [here](https://github.com/ironcore-dev/dpservice/blob/main/cli/dpservice-cli/docs/development/usage.md).


## Prepare cloud-hypervisor on host side

### Preconditions

Prepare SRIOV and create some VFs

```
echo 4 > /sys/class/net/your_device_name/device/sriov_numvfs
```

Bind modules

```
modprobe vfio_iommu_type1 allow_unsafe_interrupts
modprobe vfio_pci
```

### Add pci device

The following steps are needed in order to use a device with the following pci address `0000:06:00.2`

Unbind if bound

```
echo 0000:06:00.2 > /sys/bus/pci/devices/0000:06:00.2/driver/unbind
```

Bind to vfio

```
echo  {vendorID} {deviceID} > /sys/bus/pci/drivers/vfio-pci/new_id
# echo  15b3 101e > /sys/bus/pci/drivers/vfio-pci/new_id
```

Reference device in cloud-hypervisor

```
--device path=/sys/bus/pci/devices/0000:06:00.2/
```
