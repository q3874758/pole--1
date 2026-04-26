use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::primitives::{Amount, EpochId, Height, Hash32, NodeId};
use crate::records::{AggregateRecord, BatchCommit, Challenge, ChallengeEvidenceRef, EpochCommit, ReplicaReceipt, RewardRecord};
use crate::node_pipeline::AssembledBatch;
use crate::node_settlement::EpochSettlementArtifact;
use crate::node_rewards::EpochRewardArtifact;
use crate::node_aggregator::EpochAggregationArtifact;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CosmosBridgeConfig {
    pub rpc_url: String,
    pub grpc_url: String,
    pub chain_id: String,
    pub gas_adjustment: f64,
}

impl Default for CosmosBridgeConfig {
    fn default() -> Self {
        Self {
            rpc_url: "http://localhost:26657".to_string(),
            grpc_url: "http://localhost:9090".to_string(),
            chain_id: "pole".to_string(),
            gas_adjustment: 1.2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchCommitOutput {
    pub json_path: String,
    pub binary_path: String,
    pub payload_cid: String,
    pub batch_root_hex: String,
    pub obs_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpochCommitOutput {
    pub json_path: String,
    pub epoch_id: EpochId,
    pub accepted_batches_root_hex: String,
    pub observations_root_hex: String,
    pub aggregates_root_hex: String,
    pub rewards_root_hex: String,
    pub availability_root_hex: String,
    pub challenge_deadline_height: Height,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardRecordsOutput {
    pub json_path: String,
    pub epoch_id: EpochId,
    pub reward_root_hex: String,
    pub record_count: usize,
}

pub struct CosmosBridge {
    config: CosmosBridgeConfig,
}

impl CosmosBridge {
    pub fn new(config: CosmosBridgeConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &CosmosBridgeConfig {
        &self.config
    }

    pub fn with_default_config() -> Self {
        Self::new(CosmosBridgeConfig::default())
    }
}

pub fn assemble_batch_to_json(batch: &AssembledBatch, output_path: &Path) -> Result<BatchCommitOutput, std::io::Error> {
    let rust_batch = batch.batch_commit.clone();

    let json = serde_json::to_string_pretty(&rust_batch).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    std::fs::write(output_path, &json)?;

    Ok(BatchCommitOutput {
        json_path: output_path.to_string_lossy().to_string(),
        binary_path: String::new(),
        payload_cid: rust_batch.payload_cid.clone(),
        batch_root_hex: hex_encode(&rust_batch.batch.root),
        obs_count: rust_batch.obs_count,
    })
}

pub fn epoch_settlement_to_commit_output(settlement: &EpochSettlementArtifact) -> EpochCommitOutput {
    EpochCommitOutput {
        json_path: settlement.local_chain_store_path.clone(),
        epoch_id: settlement.epoch_id,
        accepted_batches_root_hex: settlement.accepted_batches_root_hex.clone(),
        observations_root_hex: settlement.observations_root_hex.clone(),
        aggregates_root_hex: settlement.aggregates_root_hex.clone(),
        rewards_root_hex: settlement.rewards_root_hex.clone(),
        availability_root_hex: settlement.availability_root_hex.clone(),
        challenge_deadline_height: settlement.challenge_deadline_height,
    }
}

pub fn reward_records_to_json(artifact: &EpochRewardArtifact, records: &[RewardRecord], output_path: &Path) -> Result<RewardRecordsOutput, std::io::Error> {
    #[derive(Serialize)]
    struct RewardRecordsWrapper<'a> {
        epoch_id: EpochId,
        reward_root_hex: &'a str,
        reward_block_secs: u64,
        total_network_weight_units: Amount,
        records: &'a [RewardRecord],
    }

    let wrapper = RewardRecordsWrapper {
        epoch_id: artifact.epoch_id,
        reward_root_hex: &artifact.reward_root_hex,
        reward_block_secs: artifact.reward_block_secs,
        total_network_weight_units: artifact.total_network_weight_units,
        records,
    };

    let json = serde_json::to_string_pretty(&wrapper).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    std::fs::write(output_path, &json)?;

    Ok(RewardRecordsOutput {
        json_path: output_path.to_string_lossy().to_string(),
        epoch_id: artifact.epoch_id,
        reward_root_hex: artifact.reward_root_hex.clone(),
        record_count: records.len(),
    })
}

pub fn aggregate_records_to_json(records: &[AggregateRecord], aggregate_root_hex: &str, output_path: &Path) -> Result<String, std::io::Error> {
    #[derive(Serialize)]
    struct AggregateRecordsWrapper<'a> {
        aggregate_root_hex: &'a str,
        records: &'a [AggregateRecord],
    }

    let wrapper = AggregateRecordsWrapper {
        aggregate_root_hex,
        records,
    };

    let json = serde_json::to_string_pretty(&wrapper).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    std::fs::write(output_path, &json)?;

    Ok(aggregate_root_hex.to_string())
}

pub fn replica_receipt_to_json(receipt: &ReplicaReceipt, output_path: &Path) -> Result<String, std::io::Error> {
    let json = serde_json::to_string_pretty(receipt).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    std::fs::write(output_path, &json)?;

    Ok(receipt.payload_cid.clone())
}

fn challenge_kind_name(kind: crate::primitives::ChallengeKind) -> &'static str {
    match kind {
        crate::primitives::ChallengeKind::BadBatch => "BadBatch",
        crate::primitives::ChallengeKind::Omission => "Omission",
        crate::primitives::ChallengeKind::BadAggregate => "BadAggregate",
        crate::primitives::ChallengeKind::BadReward => "BadReward",
        crate::primitives::ChallengeKind::BadStorage => "BadStorage",
    }
}

fn challenge_state_name(state: crate::primitives::ChallengeState) -> &'static str {
    match state {
        crate::primitives::ChallengeState::Open => "Open",
        crate::primitives::ChallengeState::Responded => "Responded",
        crate::primitives::ChallengeState::Succeeded => "Succeeded",
        crate::primitives::ChallengeState::Rejected => "Rejected",
        crate::primitives::ChallengeState::Expired => "Expired",
    }
}

pub fn challenge_to_json(challenge: &Challenge, output_path: &Path) -> Result<String, std::io::Error> {
    #[derive(Serialize)]
    struct ChallengeWrapper {
        challenge_id_hex: String,
        kind: String,
        epoch_id: EpochId,
        target_address: Option<String>,
        challenger: String,
        bond: Amount,
        opened_at_height: Height,
        deadline_height: Height,
        state: String,
        evidence: Option<ChallengeEvidenceRefJson>,
    }

    #[derive(Serialize)]
    struct ChallengeEvidenceRefJson {
        batch_root_hex: Option<String>,
        aggregate_root_hex: Option<String>,
        reward_root_hex: Option<String>,
        payload_cid: Option<String>,
        merkle_proof: Vec<String>,
    }

    let evidence_json = {
        let e = &challenge.evidence;
        Some(ChallengeEvidenceRefJson {
            batch_root_hex: e.batch_root.map(|h| hex_encode(&h)),
            aggregate_root_hex: e.aggregate_root.map(|h| hex_encode(&h)),
            reward_root_hex: e.reward_root.map(|h| hex_encode(&h)),
            payload_cid: e.payload_cid.clone(),
            merkle_proof: e.merkle_proof.iter().map(|h| hex_encode(h)).collect(),
        })
    };

    let wrapper = ChallengeWrapper {
        challenge_id_hex: hex_encode(&challenge.challenge_id),
        kind: challenge_kind_name(challenge.kind).to_string(),
        epoch_id: challenge.epoch_id,
        target_address: challenge.target_node.map(|n| hex_encode(&n)),
        challenger: hex_encode(&challenge.challenger),
        bond: challenge.bond,
        opened_at_height: challenge.opened_at_height,
        deadline_height: challenge.deadline_height,
        state: challenge_state_name(challenge.state).to_string(),
        evidence: evidence_json,
    };

    let json = serde_json::to_string_pretty(&wrapper).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    std::fs::write(output_path, &json)?;

    Ok(hex_encode(&challenge.challenge_id))
}

pub struct CosmosTxMessage {
    pub type_url: String,
    pub value: Vec<u8>,
}

impl CosmosTxMessage {
    pub fn submit_batch(collector: &str, batch_commit_json: &serde_json::Value) -> Result<Self, serde_json::Error> {
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

    pub fn commit_epoch(proposer: &str, epoch_commit_json: &serde_json::Value) -> Result<Self, serde_json::Error> {
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

    pub fn claim_reward(claimer: &str, epoch_id: u64, recipient: &str) -> Result<Self, serde_json::Error> {
        #[derive(Serialize)]
        struct MsgClaimReward {
            claimer: String,
            #[serde(rename = "epoch_id")]
            epoch_id: u64,
            recipient: String,
        }

        let msg = MsgClaimReward {
            claimer: claimer.to_string(),
            epoch_id,
            recipient: recipient.to_string(),
        };

        let value = serde_json::to_vec(&msg)?;

        Ok(Self {
            type_url: "/pole.chain.pole.v1.MsgClaimReward".to_string(),
            value,
        })
    }

    pub fn open_challenge(challenger: &str, challenge_json: &serde_json::Value) -> Result<Self, serde_json::Error> {
        #[derive(Serialize)]
        struct MsgOpenChallenge {
            challenger: String,
            challenge: serde_json::Value,
        }

        let msg = MsgOpenChallenge {
            challenger: challenger.to_string(),
            challenge: challenge_json.clone(),
        };

        let value = serde_json::to_vec(&msg)?;

        Ok(Self {
            type_url: "/pole.chain.pole.v1.MsgOpenChallenge".to_string(),
            value,
        })
    }

    pub fn submit_replica_receipt(storer: &str, receipt_json: &serde_json::Value) -> Result<Self, serde_json::Error> {
        #[derive(Serialize)]
        struct MsgSubmitReplicaReceipt {
            storer: String,
            #[serde(rename = "replica_receipt")]
            replica_receipt: serde_json::Value,
        }

        let msg = MsgSubmitReplicaReceipt {
            storer: storer.to_string(),
            replica_receipt: receipt_json.clone(),
        };

        let value = serde_json::to_vec(&msg)?;

        Ok(Self {
            type_url: "/pole.chain.pole.v1.MsgSubmitReplicaReceipt".to_string(),
            value,
        })
    }

    pub fn upsert_game_weight(authority: &str, game_weight_json: &serde_json::Value) -> Result<Self, serde_json::Error> {
        #[derive(Serialize)]
        struct MsgUpsertGameWeight {
            authority: String,
            entry: serde_json::Value,
        }

        let msg = MsgUpsertGameWeight {
            authority: authority.to_string(),
            entry: game_weight_json.clone(),
        };

        let value = serde_json::to_vec(&msg)?;

        Ok(Self {
            type_url: "/pole.chain.pole.v1.MsgUpsertGameWeight".to_string(),
            value,
        })
    }
}

pub fn build_batch_submit_tx(collector_hex: &str, batch_commit_json: &serde_json::Value) -> Result<CosmosTxMessage, serde_json::Error> {
    CosmosTxMessage::submit_batch(collector_hex, batch_commit_json)
}

pub fn build_epoch_commit_tx(proposer_hex: &str, epoch_commit_json: &serde_json::Value) -> Result<CosmosTxMessage, serde_json::Error> {
    CosmosTxMessage::commit_epoch(proposer_hex, epoch_commit_json)
}

pub fn build_claim_reward_tx(claimer_hex: &str, epoch_id: u64, recipient_hex: &str) -> Result<CosmosTxMessage, serde_json::Error> {
    CosmosTxMessage::claim_reward(claimer_hex, epoch_id, recipient_hex)
}

pub fn build_open_challenge_tx(challenger_hex: &str, challenge_json: &serde_json::Value) -> Result<CosmosTxMessage, serde_json::Error> {
    CosmosTxMessage::open_challenge(challenger_hex, challenge_json)
}

pub fn build_replica_receipt_tx(storer_hex: &str, receipt_json: &serde_json::Value) -> Result<CosmosTxMessage, serde_json::Error> {
    CosmosTxMessage::submit_replica_receipt(storer_hex, receipt_json)
}

pub fn build_game_weight_tx(authority_hex: &str, game_weight_json: &serde_json::Value) -> Result<CosmosTxMessage, serde_json::Error> {
    CosmosTxMessage::upsert_game_weight(authority_hex, game_weight_json)
}

pub fn parse_batch_commit_from_json(json_str: &str) -> Result<BatchCommit, serde_json::Error> {
    serde_json::from_str(json_str)
}

pub fn parse_epoch_commit_from_json(json_str: &str) -> Result<EpochCommit, serde_json::Error> {
    serde_json::from_str(json_str)
}

pub fn parse_reward_record_from_json(json_str: &str) -> Result<RewardRecord, serde_json::Error> {
    serde_json::from_str(json_str)
}

pub fn parse_aggregate_record_from_json(json_str: &str) -> Result<AggregateRecord, serde_json::Error> {
    serde_json::from_str(json_str)
}

pub fn parse_replica_receipt_from_json(json_str: &str) -> Result<ReplicaReceipt, serde_json::Error> {
    serde_json::from_str(json_str)
}

pub fn batch_commit_to_cosmos_json(batch: &BatchCommit) -> serde_json::Value {
    serde_json::json!({
        "epoch_id": batch.epoch_id,
        "collector_address": hex_encode(&batch.collector_id),
        "slot_start": batch.slot_start,
        "slot_end": batch.slot_end,
        "batch": {
            "root": hex_encode(&batch.batch.root),
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
            "root": hex_encode(&commit.accepted_batches.root),
            "leaf_count": commit.accepted_batches.leaf_count,
        },
        "observations": {
            "root": hex_encode(&commit.observations.root),
            "leaf_count": commit.observations.leaf_count,
        },
        "aggregates": {
            "root": hex_encode(&commit.aggregates.root),
            "leaf_count": commit.aggregates.leaf_count,
        },
        "rewards": {
            "root": hex_encode(&commit.rewards.root),
            "leaf_count": commit.rewards.leaf_count,
        },
        "availability": {
            "root": hex_encode(&commit.availability.root),
            "leaf_count": commit.availability.leaf_count,
        },
        "randomness_seed_hex": hex_encode(&commit.randomness_seed),
        "proposer_address": hex_encode(&commit.proposer_id),
        "challenge_open_height": commit.challenge_open_height,
        "challenge_deadline_height": commit.challenge_deadline_height,
    })
}

pub fn reward_record_to_cosmos_json(record: &RewardRecord) -> serde_json::Value {
    serde_json::json!({
        "epoch_id": record.epoch_id,
        "recipient": hex_encode(&record.node_id),
        "player_reward": record.player_reward,
        "collect_reward": record.collect_reward,
        "store_reward": record.store_reward,
        "verify_reward": record.verify_reward,
        "propose_reward": record.propose_reward,
        "slash_debit": record.slash_debit,
        "net_reward": record.net_reward,
    })
}

pub fn aggregate_record_to_cosmos_json(record: &AggregateRecord) -> serde_json::Value {
    serde_json::json!({
        "epoch_id": record.epoch_id,
        "app_id": record.app_id,
        "total_weight_units": record.gvs_microunits,
        "player_count": record.median_players,
    })
}

pub fn replica_receipt_to_cosmos_json(receipt: &ReplicaReceipt) -> serde_json::Value {
    serde_json::json!({
        "epoch_id": receipt.epoch_id,
        "payload_cid": receipt.payload_cid,
        "storer_address": hex_encode(&receipt.storer_id),
        "retention_until_epoch": receipt.retention_until_epoch,
        "receipt_signature": hex_encode(&receipt.receipt_signature),
        "receipt_hash_hex": hex_encode(&receipt.epoch_id.to_le_bytes()),
    })
}

fn hex_encode<T: AsRef<[u8]>>(bytes: &T) -> String {
    bytes.as_ref().iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn generate_tx_json_for_batch(collector_hex: &str, batch: &BatchCommit) -> Result<String, serde_json::Error> {
    let cosmos_json = batch_commit_to_cosmos_json(batch);
    let tx = build_batch_submit_tx(collector_hex, &cosmos_json)?;
    let wrapper = serde_json::json!({
        "type_url": tx.type_url,
        "value": base64_encode(&tx.value),
    });
    serde_json::to_string_pretty(&wrapper)
}

pub fn generate_tx_json_for_epoch_commit(proposer_hex: &str, commit: &EpochCommit) -> Result<String, serde_json::Error> {
    let cosmos_json = epoch_commit_to_cosmos_json(commit);
    let tx = build_epoch_commit_tx(proposer_hex, &cosmos_json)?;
    let wrapper = serde_json::json!({
        "type_url": tx.type_url,
        "value": base64_encode(&tx.value),
    });
    serde_json::to_string_pretty(&wrapper)
}

pub fn generate_tx_json_for_claim_reward(claimer_hex: &str, epoch_id: u64, recipient_hex: &str) -> Result<String, serde_json::Error> {
    let tx = build_claim_reward_tx(claimer_hex, epoch_id, recipient_hex)?;
    let wrapper = serde_json::json!({
        "type_url": tx.type_url,
        "value": base64_encode(&tx.value),
    });
    serde_json::to_string_pretty(&wrapper)
}

pub fn export_batch_for_cosmos_submit(batch: &AssembledBatch, output_dir: &Path) -> Result<BatchCommitOutput, std::io::Error> {
    let rust_batch = &batch.batch_commit;

    let tx_json = generate_tx_json_for_batch(
        &hex_encode(&rust_batch.collector_id),
        rust_batch,
    ).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let file_name = format!("batch-{:06}-submit-tx.json", rust_batch.epoch_id);
    let output_path = output_dir.join(&file_name);

    std::fs::write(&output_path, &tx_json)?;

    Ok(BatchCommitOutput {
        json_path: output_path.to_string_lossy().to_string(),
        binary_path: String::new(),
        payload_cid: rust_batch.payload_cid.clone(),
        batch_root_hex: hex_encode(&rust_batch.batch.root),
        obs_count: rust_batch.obs_count,
    })
}

pub fn export_epoch_commit_for_cosmos(commit: &EpochCommit, output_dir: &Path) -> Result<EpochCommitOutput, std::io::Error> {
    let tx_json = generate_tx_json_for_epoch_commit(
        &hex_encode(&commit.proposer_id),
        commit,
    ).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let file_name = format!("epoch-{:06}-commit-tx.json", commit.epoch_id);
    let output_path = output_dir.join(&file_name);

    std::fs::write(&output_path, &tx_json)?;

    Ok(EpochCommitOutput {
        json_path: output_path.to_string_lossy().to_string(),
        epoch_id: commit.epoch_id,
        accepted_batches_root_hex: hex_encode(&commit.accepted_batches.root),
        observations_root_hex: hex_encode(&commit.observations.root),
        aggregates_root_hex: hex_encode(&commit.aggregates.root),
        rewards_root_hex: hex_encode(&commit.rewards.root),
        availability_root_hex: hex_encode(&commit.availability.root),
        challenge_deadline_height: commit.challenge_deadline_height,
    })
}

pub fn export_replica_receipt_for_cosmos(receipt: &ReplicaReceipt, output_dir: &Path) -> Result<String, std::io::Error> {
    let cosmos_json = replica_receipt_to_cosmos_json(receipt);

    let file_name = format!("receipt-{:06}-{}-tx.json", receipt.epoch_id, receipt.payload_cid);
    let output_path = output_dir.join(&file_name);

    std::fs::write(&output_path, serde_json::to_string_pretty(&cosmos_json).unwrap())?;

    Ok(output_path.to_string_lossy().to_string())
}

pub fn batch_commit_from_assembled_batch(batch: &AssembledBatch) -> BatchCommit {
    batch.batch_commit.clone()
}

pub fn export_reward_records_for_cosmos(records: &[RewardRecord], output_dir: &Path, epoch_id: EpochId) -> Result<RewardRecordsOutput, std::io::Error> {
    let file_name = format!("reward-records-{:06}.json", epoch_id);
    let output_path = output_dir.join(&file_name);

    let cosmos_records: Vec<serde_json::Value> = records.iter()
        .map(reward_record_to_cosmos_json)
        .collect();

    let wrapper = serde_json::json!({
        "epoch_id": epoch_id,
        "records": cosmos_records,
    });

    std::fs::write(&output_path, serde_json::to_string_pretty(&wrapper).unwrap())?;

    Ok(RewardRecordsOutput {
        json_path: output_path.to_string_lossy().to_string(),
        epoch_id,
        reward_root_hex: String::new(),
        record_count: records.len(),
    })
}

pub fn export_aggregate_records_for_cosmos(records: &[AggregateRecord], output_dir: &Path, epoch_id: EpochId) -> Result<String, std::io::Error> {
    let file_name = format!("aggregate-records-{:06}.json", epoch_id);
    let output_path = output_dir.join(&file_name);

    let cosmos_records: Vec<serde_json::Value> = records.iter()
        .map(aggregate_record_to_cosmos_json)
        .collect();

    let wrapper = serde_json::json!({
        "epoch_id": epoch_id,
        "records": cosmos_records,
    });

    std::fs::write(&output_path, serde_json::to_string_pretty(&wrapper).unwrap())?;

    Ok(output_path.to_string_lossy().to_string())
}

fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;
        result.push(ALPHABET[b0 >> 2] as char);
        result.push(ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)] as char);
        if chunk.len() > 1 {
            result.push(ALPHABET[((b1 & 0x0F) << 2) | (b2 >> 6)] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(ALPHABET[b2 & 0x3F] as char);
        } else {
            result.push('=');
        }
    }
    result
}

pub fn create_cosmos_signed_tx_json(
    tx_json: &str,
    _chain_id: &str,
    _gas_limit: u64,
    _fee_amount: &str,
) -> serde_json::Value {
    serde_json::json!({
        "tx": serde_json::from_str::<serde_json::Value>(tx_json).unwrap_or(serde_json::Value::Null),
        "mode": "block",
        "signing_options": {
            "signature": base64_encode(b"placeholder_signature_for_development"),
            "public_key": base64_encode(b"placeholder_pubkey_for_development"),
        }
    })
}

pub fn generate_submit_batch_tx(collector_id: &NodeId, batch: &BatchCommit, output_dir: &Path) -> Result<BatchCommitOutput, std::io::Error> {
    export_batch_for_cosmos_submit(&AssembledBatch {
        batch_commit: batch.clone(),
        payload_hash: [0u8; 32],
        payload_cid: batch.payload_cid.clone(),
        payload_bytes: Vec::new(),
        observations: Vec::new(),
    }, output_dir)
}

pub fn generate_commit_epoch_tx(proposer_id: &NodeId, commit: &EpochCommit, output_dir: &Path) -> Result<EpochCommitOutput, std::io::Error> {
    export_epoch_commit_for_cosmos(commit, output_dir)
}

pub fn generate_claim_reward_tx(claimer_id: &NodeId, epoch_id: EpochId, recipient_id: &NodeId, output_dir: &Path) -> Result<String, std::io::Error> {
    let tx_json = generate_tx_json_for_claim_reward(
        &hex_encode(claimer_id),
        epoch_id,
        &hex_encode(recipient_id),
    ).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let file_name = format!("claim-reward-{:06}-{}.json", epoch_id, hex_encode(recipient_id));
    let output_path = output_dir.join(&file_name);

    std::fs::write(&output_path, &tx_json)?;

    Ok(output_path.to_string_lossy().to_string())
}

pub fn generate_open_challenge_tx(challenger_id: &NodeId, challenge: &Challenge, output_dir: &Path) -> Result<String, std::io::Error> {
    let challenge_json = serde_json::to_value(challenge).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let tx = build_open_challenge_tx(&hex_encode(challenger_id), &challenge_json)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let wrapper = serde_json::json!({
        "type_url": tx.type_url,
        "value": base64_encode(&tx.value),
    });

    let file_name = format!("open-challenge-{}.json", hex_encode(&challenge.challenge_id));
    let output_path = output_dir.join(&file_name);

    std::fs::write(&output_path, serde_json::to_string_pretty(&wrapper).unwrap())?;

    Ok(output_path.to_string_lossy().to_string())
}