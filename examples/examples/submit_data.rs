// Copyright 2019-2022 Parity Technologies (UK) Ltd.
// This file is part of subxt.
//
// subxt is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// subxt is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with subxt.  If not, see <http://www.gnu.org/licenses/>.

//! To run this example, a local polkadot node should be running. Example verified against polkadot 0.9.13-82616422d0-aarch64-macos.
//!
//! E.g.
//! ```bash
//! curl "https://github.com/paritytech/polkadot/releases/download/v0.9.13/polkadot" --output /usr/local/bin/polkadot --location
//! polkadot --dev --tmp
//! ```

use sp_keyring::AccountKeyring;
use subxt::{
    ClientBuilder,
    DefaultConfig,
    AvailExtra,
    AvailExtraParameters,
    PairSigner,
};

use avail::runtime_types::frame_support::storage::bounded_vec::BoundedVec;

#[subxt::subxt(runtime_metadata_path = "examples/avail.metadata.scale")]
pub mod avail{}

#[async_std::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let signer = PairSigner::new(AccountKeyring::Alice.pair());

    let api = ClientBuilder::new()
        .build()
        .await?
        .to_runtime_api::<avail::RuntimeApi<DefaultConfig, AvailExtra<DefaultConfig>>>();
    let hash = api
        .tx()
        .data_availability()
        .submit_data(BoundedVec(b"example".to_vec()))
        .sign_and_submit_with_additional(&signer, AvailExtraParameters{ tip: 0, app_id: 1 })
        .await?;

    println!("Data extrinsic submitted: {}", hash);

    Ok(())
}
