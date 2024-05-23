FW_FILENAME = hypervisor-fw

target/cloud-hypervisor-firmware:
	mkdir -p target/cloud-hypervisor/target
	curl -L "https://github.com/cloud-hypervisor/rust-hypervisor-firmware/releases/download/$(shell cat hack/cloud-hypervisor-firmware/version)/$(FW_FILENAME)" -o "target/cloud-hypervisor/target/$(FW_FILENAME)"