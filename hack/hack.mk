SHELL := /bin/bash

empty:

build-container:
	cd hack/build-container && ./mk-build-container
	mkdir -p target
	touch hack/build-container

container-release:
#	make cargo index cache
	mkdir -p target/cargo
	docker run --rm -u $${UID} -v "`pwd`/target/cargo:/.cargo" -v "`pwd`:/feos" feos-builder bash -c "cd /feos && CARGO_HOME=/.cargo make release"

kernel:
	mkdir -p target/rootfs/boot
	docker run --rm -u $${UID} -v "`pwd`:/feos" feos-builder bash -c "cd hack/kernel && ./mk-kernel"
	cp hack/kernel/cmdline.txt target/cmdline

menuconfig:
	docker run -it --rm -u $${UID} -v "`pwd`:/feos" feos-builder bash -c "cd hack/kernel && ./mk-menuconfig"

initramfs: target/cloud-hypervisor container-release
	mkdir -p target/rootfs/bin
	mkdir -p target/rootfs/etc/feos
	cp target/cloud-hypervisor/target/x86_64-unknown-linux-musl/release/cloud-hypervisor target/rootfs/bin/cloud-hypervisor
	cp target/x86_64-unknown-linux-musl/release/feos target/rootfs/bin/feos
	sudo chown -R `whoami` target/rootfs/etc/feos/
	cd target/rootfs && rm -f init && ln -s bin/feos init
	docker run --rm -u $${UID} -v "`pwd`:/feos" feos-builder bash -c "cd hack/initramfs && ./mk-initramfs"

target/cloud-hypervisor:
	git clone --depth 1 --branch $(shell cat hack/cloud-hypervisor/version) https://github.com/cloud-hypervisor/cloud-hypervisor.git target/cloud-hypervisor
	docker run --rm -u $${UID} -v "`pwd`:/feos" feos-builder bash -c "cd target/cloud-hypervisor && cargo build --release --target=x86_64-unknown-linux-musl --all"

keys:
	mkdir keys
	chmod 700 keys
	cp hack/uki/secureboot-cert.conf keys/
	openssl genrsa -out keys/secureboot.key 2048
	openssl req -config keys/secureboot-cert.conf -new -x509 -newkey rsa:2048 -keyout keys/secureboot.key -outform PEM -out keys/secureboot.pem -nodes -days 3650 -subj "/CN=FeOS/"
	openssl x509 -in keys/secureboot.pem -out keys/secureboot.der -outform DER

uki: keys
	docker run --rm -u $${UID} -v "`pwd`:/feos" feos-builder ukify build \
	  --os-release @/feos/hack/uki/os-release.txt \
	  --linux /feos/target/kernel/vmlinuz \
	  --initrd /feos/target/initramfs.zst \
	  --cmdline @/feos/hack/kernel/cmdline.txt \
	  --secureboot-private-key /feos/keys/secureboot.key \
	  --secureboot-certificate /feos/keys/secureboot.pem \
	  --output /feos/target/uki.efi

virsh-start:
	./hack/libvirt/init.sh libvirt-kvm.xml
	virsh --connect qemu:///system create target/libvirt.xml

virsh-stop:
	virsh --connect qemu:///system destroy feos

virsh-console:
	virsh --connect qemu:///system console feos

virsh-shutdown:
	virsh --connect qemu:///system shutdown feos --mode acpi

network:
	sudo ip link add name vm-br0 type bridge
	sudo ip link set up dev vm-br0
	sudo ip addr add fe80::1/64 dev vm-br0
	sudo ip addr add 169.254.42.1/24 dev vm-br0

run-vm:
	sudo cloud-hypervisor --cpus boot=4 --memory size=1024M --net tap=tap0 --serial tty --kernel target/kernel/vmlinuz --initramfs target/initramfs.zst --cmdline "$(shell cat target/cmdline)"
