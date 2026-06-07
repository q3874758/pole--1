//! Bridge layer between the Rust prototype and the Cosmos SDK chain.
//!
//! Split into:
//! - [`address`]    — hex ↔ bech32 helpers and the [`CosmosAddress`] type
//! - [`error`]      — single [`CosmosError`] used by every other module
//! - [`rpc_client`] — Tendermint RPC (`/broadcast_tx_sync`, `/status`, abci_query)
//! - [`query_client`] — REST client (`/cosmos/auth/v1beta1/accounts/...`)
//! - [`tx_signer`]  — canonical [`SignDoc`] and Ed25519 signing
//! - [`tx_builder`] — [`BridgeMessage`] enum + [`TxBuilder`] for JSON projection
//!
//! The high-level [`CosmosClient`] ties the pieces together: it takes a
//! [`crate::wallet::KeyPair`], builds a [`TxBuilder`], looks up the
//! account, signs, and broadcasts.
pub mod address;
pub mod eip712;
pub mod error;
pub mod pole_msgs;
pub mod proto;
pub mod query_client;
pub mod rpc_client;
pub mod tx_builder;
pub mod tx_signer;

use std::time::Duration;

pub use address::{bech32_to_address, bech32_to_hex, hex_to_bech32, CosmosAddress, DEFAULT_BECH32_PREFIX, POLE_BECH32_PREFIX};
pub use eip712::{eip712_sign, hash_struct, keccak256, typed_data_hash, DomainSeparator};
pub use error::{CosmosError, Result};
pub use query_client::{AccountInfo, RestClient};
pub use rpc_client::{AbciQueryResponseInner, BroadcastOptions, BroadcastTxResponse, StatusResponse, TendermintRpc};
pub use tx_builder::{BridgeMessage, FeeConfig, TxBuilder};
pub use tx_signer::{sign_with_keypair, SignDocInputs, SignedTx};

/// Cargo feature name that gates real-`poled` integration tests in
/// `tests/harness/mod.rs` and `tests/integration.rs`. Pinning this
/// constant at the lib root lets `tests/harness_feature_consistency.rs`
/// cross-check that the `Cargo.toml` `[features]` entry actually
/// declares the same name. Phase 0.1 closed a latent bug where the
/// constant existed but the feature was not declared.
pub const HARNESS_FEATURE_NAME_FOR_TEST: &str = "integration";

use crate::wallet::KeyPair;

/// Connection parameters for the bridge. Constructed once at startup and
/// shared by every `CosmosClient`.
#[derive(Debug, Clone)]
pub struct CosmosEndpoint {
    /// Tendermint RPC, e.g. `http://localhost:26657`
    pub rpc_url: String,
    /// Cosmos SDK REST API, e.g. `http://localhost:1317`
    pub rest_url: String,
    /// `chain_id` from the genesis file, e.g. `pole_7776-1`
    pub chain_id: String,
    /// bech32 prefix for addresses on this chain
    pub address_prefix: String,
}

impl CosmosEndpoint {
    pub fn local_pole(chain_id: impl Into<String>) -> Self {
        Self {
            rpc_url: "http://localhost:26657".into(),
            rest_url: "http://localhost:1317".into(),
            chain_id: chain_id.into(),
            address_prefix: DEFAULT_BECH32_PREFIX.to_string(),
        }
    }

    /// Override the bech32 address prefix. Defaults to
    /// [`DEFAULT_BECH32_PREFIX`] (matches the chain's current
    /// `"cosmos"` codec); pass [`POLE_BECH32_PREFIX`] when the chain
    /// is migrated to the forward-looking `pole1...` addresses.
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.address_prefix = prefix.into();
        self
    }
}

/// High-level façade. Holds the RPC + REST clients and exposes
/// `broadcast(...)` for callers that already produced a [`SignedTx`].
pub struct CosmosClient {
    pub endpoint: CosmosEndpoint,
    pub rpc: TendermintRpc,
    pub rest: RestClient,
}

impl CosmosClient {
    pub fn new(endpoint: CosmosEndpoint) -> Result<Self> {
        let rpc = TendermintRpc::new(endpoint.rpc_url.clone())?;
        let rest = RestClient::new(endpoint.rest_url.clone())?;
        Ok(Self { endpoint, rpc, rest })
    }

    pub fn with_timeout(endpoint: CosmosEndpoint, timeout: Duration) -> Result<Self> {
        let rpc_client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| CosmosError::Http(e.to_string()))?;
        let rest_client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| CosmosError::Http(e.to_string()))?;
        let rpc = TendermintRpc::with_client(endpoint.rpc_url.clone(), rpc_client);
        let rest = RestClient::with_client(endpoint.rest_url.clone(), rest_client);
        Ok(Self { endpoint, rpc, rest })
    }

    /// Construct a client with a non-default bech32 prefix.
    /// Convenience wrapper around [`CosmosEndpoint::with_prefix`].
    pub fn with_prefix(endpoint: CosmosEndpoint, prefix: impl Into<String>) -> Result<Self> {
        Self::new(endpoint.with_prefix(prefix))
    }

    /// Look up the on-chain `account_number`/`sequence` for `address`.
    pub async fn account(&self, address: &str) -> Result<AccountInfo> {
        self.rest.get_account(address).await
    }

    /// Convenience: build a `TxBuilder` pre-configured for this chain.
    pub fn builder(&self, account_number: u64, sequence: u64) -> TxBuilder<'_> {
        TxBuilder::new(&self.endpoint.chain_id).with_sequence(account_number, sequence)
    }

    /// Broadcast a pre-signed transaction. Returns the tx hash on success.
    pub async fn broadcast(
        &self,
        signed: &SignedTx,
        opts: &BroadcastOptions,
    ) -> Result<BroadcastTxResponse> {
        let b64 = signed.to_base64()?;
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64)?;
        self.rpc.broadcast_tx_sync(&bytes, opts).await
    }

    /// End-to-end: lookup account → build → sign → broadcast.
    /// `signer` must be the bech32 address derived from `keypair`.
    pub async fn submit(
        &self,
        msg: &BridgeMessage,
        signer: &CosmosAddress,
        keypair: &KeyPair,
        opts: &BroadcastOptions,
    ) -> Result<BroadcastTxResponse> {
        let info = self.account(&signer.bech32).await?;
        let account_number: u64 = info.account_number.parse()
            .map_err(|e: std::num::ParseIntError| CosmosError::Decode(e.to_string()))?;
        let sequence: u64 = info.sequence.parse()
            .map_err(|e: std::num::ParseIntError| CosmosError::Decode(e.to_string()))?;
        let builder = self.builder(account_number, sequence);
        let signed = builder.build(msg, signer, keypair)?;
        self.broadcast(&signed, opts).await
    }
}
