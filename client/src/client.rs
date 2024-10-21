use std::time::Duration;
use structopt::StructOpt;
use tokio::io::{self, AsyncBufReadExt};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Endpoint;
use tonic::Request;

use crate::client_container::ContainerCommand;
use crate::client_isolated_container::IsolatedContainerCommand;
use feos_grpc::feos_grpc_client::FeosGrpcClient;
use feos_grpc::*;

pub mod feos_grpc {
    tonic::include_proto!("feos_grpc");
}

#[derive(StructOpt, Debug)]
#[structopt(name = "feos_client")]
pub struct Opt {
    #[structopt(short, long)]
    pub server_ip: String,
    #[structopt(short, long, default_value = "1337")]
    pub port: u16,
    #[structopt(subcommand)]
    pub cmd: Command,
}

#[derive(StructOpt, Debug)]
pub enum Command {
    Reboot,
    Shutdown,
    HostInfo,
    Ping,
    FetchImage {
        image: String,
    },
    CreateVM {
        cpu: u32,
        memory_bytes: u64,
        image_uuid: String,
        ignition: Option<String>,
    },
    GetVM {
        uuid: String,
    },
    BootVM {
        uuid: String,
    },
    ShutdownVM {
        uuid: String,
    },
    PingVM {
        uuid: String,
    },
    AttachNicVM {
        uuid: String,
        mac_address: String,
        pci_address: String,
    },
    GetFeOSKernelLogs,
    GetFeOSLogs,
    ConsoleVM {
        uuid: String,
    },
    Container(ContainerCommand),
    IsolatedContainer(IsolatedContainerCommand),
}

fn format_address(ip: &str, port: u16) -> String {
    if ip.contains(':') {
        // IPv6 address
        format!("http://[{}]:{}", ip, port)
    } else {
        // IPv4 address
        format!("http://{}:{}", ip, port)
    }
}

pub async fn run_client(opt: Opt) -> Result<(), Box<dyn std::error::Error>> {
    let address = format_address(&opt.server_ip, opt.port);
    let endpoint = Endpoint::from_shared(address)?
        .keep_alive_while_idle(true)
        .keep_alive_timeout(Duration::from_secs(20));

    let channel = endpoint.connect().await?;
    let mut client = FeosGrpcClient::new(channel);

    match opt.cmd {
        Command::Container(container_cmd) => {
            crate::client_container::run_container_client(opt.server_ip, opt.port, container_cmd)
                .await?;
        }
        Command::IsolatedContainer(container_cmd) => {
            crate::client_isolated_container::run_isolated_container_client(
                opt.server_ip,
                opt.port,
                container_cmd,
            )
            .await?;
        }
        Command::Reboot => {
            let request = Request::new(RebootRequest {});
            let response = client.reboot(request).await?;
            println!("REBOOT RESPONSE={:?}", response);
        }
        Command::Shutdown => {
            let request = Request::new(ShutdownRequest {});
            let response = client.shutdown(request).await?;
            println!("SHUTDOWN RESPONSE={:?}", response);
        }
        Command::HostInfo => {
            let request = Request::new(HostInfoRequest {});
            let response = client.host_info(request).await?;
            println!("HOST INFO RESPONSE={:?}", response);
        }
        Command::Ping => {
            let request = Request::new(Empty {});
            let response = client.ping(request).await?;
            println!("PING RESPONSE={:?}", response);
        }
        Command::FetchImage { image } => {
            let request = Request::new(FetchImageRequest { image });
            let response = client.fetch_image(request).await?;
            println!("FETCH IMAGE RESPONSE={:?}", response);
        }
        Command::CreateVM {
            cpu,
            memory_bytes,
            image_uuid,
            ignition,
        } => {
            let request = Request::new(CreateVmRequest {
                cpu,
                memory_bytes,
                image_uuid,
                ignition,
            });
            let response = client.create_vm(request).await?;
            println!("CREATE VM RESPONSE={:?}", response);
        }
        Command::GetVM { uuid } => {
            let request = Request::new(GetVmRequest { uuid });
            let response = client.get_vm(request).await?;
            println!("GET VM RESPONSE={:?}", response);
        }
        Command::BootVM { uuid } => {
            let request = Request::new(BootVmRequest { uuid });
            let response = client.boot_vm(request).await?;
            println!("BOOT VM RESPONSE={:?}", response);
        }
        Command::PingVM { uuid } => {
            let request = Request::new(PingVmRequest { uuid });
            let response = client.ping_vm(request).await?;
            println!("BOOT VM RESPONSE={:?}", response);
        }
        Command::ShutdownVM { uuid } => {
            let request = Request::new(ShutdownVmRequest { uuid });
            let response = client.shutdown_vm(request).await?;
            println!("SHUTDOWN VM RESPONSE={:?}", response);
        }
        Command::AttachNicVM {
            uuid,
            mac_address,
            pci_address,
        } => {
            let request = Request::new(AttachNicVmRequest {
                uuid,
                mac_address,
                pci_address,
            });
            let response = client.attach_nic_vm(request).await?;
            println!("ATTACH NIC VM RESPONSE={:?}", response);
        }
        Command::GetFeOSKernelLogs => {
            let request = Request::new(GetFeOsKernelLogRequest {});
            let mut response = client.get_fe_os_kernel_logs(request).await?.into_inner();

            while let Some(log_response) = response.message().await? {
                println!("FEOS KERNEL LOG RESPONSE={:?}", log_response);
            }
        }
        Command::GetFeOSLogs => {
            let request = Request::new(GetFeOsLogRequest {});
            let mut response = client.get_fe_os_logs(request).await?.into_inner();

            while let Some(log_response) = response.message().await? {
                println!("FEOS LOG RESPONSE={:?}", log_response);
            }
        }
        Command::ConsoleVM { uuid } => {
            let (tx, rx) = mpsc::channel(4);

            tokio::spawn(async move {
                let mut reader = io::BufReader::new(io::stdin()).lines();
                while let Some(line) = reader.next_line().await.unwrap_or_else(|e| {
                    println!("Failed to read line from stdin: {:?}", e);
                    None
                }) {
                    let request = ConsoleVmRequest {
                        uuid: uuid.clone(),
                        input: line,
                    };
                    if tx.send(request).await.is_err() {
                        break;
                    }
                }
            });

            let request_stream = ReceiverStream::new(rx);
            let mut response = client.console_vm(request_stream).await?.into_inner();

            while let Some(response) = response.message().await? {
                print!("{}", response.message);
            }
        }
    }

    Ok(())
}
