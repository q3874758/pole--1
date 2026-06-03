//! Reusable test harness for the Rust ↔ Cosmos bridge layer.
//!
//! Each `IntegrationHarness` instance owns a `tempfile::TempDir` and a
//! `Child` process for `poled`, plus a `CosmosClient` wired to it.
//! High-level helpers (`register_node`, `submit_batch`, `commit_epoch`,
//! `claim_reward`) wrap the corresponding `MsgServer` entry points.
//!
//! Skeleton limitations:
//! - The `submit_batch` / `commit_epoch` paths reuse the
//!   `MsgUpsertNode` JSON projection as a stand-in. Replace once the
//!   `BatchCommit → MsgSubmitBatch` and `EpochCommit → MsgCommitEpoch`
//!   converters land.
//! - The harness requires a built `poled` binary on $PATH; tests
//!   should be guarded with `#[cfg(feature = "integration")]`.
//! - All async ops time out after 30s. Recovery from a crashed `poled`
//!   is not supported yet.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tempfile::TempDir;
use tokio::time::sleep;

use pole_protocol_draft::cosmos::{
    address, BridgeMessage, CosmosAddress, CosmosClient, CosmosEndpoint,
};
use pole_protocol_draft::wallet::KeyPair;

/// Test feature gate. Callers should `#[cfg(feature = "integration")]`
/// their test modules so `cargo test` still passes on dev machines
/// without a built `poled` binary.
pub const HARNESS_FEATURE: &str = "integration";

pub const DEFAULT_CHAIN_ID: &str = "pole-test";
pub const DEFAULT_RPC_URL: &str = "http://127.0.0.1:26657";
pub const DEFAULT_REST_URL: &str = "http://127.0.0.1:1317";

