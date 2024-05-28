initramfs: target/cloud-hypervisor target/cloud-hypervisor-firmware container-release
	mkdir -p target/rootfs/bin
	mkdir -p target/rootfs/etc/feos
	mkdir -p target/rootfs/lib/firmware
	cp target/cloud-hypervisor/target/cloud-hypervisor-static-aarch64 target/rootfs/bin/cloud-hypervisor
	cp target/cloud-hypervisor/target/hypervisor-fw target/rootfs/lib/firmware
	cp target/x86_64-unknown-linux-musl/release/feos target/rootfs/bin/feos
	sudo chown -R `whoami` target/rootfs/etc/feos/
	cd target/rootfs && rm -f init && ln -s bin/feos init
	docker run --rm -u $${UID} -v "`pwd`:/feos" feos-builder bash -c "cd hack/initramfs && ./mk-initramfs"
