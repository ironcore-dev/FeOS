use structopt::StructOpt;
use tonic::transport::Endpoint;
use tonic::Request;
use tokio::time::timeout;
use std::time::Duration;

use container_grpc::container_service_client::ContainerServiceClient;
use container_grpc::*;

pub mod container_grpc {
    tonic::include_proto!("container");
}

#[derive(StructOpt, Debug)]
pub enum ContainerCommand {
    CreateContainer {
        image: String,
        #[structopt(name = "COMMAND", required = true, min_values = 1)]
        command: Vec<String>,
    },
    RunContainer {
        uuid: String,
    },
    KillContainer {
        uuid: String,
    },
    StateContainer {
        uuid: String,
    },
    DeleteContainer {
        uuid: String,
    },
}

pub async fn run_container_client(
    server_ip: String,
    port: u16,
    cmd: ContainerCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    let address = format!("http://{}:{}", server_ip, port);
    let channel = Endpoint::from_shared(address)?.connect().await?;
    let mut client = ContainerServiceClient::new(channel);

    match cmd {
        ContainerCommand::CreateContainer { image, command } => {
            let request = Request::new(CreateContainerRequest { image, command });
            match timeout(Duration::from_secs(30), client.create_container(request)).await {
                Ok(response) => {
                    match response {
                        Ok(response) => println!("CREATE CONTAINER RESPONSE={:?}", response),
                        Err(e) => eprintln!("Error creating container: {:?}", e),
                    }
                },
                Err(_) => eprintln!("Request timed out after 30 seconds"),
            }
        },
        ContainerCommand::RunContainer { uuid } => {
            let request = Request::new(RunContainerRequest { uuid });
            match client.run_container(request).await {
                Ok(response) => println!("RUN CONTAINER RESPONSE={:?}", response),
                Err(e) => eprintln!("Error running container: {:?}", e),
            }
        },
        ContainerCommand::KillContainer { uuid } => {
            let request = Request::new(KillContainerRequest { uuid });
            match client.kill_container(request).await {
                Ok(response) => println!("KILL CONTAINER RESPONSE={:?}", response),
                Err(e) => eprintln!("Error killing container: {:?}", e),
            }
        },
        ContainerCommand::StateContainer { uuid } => {
            let request = Request::new(StateContainerRequest { uuid });
            match client.state_container(request).await {
                Ok(response) => println!("STATE CONTAINER RESPONSE={:?}", response),
                Err(e) => eprintln!("Error getting container state: {:?}", e),
            }
        },
        ContainerCommand::DeleteContainer { uuid } => {
            let request = Request::new(DeleteContainerRequest { uuid });
            match client.delete_container(request).await {
                Ok(response) => println!("DELETE CONTAINER RESPONSE={:?}", response),
                Err(e) => eprintln!("Error deleting container: {:?}", e),
            }
        },
    }

    Ok(())
}
