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

use futures::future;
use sp_runtime::traits::Hash;
pub use sp_runtime::traits::SignedExtension;

use crate::{
    error::{
        BasicError,
        HasModuleError,
    },
    extrinsic::{
        self,
        SignedExtra,
        Signer,
        UncheckedExtrinsic,
    },
    rpc::{
        Rpc,
        RpcClient,
        RuntimeVersion,
        SystemProperties,
    },
    storage::StorageClient,
    transaction::TransactionProgress,
    Call,
    Config,
    Metadata,
};
use codec::Decode;
use derivative::Derivative;
use std::sync::Arc;

/// ClientBuilder for constructing a Client.
#[derive(Default)]
pub struct ClientBuilder {
    url: Option<String>,
    client: Option<RpcClient>,
    page_size: Option<u32>,
}

impl ClientBuilder {
    /// Creates a new ClientBuilder.
    pub fn new() -> Self {
        Self {
            url: None,
            client: None,
            page_size: None,
        }
    }

    /// Sets the jsonrpsee client.
    pub fn set_client<C: Into<RpcClient>>(mut self, client: C) -> Self {
        self.client = Some(client.into());
        self
    }

    /// Set the substrate rpc address.
    pub fn set_url<P: Into<String>>(mut self, url: P) -> Self {
        self.url = Some(url.into());
        self
    }

    /// Set the page size.
    pub fn set_page_size(mut self, size: u32) -> Self {
        self.page_size = Some(size);
        self
    }

    /// Creates a new Client.
    pub async fn build<T: Config>(self) -> Result<Client<T>, BasicError> {
        let client = if let Some(client) = self.client {
            client
        } else {
            let url = self.url.as_deref().unwrap_or("ws://127.0.0.1:9944");
            crate::rpc::ws_client(url).await?
        };
        let rpc = Rpc::new(client);
        let (metadata, genesis_hash, runtime_version, properties) = future::join4(
            rpc.metadata(),
            rpc.genesis_hash(),
            rpc.runtime_version(None),
            rpc.system_properties(),
        )
        .await;
        let metadata = metadata?;

        Ok(Client {
            rpc,
            genesis_hash: genesis_hash?,
            metadata: Arc::new(metadata),
            properties: properties.unwrap_or_else(|_| Default::default()),
            runtime_version: runtime_version?,
            iter_page_size: self.page_size.unwrap_or(10),
        })
    }
}

/// Client to interface with a substrate node.
#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub struct Client<T: Config> {
    rpc: Rpc<T>,
    genesis_hash: T::Hash,
    metadata: Arc<Metadata>,
    properties: SystemProperties,
    runtime_version: RuntimeVersion,
    iter_page_size: u32,
}

impl<T: Config> std::fmt::Debug for Client<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("rpc", &"<Rpc>")
            .field("genesis_hash", &self.genesis_hash)
            .field("metadata", &"<Metadata>")
            .field("events_decoder", &"<EventsDecoder>")
            .field("properties", &self.properties)
            .field("runtime_version", &self.runtime_version)
            .field("iter_page_size", &self.iter_page_size)
            .finish()
    }
}

impl<T: Config> Client<T> {
    /// Returns the genesis hash.
    pub fn genesis(&self) -> &T::Hash {
        &self.genesis_hash
    }

    /// Returns the chain metadata.
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    /// Returns the properties defined in the chain spec as a JSON object.
    ///
    /// # Note
    ///
    /// Many chains use this to define common properties such as `token_decimals` and `token_symbol`
    /// required for UIs, but this is merely a convention. It is up to the library user to
    /// deserialize the JSON into the appropriate type or otherwise extract the properties defined
    /// in the target chain's spec.
    pub fn properties(&self) -> &SystemProperties {
        &self.properties
    }

    /// Returns the rpc client.
    pub fn rpc(&self) -> &Rpc<T> {
        &self.rpc
    }

    /// Create a client for accessing runtime storage
    pub fn storage(&self) -> StorageClient<T> {
        StorageClient::new(&self.rpc, &self.metadata, self.iter_page_size)
    }

    /// Convert the client to a runtime api wrapper for custom runtime access.
    ///
    /// The `subxt` proc macro will provide methods to submit extrinsics and read storage specific
    /// to the target runtime.
    pub fn to_runtime_api<R: From<Self>>(self) -> R {
        self.into()
    }
}

/// A constructed call ready to be signed and submitted.
pub struct SubmittableExtrinsic<'client, T: Config, X, C, E: Decode, Evs: Decode> {
    client: &'client Client<T>,
    call: C,
    marker: std::marker::PhantomData<(X, E, Evs)>,
}

