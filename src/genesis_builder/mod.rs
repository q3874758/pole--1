//! `pole-genesis`: deterministic generator for the PoLE `genesis.json`.
//!
//! The tool consumes three inputs:
//! - `--allocations <csv>`   — `address,amount_upole` per row
//! - `--validators <json>`   — list of validator descriptors
//! - `--params <json>`       — overrides for `ProtocolParams`
//!
//! and emits a single `genesis.json` that `poled validate-genesis`
//! accepts.
//!
//! Skeleton limitations:
//! - Token allocations assume the standard `upole` denom. Multi-denom
//!   allocations are not supported yet.
//! - Validator signing keys are read as raw 32-byte hex from the input
//!   JSON. Production should switch to bech32-encoded consensus pubkeys.
//! - The output is *not yet* identical to a hand-tuned genesis; the
//!   defaults below approximate the whitepaper allocation but should be
//!   re-derived from the official allocation sheet before launch.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Mainnet-scale default: 1 trillion `upole` total supply.
pub const DEFAULT_TOTAL_SUPPLY_UPOLES: u128 = 1_000_000_000_000_000;
pub const MICROS_PER_UPOLES: u128 = 1_000_000;

#[derive(Debug, Error)]
pub enum GenesisError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("csv parse error at line {line}: {message}")]
    Csv { line: usize, message: String },

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("validation: {0}")]
    Validation(String),

    #[error("missing required field: {0}")]
    Missing(&'static str),
}

pub type Result<T> = std::result::Result<T, GenesisError>;

/// One row in the `allocations.csv` file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Allocation {
    pub address: String,
    /// Whole `upole` amount. Converted to micro-units on write.
    pub amount_upole: u128,
}

/// One validator entry. `consensus_pubkey_hex` is the 32-byte ed25519
/// public key in lowercase hex.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorSpec {
    pub moniker: String,
    pub operator_address: String,
    pub consensus_pubkey_hex: String,
    pub stake_upole: u128,
}

/// Top-level input shape.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GenesisInputs {
    pub chain_id: String,
    pub allocations: Vec<Allocation>,
    pub validators: Vec<ValidatorSpec>,
    #[serde(default)]
    pub params_overrides: serde_json::Value,
}

/// Field-level knobs that get merged into `app_state.pole.params`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoleParamsOverrides {
    pub reward_block_duration_seconds: Option<u64>,
    pub base_hourly_reward: Option<u128>,
    pub target_network_weight_units: Option<u128>,
    pub reward_adjustment_cap_bps: Option<u16>,
    pub challenge_window_blocks: Option<u32>,
    pub min_retention_epochs: Option<u32>,
    pub player_reward_allocation_bps: Option<u16>,
    pub service_reward_allocation_bps: Option<u16>,
}

/// Loader entry points.
pub mod input;
pub mod output;

pub use input::{load_allocations_csv, load_validators_json};
pub use output::write_genesis;

/// Builder façade. Lets the binary stay thin and lets tests exercise
/// the construction without going through the filesystem.
pub struct GenesisBuilder {
    inputs: GenesisInputs,
}

impl GenesisBuilder {
    pub fn new(inputs: GenesisInputs) -> Self {
        Self { inputs }
    }

    /// Read the inputs from the standard CLI flags.
    pub fn from_paths(
        chain_id: String,
        allocations: PathBuf,
        validators: PathBuf,
        params: Option<PathBuf>,
    ) -> Result<Self> {
        let allocations = load_allocations_csv(&allocations)?;
        let validators = load_validators_json(&validators)?;
        let params_overrides = match params {
            Some(p) => {
                let raw = std::fs::read_to_string(&p)?;
                serde_json::from_str(&raw)?
            }
            None => serde_json::Value::Null,
        };
        Ok(Self {
            inputs: GenesisInputs {
                chain_id,
                allocations,
                validators,
                params_overrides,
            },
        })
    }

    /// Build the genesis document as a `serde_json::Value`. Validates
    /// invariants (sum of allocations ≤ total supply, ≥ 1 validator)
    /// before returning.
    pub fn build(&self) -> Result<serde_json::Value> {
        self.validate()?;
        let mut doc = self.skeleton();
        self.populate_bank(&mut doc)?;
        self.populate_staking(&mut doc)?;
        self.populate_pole_params(&mut doc)?;
        self.populate_epochs(&mut doc)?;
        Ok(doc)
    }

    /// Convenience: build + write to `out`.
    pub fn write(&self, out: &Path) -> Result<()> {
        let doc = self.build()?;
        write_genesis(out, &doc)
    }