/// 30s default. Chain needs ~5s to commit the first block, then each
/// tx takes a couple of seconds, so this gives 10+ blocks of headroom
/// for slower hardware.
pub const DEFAULT_OP_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, thiserror::Error)]
pub enum HarnessError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("cosmos: {0}")]
    Cosmos(#[from] pole_protocol_draft::cosmos::CosmosError),

    #[error("chain not reachable at {url} after {secs}s")]
    ChainNotReady { url: String, secs: u64 },

    #[error("expected field missing: {0}")]
    Missing(&'static str),

    #[error("not implemented in skeleton: {0}")]
    Unimplemented(&'static str),

    #[error("chain returned non-zero code {code}: {log}")]
    ChainRejected { code: u32, log: String },
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct RegisteredNodeCapabilities {
    pub collect: bool,
    pub store: bool,
    pub verify: bool,
    pub propose: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredNode {
    pub operator_bech32: String,
    pub node_id_hex: String,
    pub capabilities: RegisteredNodeCapabilities,
}

/// Builder for [`IntegrationHarness`]. Pre-decode the field shape so
/// the boot path is easy to read.
#[derive(Default)]
pub struct IntegrationHarnessBuilder {
    chain_id: Option<String>,
    rpc_url: Option<String>,
    rest_url: Option<String>,
    address_prefix: Option<String>,
    poled_path: Option<PathBuf>,
    pre_mint: Vec<(String, u128)>,
}

impl IntegrationHarnessBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn chain_id(mut self, id: impl Into<String>) -> Self {
        self.chain_id = Some(id.into());
        self
    }

    pub fn rpc_url(mut self, url: impl Into<String>) -> Self {
        self.rpc_url = Some(url.into());
        self
    }

    pub fn rest_url(mut self, url: impl Into<String>) -> Self {
        self.rest_url = Some(url.into());
        self
    }

    pub fn address_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.address_prefix = Some(prefix.into());
        self
    }

    /// Path to the `poled` binary. Defaults to `poled` on $PATH.
    pub fn poled_binary(mut self, path: impl Into<PathBuf>) -> Self {
        self.poled_path = Some(path.into());
        self
    }

    /// Pre-mint `upole` to a test address. The genesis file is patched
    /// so the address is funded at startup.
    pub fn pre_mint(mut self, address: impl Into<String>, amount: u128) -> Self {
        self.pre_mint.push((address.into(), amount));
        self
    }

    /// Boot the harness. Returns once `/status` returns 200.
    pub async fn boot(self) -> Result<IntegrationHarness, HarnessError> {
        let chain_id = self.chain_id.unwrap_or_else(|| DEFAULT_CHAIN_ID.to_string());
        let rpc_url = self.rpc_url.unwrap_or_else(|| DEFAULT_RPC_URL.to_string());
        let rest_url = self.rest_url.unwrap_or_else(|| DEFAULT_REST_URL.to_string());
        let prefix = self
            .address_prefix
            .unwrap_or_else(|| pole_protocol_draft::cosmos::DEFAULT_BECH32_PREFIX.to_string());

        let tmp = TempDir::new()?;
        let chain_home = tmp.path().join(".poled");
        std::fs::create_dir_all(&chain_home)?;

        let poled_bin = self
            .poled_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("poled"));

        // 1. `poled init` to lay down config/
        let status = Command::new(&poled_bin)
            .args(["init", "test-validator", "--chain-id", &chain_id, "--home"])
            .arg(&chain_home)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if !status.success() {
            return Err(HarnessError::Unimplemented(
                "poled init returned non-zero — ensure the binary is built and on $PATH",
            ));
        }

        // 2. Patch genesis.json if any pre-mints were requested
        if !self.pre_mint.is_empty() {
            patch_genesis_balances(&chain_home, &self.pre_mint)?;
        }

        // 3. Start the chain in the background
        let poled = Command::new(&poled_bin)
            .args(["start", "--home"])
            .arg(&chain_home)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        // 4. Wire up the bridge client
        let endpoint = CosmosEndpoint {
            rpc_url: rpc_url.clone(),
            rest_url: rest_url.clone(),
            chain_id: chain_id.clone(),
            address_prefix: prefix.clone(),
        };
        let client = CosmosClient::new(endpoint)?;

        // 5. Validator keypair + derived bech32
        let validator_key = KeyPair::from_seed(&[42u8; 32]);
        let mut account = validator_key.address.to_vec();
        account.truncate(20);
        let bech32 = address::encode_bech32(&prefix, &account)?;
        let validator_address = CosmosAddress { account, bech32 };

        let node_config = tmp.path().join("node.json");
        let harness = IntegrationHarness {
            tmp,
            chain_home,
            node_config,
            chain_id,
            rpc_url,
            rest_url,
            address_prefix: prefix,
            validator_key,
            validator_address,
            poled: Some(poled),
            client,
        };
        harness.wait_for_rpc().await?;
        Ok(harness)
    }
}

/// High-level handle. Drop kills the child process.
pub struct IntegrationHarness {
    pub tmp: TempDir,
    pub chain_home: PathBuf,
    pub node_config: PathBuf,
    pub chain_id: String,
    pub rpc_url: String,
    pub rest_url: String,
    pub address_prefix: String,
    pub validator_key: KeyPair,
    pub validator_address: CosmosAddress,
    pub poled: Option<Child>,
    pub client: CosmosClient,
}

impl IntegrationHarness {
    /// Wait until `/status` returns a positive block height.
    pub async fn wait_for_rpc(&self) -> Result<(), HarnessError> {
        let url = self.rpc_url.clone();
        let deadline = Instant::now() + DEFAULT_OP_TIMEOUT;
        while Instant::now() < deadline {
            if let Ok(h) = self.client.rpc.latest_height().await {
                if h > 0 {
                    return Ok(());
                }
            }
            sleep(Duration::from_millis(500)).await;
        }
        Err(HarnessError::ChainNotReady {
            url,
            secs: DEFAULT_OP_TIMEOUT.as_secs(),
        })
    }

    /// Run a `pole-node` subcommand. Captures stdout/stderr.
    pub fn run_pole_node(&self, args: &[&str]) -> Result<std::process::Output, HarnessError> {
        let output = Command::new("pole-node")
            .args(args)
            .arg("--config")
            .arg(&self.node_config)
            .current_dir(self.tmp.path())
            .output()?;
        Ok(output)
    }

    /// `MsgUpsertNode` for the validator's keypair.
    pub async fn register_node(
        &self,
        capabilities: RegisteredNodeCapabilities,
    ) -> Result<RegisteredNode, HarnessError> {
        let node_json = serde_json::json!({
            "operator_address": self.validator_address.bech32,
            "node_id_hex": hex::encode(self.validator_key.public),
            "capabilities": {
                "collect": capabilities.collect,
                "store": capabilities.store,
                "verify": capabilities.verify,
                "propose": capabilities.propose,
            },
            "active": true,
        });
        let msg = BridgeMessage::Unsupported {
            type_url: "/pole.node.v1.MsgUpsertNode".into(),
            note: node_json.to_string(),
        };
        let resp = self
            .client
            .submit(
                &msg,
                &self.validator_address,
                &self.validator_key,
                &Default::default(),
            )
            .await?;
        if !resp.is_ok() {
            return Err(HarnessError::ChainRejected {
                code: resp.code,
                log: resp.log,
            });
        }
        Ok(RegisteredNode {
            operator_bech32: self.validator_address.bech32.clone(),
            node_id_hex: hex::encode(self.validator_key.public),
            capabilities,
        })
    }

    /// `MsgSubmitBatch`. Skeleton: forwards a JSON projection until the
    /// typed `BatchCommit → cosmos_json` converter lands.
    pub async fn submit_batch(
        &self,
        batch_json: serde_json::Value,
    ) -> Result<String, HarnessError> {
        let msg = BridgeMessage::Unsupported {
            type_url: "/pole.replica.v1.MsgSubmitReplicaReceipt".into(),
            note: batch_json.to_string(),
        };
        let resp = self
            .client
            .submit(
                &msg,
                &self.validator_address,
                &self.validator_key,
                &Default::default(),
            )
            .await?;
        if !resp.is_ok() {
            return Err(HarnessError::ChainRejected {
                code: resp.code,
                log: resp.log,
            });
        }
        Ok(resp.tx_hash)
    }

    /// `MsgCommitEpoch` placeholder. Skeleton: reuses `UpsertNode` as
    /// a stand-in so the test scaffolding can compile.
    pub async fn commit_epoch(
        &self,
        _epoch_id: u64,
        epoch_json: serde_json::Value,
    ) -> Result<String, HarnessError> {
        let msg = BridgeMessage::Unsupported {
            type_url: "/pole.epoch.v1.MsgCommitEpoch".into(),
            note: epoch_json.to_string(),
        };
        let resp = self
            .client
            .submit(
                &msg,
                &self.validator_address,
                &self.validator_key,
                &Default::default(),
            )
            .await?;
        if !resp.is_ok() {
            return Err(HarnessError::ChainRejected {
                code: resp.code,
                log: resp.log,
            });
        }
        Ok(resp.tx_hash)
    }

    /// `MsgClaimReward` for the validator's reward address.
    pub async fn claim_reward(&self, epoch_id: u64) -> Result<String, HarnessError> {
        let msg = BridgeMessage::ClaimReward {
            claimer: self.validator_address.clone(),
            epoch_id,
            recipient: self.validator_address.clone(),
        };
        let resp = self
            .client
            .submit(
                &msg,
                &self.validator_address,
                &self.validator_key,
                &Default::default(),
            )
            .await?;
        if !resp.is_ok() {
            return Err(HarnessError::ChainRejected {
                code: resp.code,
                log: resp.log,
            });
        }
        Ok(resp.tx_hash)
    }

    /// Stub: open a challenge. Returns `Unimplemented` until the
    /// challenge JSON projection lands.
    pub async fn open_challenge(&self, _epoch_id: u64) -> Result<String, HarnessError> {
        Err(HarnessError::Unimplemented("open_challenge"))
    }

    /// Read the current account sequence for `address`. Useful for
    /// tests that want to assert "the chain processed N txs".
    pub async fn current_sequence(&self, address: &str) -> Result<u64, HarnessError> {
        let info = self.client.account(address).await?;
        info.sequence
            .parse::<u64>()
            .map_err(|e| HarnessError::Json(serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e.to_string(),
            ))))
    }
}

impl Drop for IntegrationHarness {
    fn drop(&mut self) {
        if let Some(mut child) = self.poled.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Patch `genesis.json` to add the requested balances. Operates on the
/// standard `app_state.bank.balances` shape.
fn patch_genesis_balances(
    chain_home: &std::path::Path,
    mints: &[(String, u128)],
) -> Result<(), HarnessError> {
    let genesis_path = chain_home.join("config/genesis.json");
    let raw = std::fs::read_to_string(&genesis_path)?;
    let mut genesis: serde_json::Value = serde_json::from_str(&raw)?;

    let bank = genesis
        .pointer_mut("/app_state/bank")
        .ok_or(HarnessError::Missing("app_state.bank"))?;
    let balances = bank
        .pointer_mut("/balances")
        .and_then(|v| v.as_array_mut())
        .ok_or(HarnessError::Missing("app_state.bank.balances"))?;

    for (addr, amount) in mints {
        balances.push(serde_json::json!({
            "address": addr,
            "coins": [{ "denom": "upole", "amount": amount.to_string() }],
        }));
    }
    std::fs::write(&genesis_path, serde_json::to_string_pretty(&genesis)?)?;
    Ok(())
}
