target/cloud-hypervisor:
	git clone --depth 1 --branch $(shell cat hack/cloud-hypervisor/version) https://github.com/cloud-hypervisor/cloud-hypervisor.git target/cloud-hypervisor
	docker run --rm -u $${UID} -v "`pwd`:/feos" feos-builder bash -c "cd target/cloud-hypervisor && cargo build --release --target=x86_64-unknown-linux-musl --all"
