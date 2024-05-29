CH_FILENAME = cloud-hypervisor-static-aarch64

target/cloud-hypervisor:
	mkdir -p target/cloud-hypervisor/target
	curl -L "https://github.com/cloud-hypervisor/cloud-hypervisor/releases/download/$(shell cat hack/cloud-hypervisor/version)/$(CH_FILENAME)" -o "target/cloud-hypervisor/target/$(CH_FILENAME)"