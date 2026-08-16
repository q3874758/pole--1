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

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tempfile::TempDir;
use tokio::time::sleep;

use pole_protocol_draft::cosmos::wire_types::{
    AggregateRecordWire, BatchCommitWire, EpochCommitWire, MerkleCommitmentWire,
    NodeCapabilitySetWire, NodeRecordWire, NodeRoleWire,
};
use pole_protocol_draft::cosmos::{
    address, BridgeMessage, CosmosAddress, CosmosClient, CosmosEndpoint,
};
use pole_protocol_draft::records::{Challenge, ChallengeEvidenceRef};
use pole_protocol_draft::wallet::KeyPair;
use pole_protocol_draft::{decode_hex32, ChallengeKind, ChallengeState, Hash32, NodeId};

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

    #[error("parse: {0}")]
    Parse(String),

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
        let chain_id = self
            .chain_id
            .unwrap_or_else(|| DEFAULT_CHAIN_ID.to_string());
        let rpc_url = self.rpc_url.unwrap_or_else(|| DEFAULT_RPC_URL.to_string());
        let rest_url = self
            .rest_url
            .unwrap_or_else(|| DEFAULT_REST_URL.to_string());
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

        // 0. Deterministic validator keypair + derived bech32. The
        // bridge client signs with this keypair, so the account must
        // exist and hold `upole` before the genesis is finalized. The
        // account id is sha256(pubkey)[..20] — the chain derives the
        // signer address from the pubkey via tmhash.SumTruncated.
        let validator_key = KeyPair::from_seed(&[42u8; 32]);
        let account = address::cosmos_account_from_pubkey(&validator_key.public).to_vec();
        let validator_bech32 = address::encode_bech32(&prefix, &account)?;
        let validator_address = CosmosAddress {
            account,
            bech32: validator_bech32.clone(),
        };

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

        // 1.5. Fund the bridge validator account (`upole` covers tx fees).
        poled_run(
            &poled_bin,
            &chain_home,
            &[
                "genesis",
                "add-genesis-account",
                &validator_bech32,
                "1000000000000upole",
                "--keyring-backend",
                "test",
            ],
        )?;

        // 2. Patch genesis.json if any pre-mints were requested
        if !self.pre_mint.is_empty() {
            patch_genesis_balances(&chain_home, &self.pre_mint)?;
        }

        // 2.2. Seed the pole module with a finalized epoch-1 commit and
        // a reward record so `claim_reward` can resolve end-to-end.
        patch_pole_genesis(&chain_home, &validator_bech32)?;

        // 2.5. Bootstrap a single validator (a bare init leaves the
        // validator set empty, preventing block production).
        bootstrap_validator(&poled_bin, &chain_home, &chain_id)?;

        // 3. Start the chain in the background (stderr captured so
        // chain-side rejections are debuggable after a failed test).
        let poled_log = tmp.path().join("poled.log");
        let poled = Command::new(&poled_bin)
            .args(["start", "--home"])
            .arg(&chain_home)
            .stdout(Stdio::null())
            .stderr(Stdio::from(std::fs::File::create(&poled_log)?))
            .spawn()?;

        // 4. Wire up the bridge client
        let endpoint = CosmosEndpoint {
            rpc_url: rpc_url.clone(),
            rest_url: rest_url.clone(),
            chain_id: chain_id.clone(),
            address_prefix: prefix.clone(),
        };
        let client = CosmosClient::new(endpoint)?;

        let node_config = tmp.path().join("node.json");
        let harness = IntegrationHarness {
            tmp,
            poled_log,
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
    pub poled_log: PathBuf,
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
        let caps = NodeCapabilitySetWire {
            collect: capabilities.collect,
            store: capabilities.store,
            verify: capabilities.verify,
            propose: capabilities.propose,
        };
        let node = NodeRecordWire {
            operator_address: self.validator_address.bech32.clone(),
            reward_address: self.validator_address.bech32.clone(),
            consensus_address: String::new(),
            role: NodeRoleWire::Player,
            capabilities: caps,
            active: true,
            bonded_tokens: 0,
            last_updated_epoch: 0,
            is_player: false,
        };
        let msg = BridgeMessage::UpsertNode {
            operator: self.validator_address.clone(),
            node,
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

    /// `MsgSubmitBatch` for the validator's collector account. The
    /// batch is built from `batch_json` when it carries the wire
    /// fields, otherwise a minimal valid batch is used.
    pub async fn submit_batch(
        &self,
        batch_json: serde_json::Value,
    ) -> Result<String, HarnessError> {
        let epoch_id = batch_json
            .get("epoch_id")
            .and_then(|v| v.as_u64())
            .unwrap_or(1);
        let batch_commit = BatchCommitWire {
            epoch_id,
            collector_address: self.validator_address.bech32.clone(),
            slot_start: 1,
            slot_end: 100,
            batch: MerkleCommitmentWire {
                root: "00".repeat(32),
                leaf_count: 1,
            },
            payload_cid: "bafy-test-cid".into(),
            observation_count: 1,
            submitted_at_height: 0,
        };
        let msg = BridgeMessage::SubmitBatch {
            collector: self.validator_address.clone(),
            batch_commit,
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

    /// `MsgCommitEpoch` for the validator's proposer account. A zero
    /// `deadline_height` falls back to `latest height + 3` so the epoch
    /// enters its challenge window and `finalize_epoch` can run a few
    /// blocks later.
    pub async fn commit_epoch(
        &self,
        epoch_id: u64,
        deadline_height: i64,
    ) -> Result<String, HarnessError> {
        let deadline = if deadline_height > 0 {
            deadline_height
        } else {
            self.client.rpc.latest_height().await? as i64 + 3
        };
        let commit = EpochCommitWire {
            epoch_id,
            accepted_batches: MerkleCommitmentWire {
                root: "11".repeat(32),
                leaf_count: 1,
            },
            observations: MerkleCommitmentWire {
                root: "22".repeat(32),
                leaf_count: 1,
            },
            aggregates: MerkleCommitmentWire {
                root: "00".repeat(32),
                leaf_count: 0,
            },
            rewards: MerkleCommitmentWire {
                root: "44".repeat(32),
                leaf_count: 1,
            },
            availability: MerkleCommitmentWire {
                root: "55".repeat(32),
                leaf_count: 1,
            },
            randomness_seed_hex: "66".repeat(32),
            proposer_address: self.validator_address.bech32.clone(),
            challenge_open_height: 0,
            challenge_deadline_height: deadline,
            finalized: false,
            total_network_weight_units: 0,
        };
        let msg = BridgeMessage::CommitEpoch {
            proposer: self.validator_address.clone(),
            epoch_commit: commit,
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

    /// `MsgUpsertAggregateRecord` (verify capability). Refreshes the
    /// epoch's aggregates commitment so `FinalizeEpoch` can validate.
    pub async fn upsert_aggregate_record(&self, epoch_id: u64) -> Result<String, HarnessError> {
        let aggregate = AggregateRecordWire {
            epoch_id,
            app_id: 730,
            total_weight_units: 1_000,
            player_count: 1,
        };
        let msg = BridgeMessage::UpsertAggregateRecord {
            operator: self.validator_address.clone(),
            aggregate_record: aggregate,
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

    /// `MsgFinalizeEpoch`. Polls the chain until the height passes
    /// `after_height`, then broadcasts; if the chain still rejects it
    /// (challenge window not yet elapsed) it retries until accepted.
    /// A `msg` that is permanently rejected (root mismatch etc.) will
    /// surface as a `ChainRejected` on the last attempt before timeout.
    pub async fn finalize_epoch(
        &self,
        epoch_id: u64,
        after_height: i64,
    ) -> Result<String, HarnessError> {
        let deadline = Instant::now() + Duration::from_secs(90);
        let mut last_rejected: Option<HarnessError> = None;
        loop {
            if let Ok(h) = self.client.rpc.latest_height().await {
                if h as i64 > after_height {
                    let msg = BridgeMessage::FinalizeEpoch {
                        finalizer: self.validator_address.clone(),
                        epoch_id,
                    };
                    match self
                        .client
                        .submit(
                            &msg,
                            &self.validator_address,
                            &self.validator_key,
                            &Default::default(),
                        )
                        .await
                    {
                        Ok(resp) if resp.is_ok() => return Ok(resp.tx_hash),
                        Ok(resp) => {
                            last_rejected = Some(HarnessError::ChainRejected {
                                code: resp.code,
                                log: resp.log,
                            });
                        }
                        Err(err) => {
                            last_rejected = Some(HarnessError::Cosmos(err));
                        }
                    }
                }
            }
            if Instant::now() > deadline {
                return Err(last_rejected.unwrap_or(HarnessError::ChainNotReady {
                    url: self.rpc_url.clone(),
                    secs: 90,
                }));
            }
            sleep(Duration::from_millis(750)).await;
        }
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

    /// `MsgOpenChallenge` (verify capability) against `target_hex`
    /// (lowercase hex of the target `NodeId`). Requires a committed
    /// epoch; the genesis seed provides epoch 1, or use `commit_epoch`.
    pub async fn open_challenge(
        &self,
        epoch_id: u64,
        target_hex: &str,
        deadline_height: u64,
        challenge_id: Hash32,
    ) -> Result<String, HarnessError> {
        let target: NodeId = decode_hex32(target_hex, "target_hex")
            .map_err(|e| HarnessError::Parse(e.to_string()))?;
        let challenge = Challenge {
            challenge_id,
            kind: ChallengeKind::BadBatch,
            epoch_id,
            target_node: Some(target),
            challenger: [0u8; 32],
            bond: 1_000,
            opened_at_height: 0,
            deadline_height,
            state: ChallengeState::Open,
            evidence: ChallengeEvidenceRef {
                batch_root: Some([0x77u8; 32]),
                aggregate_root: None,
                reward_root: None,
                payload_cid: None,
                merkle_proof: Vec::new(),
            },
        };
        let msg = BridgeMessage::OpenChallenge {
            challenger: self.validator_address.clone(),
            challenge,
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

    /// Read the captured `poled` stderr log. Useful for diagnosing
    /// chain-side rejections after a failed broadcast.
    pub fn poled_log_text(&self) -> String {
        std::fs::read_to_string(&self.poled_log).unwrap_or_default()
    }

    /// Read the current account sequence for `address`. Useful for
    /// tests that want to assert "the chain processed N txs".
    pub async fn current_sequence(&self, address: &str) -> Result<u64, HarnessError> {
        let info = self.client.account(address).await?;
        info.sequence.parse::<u64>().map_err(|e| {
            HarnessError::Json(serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e.to_string(),
            )))
        })
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

/// Runs a poled subcommand with --home appended, erroring on non-zero exit.
fn poled_run(poled_bin: &Path, chain_home: &Path, args: &[&str]) -> Result<(), HarnessError> {
    let status = Command::new(poled_bin)
        .args(args)
        .arg("--home")
        .arg(chain_home)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if !status.success() {
        return Err(HarnessError::Unimplemented(
            "poled validator bootstrap command returned non-zero",
        ));
    }
    Ok(())
}

/// Bootstraps a single validator so a freshly-initialized chain can produce
/// blocks: adds a test keyring account, funds it with stake, creates a genesis
/// transaction, and collects it into the validator set.
fn bootstrap_validator(
    poled_bin: &Path,
    chain_home: &Path,
    chain_id: &str,
) -> Result<(), HarnessError> {
    poled_run(
        poled_bin,
        chain_home,
        &["keys", "add", "test-validator", "--keyring-backend", "test"],
    )?;
    poled_run(
        poled_bin,
        chain_home,
        &[
            "genesis",
            "add-genesis-account",
            "test-validator",
            "1000000000stake",
            "--keyring-backend",
            "test",
        ],
    )?;
    poled_run(
        poled_bin,
        chain_home,
        &[
            "genesis",
            "gentx",
            "test-validator",
            "1000000stake",
            "--chain-id",
            chain_id,
            "--keyring-backend",
            "test",
        ],
    )?;
    poled_run(poled_bin, chain_home, &["genesis", "collect-gentxs"])?;
    Ok(())
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

/// Seed the pole module's genesis state so the bridge happy path can
/// claim an epoch-1 reward: a finalized `EpochCommit` plus a non-zero
/// `RewardRecord` for `recipient`. Without these the chain rejects
/// `MsgClaimReward` (epoch not finalized / no reward record).
///
/// The verification-coverage gates (`min_verification_count`,
/// `min_player_verifier_share_bps`) are also zeroed for the test
/// environment — they are governance-tunable parameters, and the harness
/// runs a single validator that cannot broadcast three independent
/// `MsgVerifyBatch` attestations.
fn patch_pole_genesis(chain_home: &std::path::Path, recipient: &str) -> Result<(), HarnessError> {
    let genesis_path = chain_home.join("config/genesis.json");
    let raw = std::fs::read_to_string(&genesis_path)?;
    let mut genesis: serde_json::Value = serde_json::from_str(&raw)?;

    let pole = genesis
        .pointer_mut("/app_state/pole")
        .ok_or(HarnessError::Missing("app_state.pole"))?;

    pole["epoch_commits"] = serde_json::json!([{
        "epoch_id": 1,
        "finalized": true,
        "challenge_open_height": 0,
        "challenge_deadline_height": 1,
    }]);

    pole["reward_records"] = serde_json::json!([{
        "epoch_id": 1,
        "recipient": recipient,
        "player_reward": 1000,
        "collect_reward": 0,
        "store_reward": 0,
        "verify_reward": 0,
        "propose_reward": 0,
        "slash_debit": 0,
        "net_reward": 1000,
    }]);

    if let Some(params) = pole.get_mut("params") {
        params["min_verification_count"] = serde_json::json!(0);
        params["min_player_verifier_share_bps"] = serde_json::json!(0);
    }

    std::fs::write(&genesis_path, serde_json::to_string_pretty(&genesis)?)?;
    Ok(())
}
