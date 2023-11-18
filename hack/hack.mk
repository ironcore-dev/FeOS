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

menuconfig:
	docker run -it --rm -u $${UID} -v "`pwd`:/feos" feos-builder bash -c "cd hack/kernel && ./mk-menuconfig"

initramfs: container-release
	mkdir -p target/rootfs/bin
	mkdir -p target/rootfs/etc/feos
	cp target/release/feos target/rootfs/bin/feos
	sudo chown -R `whoami` target/rootfs/etc/feos/
	cd target/rootfs && rm -f init && ln -s bin/feos init
	docker run --rm -u $${UID} -v "`pwd`:/feos" feos-builder bash -c "cd hack/initramfs && ./mk-initramfs"

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
	  --cmdline @/feos/hack/uki/cmdline.txt \
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
	sudo brctl addbr vm-br0
	sudo ip link set up dev vm-br0
	sudo ip addr add fe80::1/64 dev vm-br0
	sudo ip addr add 169.254.42.1/24 dev vm-br0
