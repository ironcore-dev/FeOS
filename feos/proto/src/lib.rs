// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

pub mod vm_service {
    tonic::include_proto!("feos.vm.vmm.api.v1");
}
pub mod host_service {
    tonic::include_proto!("feos.host.v1");
}
pub mod image_service {
    tonic::include_proto!("feos.image.vmm.api.v1");
}
pub mod task_service {
    tonic::include_proto!("feos.task.v1");
}
