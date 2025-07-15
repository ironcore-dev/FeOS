pub mod vm_service {
    tonic::include_proto!("feos.vm.vmm.api.v1");
}

pub mod host_service {
    tonic::include_proto!("feos.host.v1");
}

pub mod image_service {
    tonic::include_proto!("feos.image.vmm.api.v1");
}
