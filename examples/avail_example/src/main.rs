use anyhow::Result;
use futures::{future::join_all, TryFutureExt};
use subxt::{AvailExtra, BlockNumber, ClientBuilder};

pub mod avail_subxt_config;
use avail_subxt_config::*;

/// This example gets all the headers from testnet. It requests them in concurrently in batches of BATCH_NUM.
/// Fetching headers one by one is too slow for a large number of blocks.

const BATCH_NUM: usize = 1000;
#[async_std::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let api = ClientBuilder::new()
        .set_url("wss://testnet.polygonavail.net:443/ws")
        .build()
        .await?
        .to_runtime_api::<avail::RuntimeApi<AvailConfig, AvailExtra<AvailConfig>>>();

    let block_num = api.client.rpc().header(None).await.unwrap().unwrap().number;
    println!("Current head: {block_num}");

    let mut headers = vec![];

    for batch in (1u32..=block_num)
        .collect::<Vec<_>>()
        .chunks(BATCH_NUM)
        .map(|e| {
            join_all(
                e.iter()
                    .map(|n| {
                        api.client
                            .rpc()
                            .block_hash(Some(BlockNumber::from(*n)))
                            .and_then(|h| api.client.rpc().header(h))
                    })
                    .collect::<Vec<_>>(),
            )
        })
    {
        headers.extend(batch.await);
    }
    println!("Headers: {num}", num = headers.len());

    assert_eq!(
        headers.len(),
        block_num as usize,
        "Didn't get the same number of block headers."
    );

    Ok(())
}
