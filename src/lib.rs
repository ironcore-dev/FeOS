pub mod container;
pub mod daemon;
pub mod dhcpv6;
pub mod filesystem;
pub mod fsmount;
pub mod host;
pub mod move_root;
pub mod network;
pub mod radv;
pub mod ringbuffer;
pub mod vm;

pub mod feos_grpc {
    tonic::include_proto!("feos_grpc");
}
