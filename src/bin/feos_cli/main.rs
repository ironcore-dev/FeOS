mod client;
mod client_container;
mod client_isolated_container;

use client::Opt;
use structopt::StructOpt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::from_args();
    client::run_client(opt).await
}
