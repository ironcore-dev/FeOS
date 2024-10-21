use std::time::Duration;
use structopt::StructOpt;
use tokio::time::timeout;
use tonic::transport::Endpoint;
use tonic::Request;

use isolated_container_grpc::isolated_container_service_client::IsolatedContainerServiceClient;
use isolated_container_grpc::*;

pub mod isolated_container_grpc {
    tonic::include_proto!("isolated_container");
}

#[derive(StructOpt, Debug)]
pub enum IsolatedContainerCommand {
    Create {
        image: String,
        #[structopt(name = "COMMAND", required = true, min_values = 1)]
        command: Vec<String>,
    },
    Run {
        uuid: String,
    },
    Stop {
        uuid: String,
    },
    State {
        uuid: String,
    },
}

pub async fn run_isolated_container_client(
    server_ip: String,
    port: u16,
    cmd: IsolatedContainerCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    let address = format!("http://{}:{}", server_ip, port);
    let channel = Endpoint::from_shared(address)?.connect().await?;
    let mut client = IsolatedContainerServiceClient::new(channel);

    match cmd {
        IsolatedContainerCommand::Create { image, command } => {
            let request = Request::new(CreateContainerRequest { image, command });
            match timeout(Duration::from_secs(30), client.create_container(request)).await {
                Ok(response) => match response {
                    Ok(response) => println!("CREATE ISOLATED CONTAINER RESPONSE={:?}", response),
                    Err(e) => eprintln!("Error creating isolated container: {:?}", e),
                },
                Err(_) => eprintln!("Request timed out after 30 seconds"),
            }
        }
        IsolatedContainerCommand::Run { uuid } => {
            let request = Request::new(RunContainerRequest { uuid });
            match client.run_container(request).await {
                Ok(response) => println!("RUN ISOLATED CONTAINER RESPONSE={:?}", response),
                Err(e) => eprintln!("Error running isolated container: {:?}", e),
            }
        }
        IsolatedContainerCommand::Stop { uuid } => {
            let request = Request::new(StopContainerRequest { uuid });
            match client.stop_container(request).await {
                Ok(response) => println!("STOP ISOLATED CONTAINER RESPONSE={:?}", response),
                Err(e) => eprintln!("Error stopping isolated container: {:?}", e),
            }
        }
        IsolatedContainerCommand::State { uuid } => {
            let request = Request::new(StateContainerRequest { uuid });
            match client.state_container(request).await {
                Ok(response) => println!("STATE ISOLATED CONTAINER RESPONSE={:?}", response),
                Err(e) => eprintln!("Error getting isolated container state: {:?}", e),
            }
        }
    }

    Ok(())
}