impl<'client, T, X, C, E, Evs> SubmittableExtrinsic<'client, T, X, C, E, Evs>
where
    T: Config,
    X: SignedExtra<T>,
    C: Call + Send + Sync,
    E: Decode + HasModuleError,
    Evs: Decode,
{
    /// Create a new [`SubmittableExtrinsic`].
    pub fn new(client: &'client Client<T>, call: C) -> Self {
        Self {
            client,
            call,
            marker: Default::default(),
        }
    }

    /// Creates and signs an extrinsic and submits it to the chain.
    ///
    /// Returns a [`TransactionProgress`], which can be used to track the status of the transaction
    /// and obtain details about it, once it has made it into a block.
    pub async fn sign_and_submit_then_watch(
        self,
        signer: &(dyn Signer<T, X> + Send + Sync),
    ) -> Result<TransactionProgress<'client, T, E, Evs>, BasicError>
    where
        <<X as SignedExtra<T>>::Extra as SignedExtension>::AdditionalSigned:
            Send + Sync + 'static,
    {
        // Sign the call data to create our extrinsic.
        let extrinsic = self.create_signed(signer, Default::default()).await?;

        // Get a hash of the extrinsic (we'll need this later).
        let ext_hash = T::Hashing::hash_of(&extrinsic);

        // Submit and watch for transaction progress.
        let sub = self.client.rpc().watch_extrinsic(extrinsic).await?;

        Ok(TransactionProgress::new(sub, self.client, ext_hash))
    }

    /// Creates and signs an extrinsic using `additional_params` and submits it to the chain.
    ///
    /// Returns a [`TransactionProgress`], which can be used to track the status of the transaction
    /// and obtain details about it, once it has made it into a block.
    pub async fn sign_and_submit_with_aditional_then_watch(
        self,
        signer: &(dyn Signer<T, X> + Send + Sync),
        additional_params: X::Parameters,
    ) -> Result<TransactionProgress<'client, T, E, Evs>, BasicError>
    where
        <<X as SignedExtra<T>>::Extra as SignedExtension>::AdditionalSigned:
            Send + Sync + 'static,
    {
        // Sign the call data to create our extrinsic.
        let extrinsic = self.create_signed(signer, additional_params).await?;

        // Get a hash of the extrinsic (we'll need this later).
        let ext_hash = T::Hashing::hash_of(&extrinsic);

        // Submit and watch for transaction progress.
        let sub = self.client.rpc().watch_extrinsic(extrinsic).await?;

        Ok(TransactionProgress::new(sub, self.client, ext_hash))
    }

    /// Creates and signs an extrinsic and submits to the chain for block inclusion.
    ///
    /// Returns `Ok` with the extrinsic hash if it is valid extrinsic.
    ///
    /// # Note
    ///
    /// Success does not mean the extrinsic has been included in the block, just that it is valid
    /// and has been included in the transaction pool.
    pub async fn sign_and_submit(
        self,
        signer: &(dyn Signer<T, X> + Send + Sync),
    ) -> Result<T::Hash, BasicError>
    where
        <<X as SignedExtra<T>>::Extra as SignedExtension>::AdditionalSigned:
            Send + Sync + 'static,
    {
        self.sign_and_submit_with_additional(signer, Default::default()).await
    }

    /// Creates and signs an extrinsic using `additional_params` and submits to the chain for 
    /// block inclusion.
    ///
    /// Returns `Ok` with the extrinsic hash if it is valid extrinsic.
    pub async fn sign_and_submit_with_additional(
        self,
        signer: &(dyn Signer<T, X> + Send + Sync),
        additional_params: X::Parameters,
    ) -> Result<T::Hash, BasicError>
    where
        <<X as SignedExtra<T>>::Extra as SignedExtension>::AdditionalSigned:
            Send + Sync + 'static,
    {
        let extrinsic = self.create_signed(signer, additional_params).await?;
        self.client.rpc().submit_extrinsic(extrinsic).await
    }

    /// Creates a signed extrinsic.
    pub async fn create_signed(
        &self,
        signer: &(dyn Signer<T, X> + Send + Sync),
        additional_params: X::Parameters,
    ) -> Result<UncheckedExtrinsic<T, X>, BasicError>
    where
        <<X as SignedExtra<T>>::Extra as SignedExtension>::AdditionalSigned:
            Send + Sync + 'static,
    {
        let account_nonce = if let Some(nonce) = signer.nonce() {
            nonce
        } else {
            self.client
                .rpc()
                .system_account_next_index(signer.account_id())
                .await?
        };
        let call = self
            .client
            .metadata()
            .pallet(C::PALLET)
            .and_then(|pallet| pallet.encode_call(&self.call))?;

        let signed = extrinsic::create_signed(
            &self.client.runtime_version,
            self.client.genesis_hash,
            account_nonce,
            call,
            signer,
            additional_params,
        )
        .await?;
        Ok(signed)
    }
}