    fn validate(&self) -> Result<()> {
        if self.inputs.chain_id.is_empty() {
            return Err(GenesisError::Missing("chain_id"));
        }
        if self.inputs.validators.is_empty() {
            return Err(GenesisError::Validation(
                "at least one validator is required".into(),
            ));
        }
        let total: u128 = self.inputs.allocations.iter().map(|a| a.amount_upole).sum();
        if total > DEFAULT_TOTAL_SUPPLY_UPOLES {
            return Err(GenesisError::Validation(format!(
                "allocations sum {} upole exceeds total supply {} upole",
                total, DEFAULT_TOTAL_SUPPLY_UPOLES
            )));
        }
        for v in &self.inputs.validators {
            if v.consensus_pubkey_hex.len() != 64 {
                return Err(GenesisError::Validation(format!(
                    "validator {} has invalid pubkey length (expected 64 hex chars)",
                    v.moniker
                )));
            }
            hex::decode(&v.consensus_pubkey_hex).map_err(|e| {
                GenesisError::Validation(format!("validator {} pubkey: {}", v.moniker, e))
            })?;
        }
        Ok(())
    }

    fn skeleton(&self) -> serde_json::Value {
        serde_json::json!({
            "chain_id": self.inputs.chain_id,
            "app_state": {
                "bank": { "balances": [], "supply": [], "params": {} },
                "staking": {
                    "params": { "bond_denom": "upole" },
                    "validators": []
                },
                "pole": {
                    "params": default_pole_params(),
                    "nodes": []
                },
                "epochs": {
                    "epochs": [{
                        "identifier": "hour",
                        "duration": "3600s",
                        "current_epoch": "1",
                        "current_epoch_start_time": "2026-06-01T00:00:00Z",
                        "epoch_counting_started": true,
                        "current_epoch_start_height": "1"
                    }]
                }
            }
        })
    }

    fn populate_bank(&self, doc: &mut serde_json::Value) -> Result<()> {
        let mut balances: Vec<serde_json::Value> = Vec::new();
        let mut supply_by_denom: BTreeMap<String, u128> = BTreeMap::new();

        for a in &self.inputs.allocations {
            let micros = a
                .amount_upole
                .checked_mul(MICROS_PER_UPOLES)
                .ok_or_else(|| GenesisError::Validation("overflow on amount".into()))?;
            balances.push(serde_json::json!({
                "address": a.address,
                "coins": [{ "denom": "upole", "amount": micros.to_string() }],
            }));
            *supply_by_denom.entry("upole".into()).or_insert(0) += micros;
        }
        let supply: Vec<serde_json::Value> = supply_by_denom
            .into_iter()
            .map(|(denom, amount)| serde_json::json!({ "denom": denom, "amount": amount.to_string() }))
            .collect();

        doc["app_state"]["bank"]["balances"] = serde_json::Value::Array(balances);
        doc["app_state"]["bank"]["supply"] = serde_json::Value::Array(supply);
        Ok(())
    }

    fn populate_staking(&self, doc: &mut serde_json::Value) -> Result<()> {
        let validators: Vec<serde_json::Value> = self
            .inputs
            .validators
            .iter()
            .map(|v| {
                let stake = v.stake_upole * MICROS_PER_UPOLES;
                serde_json::json!({
                    "operator_address": v.operator_address,
                    "consensus_pubkey": {
                        "@type": "/cosmos.crypto.ed25519.PubKey",
                        "key": base64::Engine::encode(
                            &base64::engine::general_purpose::STANDARD,
                            hex::decode(&v.consensus_pubkey_hex).unwrap(),
                        ),
                    },
                    "tokens": stake.to_string(),
                    "moniker": v.moniker,
                    "description": { "moniker": v.moniker },
                })
            })
            .collect();
        doc["app_state"]["staking"]["validators"] = serde_json::Value::Array(validators);
        Ok(())
    }

    fn populate_pole_params(&self, doc: &mut serde_json::Value) -> Result<()> {
        // Merge any caller-supplied overrides on top of the defaults.
        if let Some(obj) = self.inputs.params_overrides.as_object() {
            let params = doc["app_state"]["pole"]["params"]
                .as_object_mut()
                .ok_or(GenesisError::Missing("app_state.pole.params"))?;
            for (k, v) in obj {
                params.insert(k.clone(), v.clone());
            }
        }
        Ok(())
    }

    fn populate_epochs(&self, _doc: &mut serde_json::Value) -> Result<()> {
        // Skeleton already populates the hours epoch in `skeleton()`.
        Ok(())
    }
}

