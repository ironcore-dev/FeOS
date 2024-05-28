FILENAME = cloud-hypervisor-static-aarch64
LOCAL_DIR = target/cloud-hypervisor/target

target/cloud-hypervisor:
	mkdir -p $(LOCAL_DIR)
	curl -L "https://github.com/cloud-hypervisor/cloud-hypervisor/releases/download/$(shell cat hack/cloud-hypervisor/version)/$(FILENAME)" -o "$(LOCAL_DIR)/$(FILENAME)"