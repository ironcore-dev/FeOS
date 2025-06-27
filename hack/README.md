FeOS VM hacking tools
=======================

This directory includes scripts to run FeOS as the pid 1 process within a VM.

    make build-container
    make kernel
    make initramfs
    make ucode
    make uki

    # create `vm-br0` bridge on your machine:
    make network

    # run FeOS in a VM as pid 1:
    make virsh-start virsh-console virsh-stop

    # exit VM console with Ctrl+]


As FeOS is dynamically compiled, we need to copy all linked libraries into the initramfs. To get a consistent result across different build machines, a build container based on Debian is used (`make build-container`) to build FeOS. Also the libraries will be copied from this container into the initramfs (`make initramfs`).

The Linux kernel is built from source (`make kernel`) with a custom kernel config. You can specify the used kernel version in `hack/kernel/config.sh` and modify the kernel config using `make menuconfig`.

The virtual machine has a virtio-net NIC attached which will be connected to a Linux bridge on the host system. You can create and configure this bridge using `make network`. The NIC will appear as `eth0` within the VM.

With the make target `virsh-start` the VM will be created and started. `virsh-console` brings you into the serial console of the VM (you can exit it with `Ctrl+]`). To stop and destroy the VM call `make virsh-stop` - you'll probably want to concatenate those commands to `make virsh-start virsh-console virsh-stop`. This will start the VM, opens the serial console and waits for you to hit `Ctrl+]` to exit the serial console and destroy the VM.

### make run
To test feos locally, you can execute `make run` to compile and run feos locally as a non-PID 1 process. Via the environment system `$IPAM` you can provide the IPAM prefix, that feos will use for providing IP addresses to containers and VMs:

    IPAM=2001:db8::/64 make run




## Cloud-Hypervisor
If you want to run FeOS within a [cloud-hypervisor](https://www.cloudhypervisor.org/) VM get some inspiration from this bash snippet:

    sudo ip tuntap add mode tap name tap0
    sudo ip link set tap0 up
    sudo ip link set tap0 master vm-br0
    
    cloud-hypervisor \
        --cpus boot=4 \
        --memory size=1024M \
        --net tap=tap0 \
        --serial tty \
        --kernel target/kernel/vmlinuz \
        --initramfs target/initramfs.zst \
        --cmdline "`cat target/cmdline`"