/// Returns the default `app_state.pole.params` block. Values are
/// derived from the whitepaper (1 trillion upole, 2% year-1 emission,
/// 80% player / 10% service split) and intentionally conservative.
fn default_pole_params() -> serde_json::Value {
    // base_hourly_reward = total_supply * 0.02 / (365 * 24) in micro-units
    // = 1e15 * 0.02 / 8760 ≈ 2_283_105_022.83 → 2_283_105_022
    let base_hourly_reward: u128 = (DEFAULT_TOTAL_SUPPLY_UPOLES
        * MICROS_PER_UPOLES
        * 2
        / 100)
        / (365 * 24);

    serde_json::json!({
        "reward_block_duration_seconds": 3600,
        "base_hourly_reward": base_hourly_reward,
        "target_network_weight_units": 150_000_000_000_000u128,
        "reward_adjustment_cap_bps": 2000,
        "challenge_window_blocks": 20,
        "min_retention_epochs": 2,
        "player_reward_allocation_bps": 8000,
        "service_reward_allocation_bps": 1000,
        "collect_reward_bps": 5000,
        "store_reward_bps": 2500,
        "verify_reward_bps": 1500,
        "propose_reward_bps": 1000,
        "tier1_weight_ppm": 1_000_000,
        "tier2_weight_min_ppm": 300_000,
        "tier2_weight_max_ppm": 600_000,
        "tier3_weight_min_ppm": 50_000,
        "tier3_weight_max_ppm": 150_000,
        "fee_burn_bps": 2500,
        "reward_burn_threshold": 10000,
        "reward_burn_bps": 1000,
        "governance_burn_bps": 100,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Build a CSV in a leaked tempdir so the path stays valid for the
    /// rest of the test. (Cargo's test runner isolates each test in
    /// its own process group, so the leak is fine.)
    fn tmp_csv(rows: &[&str]) -> PathBuf {
        let dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let path = dir.path().join("allocations.csv");
        let mut f = std::fs::File::create(&path).unwrap();
        for r in rows {
            writeln!(f, "{}", r).unwrap();
        }
        path
    }

    fn tmp_json_validators() -> PathBuf {
        let dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let path = dir.path().join("validators.json");
        std::fs::write(
            &path,
            r#"[
              {
                "moniker": "v1",
                "operator_address": "cosmosvaloper1xxx",
                "consensus_pubkey_hex": "0000000000000000000000000000000000000000000000000000000000000001",
                "stake_upole": 1000000
              }
            ]"#,
        )
        .unwrap();
        path
    }

    fn tmp_empty_validators() -> PathBuf {
        let dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let path = dir.path().join("empty.json");
        std::fs::write(&path, "[]").unwrap();
        path
    }

    #[test]
    fn build_produces_valid_skeleton() {
        let csv = tmp_csv(&[
            "cosmos1abc,1000",
            "cosmos1def,2000",
        ]);
        let vals = tmp_json_validators();
        let builder = GenesisBuilder::from_paths(
            "pole-test".into(),
            csv,
            vals,
            None,
        )
        .unwrap();
        let doc = builder.build().unwrap();
        let balances = doc["app_state"]["bank"]["balances"].as_array().unwrap();
        assert_eq!(balances.len(), 2);
        assert_eq!(balances[0]["coins"][0]["denom"], "upole");
    }

    #[test]
    fn rejects_empty_validator_set() {
        let csv = tmp_csv(&["cosmos1abc,100"]);
        let empty_vals = tmp_empty_validators();
        let builder = GenesisBuilder::from_paths(
            "pole-test".into(),
            csv,
            empty_vals,
            None,
        )
        .unwrap();
        assert!(builder.build().is_err());
    }

    #[test]
    fn rejects_overallocation() {
        let csv = tmp_csv(&[&format!(
            "cosmos1abc,{}",
            DEFAULT_TOTAL_SUPPLY_UPOLES + 1
        )]);
        let vals = tmp_json_validators();
        let builder = GenesisBuilder::from_paths(
            "pole-test".into(),
            csv,
            vals,
            None,
        )
        .unwrap();
        let err = builder.build().unwrap_err();
        assert!(matches!(err, GenesisError::Validation(_)));
    }

    #[test]
    fn default_params_derive_correct_hourly_reward() {
        let json = default_pole_params();
        // 1e15 upole * 1e6 microupole/upole * 0.02 / 8760 ≈ 2.28e15
        let v: u128 = json["base_hourly_reward"].as_u64().unwrap() as u128;
        assert!(v >= 2_000_000_000_000_000 && v <= 2_500_000_000_000_000,
                "base_hourly_reward out of range: {v}");
    }
}
