kernel:
	mkdir -p target/rootfs/boot
	docker run --rm -u $${UID} -v "`pwd`:/feos" feos-builder bash -c "cd hack/kernel && ./mk-kernel"
	cp hack/kernel/cmdline.txt target/cmdline

menuconfig:
	docker run -it --rm -u $${UID} -v "`pwd`:/feos" feos-builder bash -c "cd hack/kernel && ./mk-menuconfig"
