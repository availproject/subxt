use anyhow::Result;
use subxt::{AvailExtra, ClientBuilder};

pub mod avail_subxt_config;
use avail_subxt_config::*;

#[async_std::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let api = ClientBuilder::new()
        .set_url("wss://testnet.polygonavail.net:443/ws")
        .build()
        .await?
        .to_runtime_api::<avail::RuntimeApi<AvailConfig, AvailExtra<AvailConfig>>>();

    let mut finalized_blocks = api
        .client
        .rpc()
        .subscribe_finalized_blocks()
        .await?;

    while let Some(finalized_block) = finalized_blocks.next().await {
        println!("\nFinalized Block: {:?}", finalized_block.unwrap());
    }

    Ok(())
}
