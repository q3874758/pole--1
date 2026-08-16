//! Minimal tx-JSON projection helpers for the bridge skeleton.
//!
//! This module was slimmed down from a full chain-bridge module: every
//! other helper (export_*/parse_*/build_*/generate_* variants,
//! `CosmosBridge`, `CosmosBridgeConfig`, output structs, ...) had zero
//! callers anywhere in the crate 鈥?a legacy from the pre-`wire_types`
//! era. The real proto3 wire encoders live in
//! `cosmos::pole_msgs` / `cosmos::tx_builder`; only `pole-client`'s
//! `submit-batch` / `submit-epoch` / `export-tx` commands still print
//! these JSON tx projections, so exactly their dependency closure is
//! kept here.

use base64::Engine as _;
use serde::Serialize;

use crate::records::{BatchCommit, EpochCommit};

/// A tx projection: `type_url` + JSON-serialized `value` (skeleton
/// format 鈥?NOT the proto3 wire encoding used by `tx_builder`).
pub struct CosmosTxMessage {
    pub type_url: String,
    pub value: Vec<u8>,
}

impl CosmosTxMessage {
    pub fn submit_batch(
        collector: &str,
        batch_commit_json: &serde_json::Value,
    ) -> Result<Self, serde_json::Error> {
        #[derive(Serialize)]
        struct MsgSubmitBatch {
            collector: String,
            #[serde(rename = "batch_commit")]
            batch_commit: serde_json::Value,
        }

        let msg = MsgSubmitBatch {
            collector: collector.to_string(),
            batch_commit: batch_commit_json.clone(),
        };

        let value = serde_json::to_vec(&msg)?;

        Ok(Self {
            type_url: "/pole.chain.pole.v1.MsgSubmitBatch".to_string(),
            value,
        })
    }

    pub fn commit_epoch(
        proposer: &str,
        epoch_commit_json: &serde_json::Value,
    ) -> Result<Self, serde_json::Error> {
        #[derive(Serialize)]
        struct MsgCommitEpoch {
            proposer: String,
            #[serde(rename = "epoch_commit")]
            epoch_commit: serde_json::Value,
        }

        let msg = MsgCommitEpoch {
            proposer: proposer.to_string(),
            epoch_commit: epoch_commit_json.clone(),
        };

        let value = serde_json::to_vec(&msg)?;

        Ok(Self {
            type_url: "/pole.chain.pole.v1.MsgCommitEpoch".to_string(),
            value,
        })
    }
}

pub fn build_batch_submit_tx(
    collector_hex: &str,
    batch_commit_json: &serde_json::Value,
) -> Result<CosmosTxMessage, serde_json::Error> {
    CosmosTxMessage::submit_batch(collector_hex, batch_commit_json)
}

pub fn build_epoch_commit_tx(
    proposer_hex: &str,
    epoch_commit_json: &serde_json::Value,
) -> Result<CosmosTxMessage, serde_json::Error> {
    CosmosTxMessage::commit_epoch(proposer_hex, epoch_commit_json)
}

pub fn batch_commit_to_cosmos_json(batch: &BatchCommit) -> serde_json::Value {
    serde_json::json!({
        "epoch_id": batch.epoch_id,
        "collector_address": hex::encode(batch.collector_id),
        "slot_start": batch.slot_start,
        "slot_end": batch.slot_end,
        "batch": {
            "root": hex::encode(batch.batch.root),
            "leaf_count": batch.batch.leaf_count,
        },
        "payload_cid": batch.payload_cid,
        "observation_count": batch.obs_count,
        "submitted_at_height": batch.submitted_at_height,
    })
}

pub fn epoch_commit_to_cosmos_json(commit: &EpochCommit) -> serde_json::Value {
    serde_json::json!({
        "epoch_id": commit.epoch_id,
        "accepted_batches": {
            "root": hex::encode(commit.accepted_batches.root),
            "leaf_count": commit.accepted_batches.leaf_count,
        },
        "observations": {
            "root": hex::encode(commit.observations.root),
            "leaf_count": commit.observations.leaf_count,
        },
        "aggregates": {
            "root": hex::encode(commit.aggregates.root),
            "leaf_count": commit.aggregates.leaf_count,
        },
        "rewards": {
            "root": hex::encode(commit.rewards.root),
            "leaf_count": commit.rewards.leaf_count,
        },
        "availability": {
            "root": hex::encode(commit.availability.root),
            "leaf_count": commit.availability.leaf_count,
        },
        "randomness_seed_hex": hex::encode(commit.randomness_seed),
        "proposer_address": hex::encode(commit.proposer_id),
        "challenge_open_height": commit.challenge_open_height,
        "challenge_deadline_height": commit.challenge_deadline_height,
    })
}

pub fn generate_tx_json_for_batch(
    collector_hex: &str,
    batch: &BatchCommit,
) -> Result<String, serde_json::Error> {
    let cosmos_json = batch_commit_to_cosmos_json(batch);
    let tx = build_batch_submit_tx(collector_hex, &cosmos_json)?;
    let wrapper = serde_json::json!({
        "type_url": tx.type_url,
        "value": base64::engine::general_purpose::STANDARD.encode(&tx.value),
    });
    serde_json::to_string_pretty(&wrapper)
}

pub fn generate_tx_json_for_epoch_commit(
    proposer_hex: &str,
    commit: &EpochCommit,
) -> Result<String, serde_json::Error> {
    let cosmos_json = epoch_commit_to_cosmos_json(commit);
    let tx = build_epoch_commit_tx(proposer_hex, &cosmos_json)?;
    let wrapper = serde_json::json!({
        "type_url": tx.type_url,
        "value": base64::engine::general_purpose::STANDARD.encode(&tx.value),
    });
    serde_json::to_string_pretty(&wrapper)
}
