use std::collections::BTreeSet;
use std::env;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::activity_collector::collect_configured_activity_source;
use crate::json_file::{load_json, load_json_or_default, save_pretty_json};
use crate::node_aggregator::{
    aggregate_local_epoch, aggregate_record_root, EpochAggregationArtifact, NodeAggregationError,
};
use crate::node_config::{NodeConfig, NodeConfigError};
use crate::node_pipeline::AssembledBatch;
use crate::node_rewards::{
    adjusted_player_block_reward, effective_challenge_window_blocks,
    effective_min_retention_epochs, effective_player_block_reward,
    effective_reward_adjustment_cap_bps, effective_reward_block_secs,
    effective_target_network_weight_units, record_player_reward_tick, reward_local_epoch,
    EpochRewardArtifact, NodeRewardError, PendingPlayerRewardBlockState, PlayerRewardTickArtifact,
    RewardTickContext,
};
use crate::node_runtime::{CollectAndStoreOutcome, LocalNodeRuntime, NodeRuntimeError};
use crate::node_settlement::{
    settle_local_epoch, suggested_settlement_height, EpochSettlementArtifact, NodeSettlementError,
};
use crate::node_storage_audit::{
    audit_local_retention, run_local_storage_challenge, NodeStorageAuditError,
    RetentionAuditArtifact, StorageChallengeArtifact,
};
use crate::node_verifier::{verify_local_epoch, EpochVerificationReport, NodeVerificationError};
use crate::p2p::{P2pMessage, P2pNetwork, P2pTopic};
use crate::primitives::Hash32;
use crate::records::{Challenge, ChallengeEvidenceRef, EpochCommit, ObservationRecord};
use crate::steam_collector::{fetch_current_players_live, HttpTextClient, SteamCollectorError};
use crate::storage_book::{LocalRetentionBook, StorageBookError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalNodeProgress {
    pub next_epoch_id: u64,
    pub next_slot_id: u64,
    pub ticks_completed: u64,
    #[serde(default)]
    pub reward_blocks_completed: u64,
    #[serde(default)]
    pub current_reward_adjustment_period_index: u64,
    #[serde(default)]
    pub current_adjustment_cycle_index: u64,
    #[serde(default)]
    pub current_fixed_block_reward: u128,
    #[serde(default)]
    pub current_fixed_player_reward: u128,
    #[serde(default)]
    pub current_fixed_block_reward_basis_period_index: u64,
    #[serde(default)]
    pub current_fixed_player_reward_basis_cycle_index: u64,
    #[serde(default)]
    pub current_fixed_block_reward_basis_network_weight_units: u128,
    #[serde(default)]
    pub current_fixed_player_reward_basis_total_network_weight_units: u128,
    #[serde(default)]
    pub previous_reward_adjustment_period_network_weight_units: u128,
    #[serde(default)]
    pub previous_adjustment_cycle_total_network_weight_units: u128,
    #[serde(default)]
    pub current_reward_adjustment_period_network_weight_units: u128,
    #[serde(default)]
    pub current_adjustment_cycle_total_network_weight_units: u128,
    #[serde(default)]
    pub pending_reward_block: Option<PendingPlayerRewardBlockState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectTickArtifact {
    pub epoch_id: u64,
    pub slot_id: u64,
    pub payload_cid: String,
    pub payload_hash_hex: String,
    pub batch_root_hex: String,
    pub obs_count: u32,
    #[serde(default)]
    pub player_reward_block_count: usize,
    #[serde(default)]
    pub player_reward_total: u128,
    #[serde(default)]
    pub reward_process_name: Option<String>,
    pub stored_payload_cid: Option<String>,
    pub retention_until_epoch: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpochCommitArtifact {
    pub epoch_id: u64,
    pub current_height: u64,
    pub challenge_deadline_height: u64,
    pub batch_count: u32,
    pub payload_count: u32,
    pub accepted_batches_root_hex: String,
    pub observations_root_hex: String,
    pub availability_root_hex: String,
    pub aggregates_root_hex: String,
    pub rewards_root_hex: String,
    pub randomness_seed_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewardAdjustmentArtifact {
    #[serde(alias = "period_index")]
    pub adjustment_cycle_index: u64,
    #[serde(alias = "basis_period_index")]
    pub basis_cycle_index: u64,
    #[serde(alias = "basis_network_weight_units")]
    pub basis_total_network_weight_units: u128,
    #[serde(alias = "target_network_weight_units")]
    pub target_total_network_weight_units: u128,
    pub reward_adjustment_cap_bps: u16,
    #[serde(alias = "reward_adjustment_period_blocks")]
    pub adjustment_cycle_blocks: u64,
    pub reward_block_secs: u64,
    #[serde(alias = "base_player_block_reward")]
    pub base_fixed_player_reward: u128,
    #[serde(alias = "adjusted_player_block_reward")]
    pub fixed_player_reward: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewardAdjustmentIndexEntry {
    #[serde(alias = "period_index")]
    pub adjustment_cycle_index: u64,
    #[serde(alias = "basis_period_index")]
    pub basis_cycle_index: u64,
    #[serde(alias = "adjusted_player_block_reward")]
    pub fixed_player_reward: u128,
    pub artifact_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RewardAdjustmentArtifactIndex {
    pub adjustment_artifacts: Vec<RewardAdjustmentIndexEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RewardAdjustmentArtifactSummary {
    pub adjustment_artifact_count: usize,
    pub adjustment_cycle_artifact_count: usize,
    pub latest_period_index: Option<u64>,
    pub latest_adjustment_cycle_index: Option<u64>,
    pub latest_basis_period_index: Option<u64>,
    pub latest_basis_cycle_index: Option<u64>,
    pub latest_adjusted_player_block_reward: Option<u128>,
    pub latest_fixed_player_reward: Option<u128>,
    pub artifact_index_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeStatusSummary {
    pub next_epoch_id: u64,
    pub next_slot_id: u64,
    pub ticks_completed: u64,
    pub target_app_ids: Vec<u32>,
    pub low_impact_mode: bool,
    pub os_background_priority: bool,
    pub inline_verify_enabled: bool,
    pub inline_propose_enabled: bool,
    pub reward_block_secs: u64,
    pub reward_source: String,
    pub emission_year: u32,
    pub challenge_window_blocks: u32,
    pub reward_adjustment_period_blocks: u64,
    pub adjustment_cycle_blocks: u64,
    pub configured_player_block_reward: u128,
    pub effective_player_block_reward: u128,
    pub configured_fixed_player_reward: u128,
    pub effective_fixed_player_reward: u128,
    pub effective_min_retention_epochs: u32,
    pub effective_app_weight_override_count: usize,
    pub target_network_weight_units: u128,
    pub reward_adjustment_cap_bps: u16,
    pub reward_blocks_completed: u64,
    pub current_reward_adjustment_period_index: u64,
    pub current_adjustment_cycle_index: u64,
    pub current_fixed_block_reward: u128,
    pub current_fixed_player_reward: u128,
    pub current_fixed_block_reward_basis_period_index: u64,
    pub current_fixed_player_reward_basis_cycle_index: u64,
    pub current_fixed_block_reward_basis_network_weight_units: u128,
    pub current_fixed_player_reward_basis_total_network_weight_units: u128,
    pub current_reward_adjustment_artifact_path: String,
    pub current_reward_adjustment_artifact_exists: bool,
    pub reward_adjustment_artifact_count: usize,
    pub adjustment_cycle_artifact_count: usize,
    pub reward_adjustment_artifact_index_path: String,
    pub reward_adjustment_artifact_summary_path: String,
    pub previous_reward_adjustment_period_network_weight_units: u128,
    pub previous_adjustment_cycle_total_network_weight_units: u128,
    pub current_reward_adjustment_period_network_weight_units: u128,
    pub current_adjustment_cycle_total_network_weight_units: u128,
    pub pending_reward_block_seconds: u64,
    pub pending_reward_block_ticks: u64,
    pub pending_reward_block_remaining_seconds: u64,
    pub pending_reward_block_entry_count: usize,
    pub stored_payload_count: usize,
    pub used_bytes: u64,
    pub quota_bytes: u64,
    pub configured_p2p_batch_listener_count: usize,
    pub configured_p2p_receipt_listener_count: usize,
    pub configured_p2p_dual_listener_count: usize,
    pub last_tick_at_millis: Option<u64>,
    pub last_payload_cid: Option<String>,
    pub last_aggregate_epoch_id: Option<u64>,
    pub last_aggregate_gvs_tier: Option<String>,
    pub last_aggregate_source_kind: Option<String>,
    pub last_aggregate_source_confidence_ppm: Option<u32>,
    pub last_p2p_batch_recipients: Option<usize>,
    pub last_p2p_receipt_recipients: Option<usize>,
    pub last_p2p_retrieval_ok: Option<bool>,
    pub last_p2p_retrieval_error: Option<String>,
    pub last_p2p_transport: Option<String>,
    pub last_p2p_known_peer_count: Option<usize>,
    pub last_p2p_learned_remote_peer_count: Option<usize>,
    pub last_p2p_batch_listener_count: Option<usize>,
    pub last_p2p_receipt_listener_count: Option<usize>,
    pub last_p2p_challenge_listener_count: Option<usize>,
    pub last_p2p_coordination_sent_count: Option<u64>,
    pub last_p2p_coordination_received_count: Option<u64>,
    pub last_p2p_hello_sent_count: Option<u64>,
    pub last_p2p_hint_sent_count: Option<u64>,
    pub last_p2p_goodbye_sent_count: Option<u64>,
    pub last_p2p_hello_received_count: Option<u64>,
    pub last_p2p_hint_received_count: Option<u64>,
    pub last_p2p_goodbye_received_count: Option<u64>,
    pub last_p2p_challenge_recipients: Option<usize>,
    pub last_p2p_challenge_kind: Option<String>,
    pub last_p2p_challenge_epoch_id: Option<u64>,
    pub last_p2p_challenge_payload_cid: Option<String>,
    pub p2p_challenge_events_total: Option<u64>,
    pub p2p_bad_batch_challenge_events: Option<u64>,
    pub p2p_omission_challenge_events: Option<u64>,
    pub p2p_bad_aggregate_challenge_events: Option<u64>,
    pub p2p_bad_reward_challenge_events: Option<u64>,
    pub p2p_bad_storage_challenge_events: Option<u64>,
    pub recent_p2p_challenge_events: Option<usize>,
    pub recent_p2p_bad_batch_challenge_events: Option<usize>,
    pub recent_p2p_omission_challenge_events: Option<usize>,
    pub recent_p2p_bad_aggregate_challenge_events: Option<usize>,
    pub recent_p2p_bad_reward_challenge_events: Option<usize>,
    pub recent_p2p_bad_storage_challenge_events: Option<usize>,
    pub p2p_challenge_delivered_events_total: Option<u64>,
    pub p2p_challenge_zero_recipient_events_total: Option<u64>,
    pub recent_p2p_challenge_delivered_events: Option<usize>,
    pub recent_p2p_challenge_zero_recipient_events: Option<usize>,
    pub recent_p2p_challenge_recipient_sum: Option<usize>,
    pub last_retention_all_retrievable: Option<bool>,
    pub last_retention_retained_payload_count: Option<usize>,
    pub last_retention_retrievable_payload_count: Option<usize>,
    pub last_retention_missing_payload_count: Option<usize>,
    pub last_retention_corrupted_payload_count: Option<usize>,
    pub last_storage_challenge_all_passed: Option<bool>,
    pub last_storage_challenge_checked_payload_count: Option<usize>,
    pub last_storage_challenge_failed_payload_count: Option<usize>,
    pub last_storage_challenge_error: Option<String>,
    pub last_auto_settlement_pending_epoch_count: Option<usize>,
    pub last_auto_settled_epoch: Option<u64>,
    pub last_auto_settlement_reward_claimed: Option<bool>,
    pub last_auto_settlement_error: Option<String>,
}

impl NodeStatusSummary {
    fn normalize_adjustment_cycle_fields(&mut self) {
        if self.adjustment_cycle_blocks == 0 {
            self.adjustment_cycle_blocks = self.reward_adjustment_period_blocks;
        }
        if self.configured_fixed_player_reward == 0 {
            self.configured_fixed_player_reward = self.configured_player_block_reward;
        }
        if self.effective_fixed_player_reward == 0 {
            self.effective_fixed_player_reward = self.effective_player_block_reward;
        }
        if self.current_adjustment_cycle_index == 0 {
            self.current_adjustment_cycle_index = self.current_reward_adjustment_period_index;
        }
        if self.current_fixed_player_reward == 0 {
            self.current_fixed_player_reward = self.current_fixed_block_reward;
        }
        if self.current_fixed_player_reward_basis_cycle_index == 0 {
            self.current_fixed_player_reward_basis_cycle_index =
                self.current_fixed_block_reward_basis_period_index;
        }
        if self.current_fixed_player_reward_basis_total_network_weight_units == 0 {
            self.current_fixed_player_reward_basis_total_network_weight_units =
                self.current_fixed_block_reward_basis_network_weight_units;
        }
        if self.adjustment_cycle_artifact_count == 0 {
            self.adjustment_cycle_artifact_count = self.reward_adjustment_artifact_count;
        }
        if self.previous_adjustment_cycle_total_network_weight_units == 0 {
            self.previous_adjustment_cycle_total_network_weight_units =
                self.previous_reward_adjustment_period_network_weight_units;
        }
        if self.current_adjustment_cycle_total_network_weight_units == 0 {
            self.current_adjustment_cycle_total_network_weight_units =
                self.current_reward_adjustment_period_network_weight_units;
        }

        self.reward_adjustment_period_blocks = self.adjustment_cycle_blocks;
        self.configured_player_block_reward = self.configured_fixed_player_reward;
        self.effective_player_block_reward = self.effective_fixed_player_reward;
        self.current_reward_adjustment_period_index = self.current_adjustment_cycle_index;
        self.current_fixed_block_reward = self.current_fixed_player_reward;
        self.current_fixed_block_reward_basis_period_index =
            self.current_fixed_player_reward_basis_cycle_index;
        self.current_fixed_block_reward_basis_network_weight_units =
            self.current_fixed_player_reward_basis_total_network_weight_units;
        self.reward_adjustment_artifact_count = self.adjustment_cycle_artifact_count;
        self.previous_reward_adjustment_period_network_weight_units =
            self.previous_adjustment_cycle_total_network_weight_units;
        self.current_reward_adjustment_period_network_weight_units =
            self.current_adjustment_cycle_total_network_weight_units;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeHeartbeat {
    pub tick_epoch_id: u64,
    pub tick_slot_id: u64,
    pub ticks_completed: u64,
    pub last_payload_cid: String,
    pub updated_at_millis: u64,
    #[serde(default)]
    pub last_p2p_batch_recipients: Option<usize>,
    #[serde(default)]
    pub last_p2p_receipt_recipients: Option<usize>,
    #[serde(default)]
    pub last_p2p_retrieval_ok: Option<bool>,
    #[serde(default)]
    pub last_p2p_retrieval_error: Option<String>,
    #[serde(default)]
    pub last_p2p_transport: Option<String>,
    #[serde(default)]
    pub last_p2p_known_peer_count: Option<usize>,
    #[serde(default)]
    pub last_p2p_learned_remote_peer_count: Option<usize>,
    #[serde(default)]
    pub last_p2p_batch_listener_count: Option<usize>,
    #[serde(default)]
    pub last_p2p_receipt_listener_count: Option<usize>,
    #[serde(default)]
    pub last_p2p_challenge_listener_count: Option<usize>,
    #[serde(default)]
    pub last_p2p_coordination_sent_count: Option<u64>,
    #[serde(default)]
    pub last_p2p_coordination_received_count: Option<u64>,
    #[serde(default)]
    pub last_p2p_hello_sent_count: Option<u64>,
    #[serde(default)]
    pub last_p2p_hint_sent_count: Option<u64>,
    #[serde(default)]
    pub last_p2p_goodbye_sent_count: Option<u64>,
    #[serde(default)]
    pub last_p2p_hello_received_count: Option<u64>,
    #[serde(default)]
    pub last_p2p_hint_received_count: Option<u64>,
    #[serde(default)]
    pub last_p2p_goodbye_received_count: Option<u64>,
    #[serde(default)]
    pub last_p2p_challenge_recipients: Option<usize>,
    #[serde(default)]
    pub last_p2p_challenge_kind: Option<String>,
    #[serde(default)]
    pub last_p2p_challenge_epoch_id: Option<u64>,
    #[serde(default)]
    pub last_p2p_challenge_payload_cid: Option<String>,
    #[serde(default)]
    pub p2p_challenge_events_total: Option<u64>,
    #[serde(default)]
    pub p2p_bad_batch_challenge_events: Option<u64>,
    #[serde(default)]
    pub p2p_omission_challenge_events: Option<u64>,
    #[serde(default)]
    pub p2p_bad_aggregate_challenge_events: Option<u64>,
    #[serde(default)]
    pub p2p_bad_reward_challenge_events: Option<u64>,
    #[serde(default)]
    pub p2p_bad_storage_challenge_events: Option<u64>,
    #[serde(default)]
    pub p2p_challenge_delivered_events_total: Option<u64>,
    #[serde(default)]
    pub p2p_challenge_zero_recipient_events_total: Option<u64>,
    #[serde(default)]
    pub recent_p2p_challenge_history: Vec<P2pChallengeHistoryEntry>,
    #[serde(default)]
    pub last_retention_all_retrievable: Option<bool>,
    #[serde(default)]
    pub last_retention_retained_payload_count: Option<usize>,
    #[serde(default)]
    pub last_retention_retrievable_payload_count: Option<usize>,
    #[serde(default)]
    pub last_retention_missing_payload_count: Option<usize>,
    #[serde(default)]
    pub last_retention_corrupted_payload_count: Option<usize>,
    #[serde(default)]
    pub last_storage_challenge_all_passed: Option<bool>,
    #[serde(default)]
    pub last_storage_challenge_checked_payload_count: Option<usize>,
    #[serde(default)]
    pub last_storage_challenge_failed_payload_count: Option<usize>,
    #[serde(default)]
    pub last_storage_challenge_error: Option<String>,
    #[serde(default)]
    pub last_auto_settlement_pending_epoch_count: Option<usize>,
    #[serde(default)]
    pub last_auto_settled_epoch: Option<u64>,
    #[serde(default)]
    pub last_auto_settlement_reward_claimed: Option<bool>,
    #[serde(default)]
    pub last_auto_settlement_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PruneOutcome {
    pub current_epoch: u64,
    pub removed_payloads: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectTickResult {
    pub progress: LocalNodeProgress,
    pub artifact: CollectTickArtifact,
    pub outcome: CollectAndStoreOutcome,
    pub player_reward_tick_artifact: PlayerRewardTickArtifact,
    pub aggregation_artifact: Option<EpochAggregationArtifact>,
    pub reward_artifact: Option<EpochRewardArtifact>,
    pub verification_report: Option<EpochVerificationReport>,
    pub epoch_commit_artifact: Option<EpochCommitArtifact>,
    pub settlement_artifacts: Vec<EpochSettlementArtifact>,
    pub auto_settlement_pending_epochs: Vec<u64>,
    pub auto_settlement_enabled: bool,
    pub auto_settlement_error: Option<String>,
    pub p2p_challenge_recipients: Option<usize>,
    pub p2p_challenge_kind: Option<String>,
    pub p2p_challenge_epoch_id: Option<u64>,
    pub p2p_challenge_payload_cid: Option<String>,
    pub retention_audit_artifact: RetentionAuditArtifact,
    pub storage_challenge_artifact: Option<StorageChallengeArtifact>,
    pub storage_challenge_error: Option<String>,
    pub retention_prune_outcome: PruneOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoSettlementSummary {
    pub enabled: bool,
    pub pending_epochs: Vec<u64>,
    pub settled_epoch_count: usize,
    pub last_settlement_artifact: Option<EpochSettlementArtifact>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectLoopSummary {
    pub ticks_completed: u64,
    pub last_result: Option<CollectTickResult>,
    pub auto_settlement_summary: AutoSettlementSummary,
    pub total_batch_recipients: usize,
    pub total_receipt_recipients: usize,
    pub total_pruned_payloads: usize,
    pub last_retention_audit_artifact: Option<RetentionAuditArtifact>,
    pub last_storage_challenge_artifact: Option<StorageChallengeArtifact>,
    pub last_storage_challenge_error: Option<String>,
    pub last_prune_epoch: u64,
}

impl AutoSettlementSummary {
    pub fn skipped(&self) -> bool {
        !self.enabled && !self.pending_epochs.is_empty()
    }
}

impl CollectLoopSummary {
    fn new(config: &NodeConfig) -> Self {
        Self {
            ticks_completed: 0,
            last_result: None,
            auto_settlement_summary: AutoSettlementSummary {
                enabled: auto_settlement_enabled(config),
                pending_epochs: Vec::new(),
                settled_epoch_count: 0,
                last_settlement_artifact: None,
                last_error: None,
            },
            total_batch_recipients: 0,
            total_receipt_recipients: 0,
            total_pruned_payloads: 0,
            last_retention_audit_artifact: None,
            last_storage_challenge_artifact: None,
            last_storage_challenge_error: None,
            last_prune_epoch: 0,
        }
    }

    fn record_result(&mut self, result: &CollectTickResult) {
        self.ticks_completed += 1;
        self.total_batch_recipients += result.outcome.batch_recipients;
        self.total_receipt_recipients += result.outcome.receipt_recipients;
        self.total_pruned_payloads += result.pruned_payload_count();
        self.last_prune_epoch = result.retention_prune_outcome.current_epoch;
        self.last_retention_audit_artifact = Some(result.retention_audit_artifact.clone());
        if let Some(artifact) = &result.storage_challenge_artifact {
            self.last_storage_challenge_artifact = Some(artifact.clone());
        }
        if let Some(error) = &result.storage_challenge_error {
            self.last_storage_challenge_error = Some(error.clone());
        }
        update_auto_settlement_summary(&mut self.auto_settlement_summary, result);
        self.last_result = Some(result.clone());
    }
}

impl CollectTickResult {
    pub fn auto_settlement_skipped(&self) -> bool {
        !self.auto_settlement_enabled && !self.unresolved_auto_settlement_epochs().is_empty()
    }

    pub fn last_settlement_artifact(&self) -> Option<&EpochSettlementArtifact> {
        self.settlement_artifacts.last()
    }

    pub fn unresolved_auto_settlement_epochs(&self) -> Vec<u64> {
        let settled_epoch_ids = self
            .settlement_artifacts
            .iter()
            .map(|artifact| artifact.epoch_id)
            .collect::<BTreeSet<_>>();
        self.auto_settlement_pending_epochs
            .iter()
            .copied()
            .filter(|epoch_id| !settled_epoch_ids.contains(epoch_id))
            .collect()
    }

    pub fn pruned_payload_count(&self) -> usize {
        self.retention_prune_outcome.removed_payloads.len()
    }

    pub fn retention_integrity_healthy(&self) -> bool {
        self.retention_audit_artifact.all_retrievable
    }

    pub fn storage_challenge_healthy(&self) -> bool {
        self.storage_challenge_artifact
            .as_ref()
            .map(|artifact| artifact.all_passed)
            .unwrap_or(false)
            && self.storage_challenge_error.is_none()
    }
}

pub fn summarize_auto_settlement(
    config: &NodeConfig,
    results: &[CollectTickResult],
) -> AutoSettlementSummary {
    let mut summary = AutoSettlementSummary {
        enabled: auto_settlement_enabled(config),
        pending_epochs: Vec::new(),
        settled_epoch_count: 0,
        last_settlement_artifact: None,
        last_error: None,
    };

    for result in results {
        update_auto_settlement_summary(&mut summary, result);
    }

    summary
}

fn update_auto_settlement_summary(summary: &mut AutoSettlementSummary, result: &CollectTickResult) {
    let mut pending_epoch_ids = summary
        .pending_epochs
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    pending_epoch_ids.extend(result.auto_settlement_pending_epochs.iter().copied());
    summary.settled_epoch_count += result.settlement_artifacts.len();
    if let Some(error) = &result.auto_settlement_error {
        summary.last_error = Some(error.clone());
    }
    for artifact in &result.settlement_artifacts {
        pending_epoch_ids.remove(&artifact.epoch_id);
        summary.last_settlement_artifact = Some(artifact.clone());
        summary.last_error = None;
    }
    summary.pending_epochs = pending_epoch_ids.into_iter().collect();
}

#[derive(Debug)]
pub enum NodeDaemonError {
    Config(NodeConfigError),
    Runtime(NodeRuntimeError),
    Storage(StorageBookError),
    Aggregation(NodeAggregationError),
    Reward(NodeRewardError),
    Verification(NodeVerificationError),
    Settlement(String),
    StorageAudit(NodeStorageAuditError),
    Steam(SteamCollectorError),
    Io(io::Error),
    Json(serde_json::Error),
    NoTargetApps,
    InvalidHexLength { expected: usize, actual: usize },
    InvalidHexCharacter { index: usize, byte: u8 },
}

impl fmt::Display for NodeDaemonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(err) => write!(f, "config error: {err}"),
            Self::Runtime(err) => write!(f, "runtime error: {err}"),
            Self::Storage(err) => write!(f, "storage error: {err}"),
            Self::Aggregation(err) => write!(f, "aggregation error: {err}"),
            Self::Reward(err) => write!(f, "reward error: {err}"),
            Self::Verification(err) => write!(f, "verification error: {err}"),
            Self::Settlement(err) => write!(f, "settlement error: {err}"),
            Self::StorageAudit(err) => write!(f, "storage audit error: {err}"),
            Self::Steam(err) => write!(f, "steam error: {err}"),
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::Json(err) => write!(f, "json error: {err}"),
            Self::NoTargetApps => write!(f, "no target app ids configured"),
            Self::InvalidHexLength { expected, actual } => {
                write!(f, "invalid hex length: expected {expected}, got {actual}")
            }
            Self::InvalidHexCharacter { index, byte } => {
                write!(f, "invalid hex character at {index}: 0x{byte:02x}")
            }
        }
    }
}

impl std::error::Error for NodeDaemonError {}

impl From<NodeConfigError> for NodeDaemonError {
    fn from(value: NodeConfigError) -> Self {
        Self::Config(value)
    }
}

impl From<NodeRuntimeError> for NodeDaemonError {
    fn from(value: NodeRuntimeError) -> Self {
        Self::Runtime(value)
    }
}

impl From<StorageBookError> for NodeDaemonError {
    fn from(value: StorageBookError) -> Self {
        Self::Storage(value)
    }
}

impl From<NodeAggregationError> for NodeDaemonError {
    fn from(value: NodeAggregationError) -> Self {
        Self::Aggregation(value)
    }
}

impl From<NodeRewardError> for NodeDaemonError {
    fn from(value: NodeRewardError) -> Self {
        Self::Reward(value)
    }
}

impl From<SteamCollectorError> for NodeDaemonError {
    fn from(value: SteamCollectorError) -> Self {
        Self::Steam(value)
    }
}

impl From<NodeVerificationError> for NodeDaemonError {
    fn from(value: NodeVerificationError) -> Self {
        Self::Verification(value)
    }
}

impl From<NodeSettlementError> for NodeDaemonError {
    fn from(value: NodeSettlementError) -> Self {
        Self::Settlement(value.to_string())
    }
}

impl From<NodeStorageAuditError> for NodeDaemonError {
    fn from(value: NodeStorageAuditError) -> Self {
        Self::StorageAudit(value)
    }
}

impl From<io::Error> for NodeDaemonError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for NodeDaemonError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl LocalNodeProgress {
    pub fn default_from_config(config: &NodeConfig) -> Self {
        let mut progress = Self {
            next_epoch_id: config.collect.default_epoch_id,
            next_slot_id: config.collect.default_slot_id,
            ticks_completed: 0,
            reward_blocks_completed: 0,
            current_reward_adjustment_period_index: 0,
            current_adjustment_cycle_index: 0,
            current_fixed_block_reward: effective_player_block_reward(config),
            current_fixed_player_reward: effective_player_block_reward(config),
            current_fixed_block_reward_basis_period_index: 0,
            current_fixed_player_reward_basis_cycle_index: 0,
            current_fixed_block_reward_basis_network_weight_units: 0,
            current_fixed_player_reward_basis_total_network_weight_units: 0,
            previous_reward_adjustment_period_network_weight_units: 0,
            previous_adjustment_cycle_total_network_weight_units: 0,
            current_reward_adjustment_period_network_weight_units: 0,
            current_adjustment_cycle_total_network_weight_units: 0,
            pending_reward_block: None,
        };
        progress.normalize_adjustment_cycle_fields();
        progress
    }

    pub fn load_or_default(
        path: impl AsRef<Path>,
        config: &NodeConfig,
    ) -> Result<Self, NodeDaemonError> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self::default_from_config(config));
        }
        let content = fs::read_to_string(path)?;
        let mut progress = serde_json::from_str::<Self>(&content)?;
        progress.normalize_adjustment_cycle_fields();
        Ok(progress)
    }

    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), NodeDaemonError> {
        let mut normalized = self.clone();
        normalized.normalize_adjustment_cycle_fields();
        save_pretty_json(&normalized, path)
    }

    pub fn advance(&mut self, slots_per_epoch: u64) {
        self.ticks_completed += 1;
        if self.next_slot_id >= slots_per_epoch {
            self.next_epoch_id += 1;
            self.next_slot_id = 1;
        } else {
            self.next_slot_id += 1;
        }
    }

    pub fn current_reward_adjustment_period_index(&self, blocks_per_period: u64) -> u64 {
        self.reward_blocks_completed / blocks_per_period.max(1)
    }

    fn set_current_adjustment_cycle_index(&mut self, cycle_index: u64) {
        self.current_adjustment_cycle_index = cycle_index;
        self.current_reward_adjustment_period_index = cycle_index;
    }

    fn set_current_fixed_player_reward(&mut self, fixed_player_reward: u128) {
        self.current_fixed_player_reward = fixed_player_reward;
        self.current_fixed_block_reward = fixed_player_reward;
    }

    fn set_fixed_player_reward_basis(
        &mut self,
        basis_cycle_index: u64,
        basis_total_network_weight_units: u128,
    ) {
        self.current_fixed_player_reward_basis_cycle_index = basis_cycle_index;
        self.current_fixed_player_reward_basis_total_network_weight_units =
            basis_total_network_weight_units;
        self.current_fixed_block_reward_basis_period_index = basis_cycle_index;
        self.current_fixed_block_reward_basis_network_weight_units =
            basis_total_network_weight_units;
    }

    fn set_previous_adjustment_cycle_total_network_weight_units(
        &mut self,
        total_network_weight_units: u128,
    ) {
        self.previous_adjustment_cycle_total_network_weight_units = total_network_weight_units;
        self.previous_reward_adjustment_period_network_weight_units = total_network_weight_units;
    }

    fn set_current_adjustment_cycle_total_network_weight_units(
        &mut self,
        total_network_weight_units: u128,
    ) {
        self.current_adjustment_cycle_total_network_weight_units = total_network_weight_units;
        self.current_reward_adjustment_period_network_weight_units = total_network_weight_units;
    }

    fn normalize_adjustment_cycle_fields(&mut self) {
        if self.current_adjustment_cycle_index == 0 {
            self.current_adjustment_cycle_index = self.current_reward_adjustment_period_index;
        }
        if self.current_fixed_player_reward == 0 {
            self.current_fixed_player_reward = self.current_fixed_block_reward;
        }
        if self.current_fixed_player_reward_basis_cycle_index == 0 {
            self.current_fixed_player_reward_basis_cycle_index =
                self.current_fixed_block_reward_basis_period_index;
        }
        if self.current_fixed_player_reward_basis_total_network_weight_units == 0 {
            self.current_fixed_player_reward_basis_total_network_weight_units =
                self.current_fixed_block_reward_basis_network_weight_units;
        }
        if self.previous_adjustment_cycle_total_network_weight_units == 0 {
            self.previous_adjustment_cycle_total_network_weight_units =
                self.previous_reward_adjustment_period_network_weight_units;
        }
        if self.current_adjustment_cycle_total_network_weight_units == 0 {
            self.current_adjustment_cycle_total_network_weight_units =
                self.current_reward_adjustment_period_network_weight_units;
        }
        if let Some(pending) = &mut self.pending_reward_block {
            pending.normalize_whitepaper_fields();
        }
        self.sync_legacy_adjustment_fields();
    }

    fn sync_legacy_adjustment_fields(&mut self) {
        self.current_reward_adjustment_period_index = self.current_adjustment_cycle_index;
        self.current_fixed_block_reward = self.current_fixed_player_reward;
        self.current_fixed_block_reward_basis_period_index =
            self.current_fixed_player_reward_basis_cycle_index;
        self.current_fixed_block_reward_basis_network_weight_units =
            self.current_fixed_player_reward_basis_total_network_weight_units;
        self.previous_reward_adjustment_period_network_weight_units =
            self.previous_adjustment_cycle_total_network_weight_units;
        self.current_reward_adjustment_period_network_weight_units =
            self.current_adjustment_cycle_total_network_weight_units;
    }
}

impl CollectTickArtifact {
    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, NodeDaemonError> {
        load_json(path)
    }

    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), NodeDaemonError> {
        save_pretty_json(self, path)
    }
}

impl EpochCommitArtifact {
    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), NodeDaemonError> {
        save_pretty_json(self, path)
    }
}

impl RewardAdjustmentArtifact {
    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), NodeDaemonError> {
        save_pretty_json(self, path)
    }
}

impl RewardAdjustmentArtifactIndex {
    pub fn load_or_default_json(path: impl AsRef<Path>) -> Result<Self, NodeDaemonError> {
        load_json_or_default(path)
    }

    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), NodeDaemonError> {
        save_pretty_json(self, path)
    }

    pub fn upsert_adjustment_artifact(
        &mut self,
        adjustment_cycle_index: u64,
        basis_cycle_index: u64,
        fixed_player_reward: u128,
        artifact_path: String,
    ) {
        if let Some(entry) = self
            .adjustment_artifacts
            .iter_mut()
            .find(|entry| entry.adjustment_cycle_index == adjustment_cycle_index)
        {
            entry.adjustment_cycle_index = adjustment_cycle_index;
            entry.basis_cycle_index = basis_cycle_index;
            entry.fixed_player_reward = fixed_player_reward;
            entry.artifact_path = artifact_path;
        } else {
            self.adjustment_artifacts.push(RewardAdjustmentIndexEntry {
                adjustment_cycle_index,
                basis_cycle_index,
                fixed_player_reward,
                artifact_path,
            });
        }
        self.adjustment_artifacts
            .sort_by_key(|entry| entry.adjustment_cycle_index);
    }
}

impl RewardAdjustmentArtifactSummary {
    pub fn load_or_default_json(path: impl AsRef<Path>) -> Result<Self, NodeDaemonError> {
        load_json_or_default(path)
    }

    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), NodeDaemonError> {
        save_pretty_json(self, path)
    }
}

impl NodeHeartbeat {
    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, NodeDaemonError> {
        load_json(path)
    }

    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), NodeDaemonError> {
        save_pretty_json(self, path)
    }
}

struct TickCollectionContext {
    samples: Vec<crate::node_pipeline::SteamCurrentPlayersSample>,
    player_reward_tick_artifact: PlayerRewardTickArtifact,
}

struct TickEpochArtifacts {
    aggregation_artifact: Option<EpochAggregationArtifact>,
    reward_artifact: Option<EpochRewardArtifact>,
    verification_report: Option<EpochVerificationReport>,
    epoch_commit_artifact: Option<EpochCommitArtifact>,
}

pub fn run_collect_tick_with_client(
    config: &NodeConfig,
    progress: &mut LocalNodeProgress,
    client: &dyn HttpTextClient,
) -> Result<CollectTickResult, NodeDaemonError> {
    run_collect_tick_with_client_inner(config, progress, client, None)
}

pub fn run_collect_tick_with_client_and_network(
    config: &NodeConfig,
    progress: &mut LocalNodeProgress,
    client: &dyn HttpTextClient,
    network: &mut impl P2pNetwork,
) -> Result<CollectTickResult, NodeDaemonError> {
    run_collect_tick_with_client_inner(config, progress, client, Some(network))
}

fn run_collect_tick_with_client_inner(
    config: &NodeConfig,
    progress: &mut LocalNodeProgress,
    client: &dyn HttpTextClient,
    mut network: Option<&mut (dyn P2pNetwork + '_)>,
) -> Result<CollectTickResult, NodeDaemonError> {
    if config.runtime.target_app_ids.is_empty() {
        return Err(NodeDaemonError::NoTargetApps);
    }

    let previous_heartbeat = NodeHeartbeat::load_json(heartbeat_path(config)).ok();

    let ledger_path = retention_book_path(config);
    let retention_book =
        LocalRetentionBook::load_or_default_json(&ledger_path, config.storage.quota_gb)?;
    let mut runtime = LocalNodeRuntime::new(config.clone(), retention_book);
    let TickCollectionContext {
        samples,
        player_reward_tick_artifact,
    } = collect_tick_context(config, progress, client)?;

    let attached_network = network.is_some();
    let outcome = if let Some(network) = network.as_deref_mut() {
        network
            .register_peer(config.node_id()?)
            .map_err(|err| NodeDaemonError::Io(io::Error::other(err.to_string())))?;
        runtime.collect_store_and_publish_samples(
            progress.next_epoch_id,
            progress.next_slot_id,
            samples,
            network,
        )?
    } else {
        runtime.collect_and_store_samples(progress.next_epoch_id, progress.next_slot_id, samples)?
    };

    write_payload_file(
        config,
        &outcome.assembled_batch.payload_cid,
        &outcome.assembled_batch.payload_bytes,
    )?;
    runtime.retention_book.save_json(&ledger_path)?;

    let artifact =
        persist_collect_tick_artifact(config, progress, &outcome, &player_reward_tick_artifact)?;
    let TickEpochArtifacts {
        aggregation_artifact,
        reward_artifact,
        verification_report,
        epoch_commit_artifact,
    } = build_tick_epoch_artifacts(config, progress)?;

    progress.set_current_adjustment_cycle_total_network_weight_units(
        progress
            .current_adjustment_cycle_total_network_weight_units
            .saturating_add(
                completed_fixed_player_reward_cycle_total_network_weight_units(
                    &player_reward_tick_artifact,
                ),
            ),
    );
    progress.reward_blocks_completed = progress
        .reward_blocks_completed
        .saturating_add(player_reward_tick_artifact.completed_reward_block_count as u64);
    progress.advance(config.runtime.slots_per_epoch);
    progress.save_json(progress_path(config))?;

    let auto_settlement_enabled = auto_settlement_enabled(config);
    let auto_settlement_pending_epochs = pending_auto_settlement_epochs(config, progress);
    let auto_settlement_outcome = if auto_settlement_enabled {
        auto_settle_pending_epochs(config, &auto_settlement_pending_epochs)
    } else {
        AutoSettlementOutcome::default()
    };
    let settlement_artifacts = auto_settlement_outcome.artifacts;
    let auto_settlement_error = auto_settlement_outcome.error;
    let unresolved_auto_settlement_epochs = auto_settlement_pending_epochs
        .iter()
        .copied()
        .filter(|epoch_id| {
            !settlement_artifacts
                .iter()
                .any(|artifact| artifact.epoch_id == *epoch_id)
        })
        .collect::<Vec<_>>();
    let retention_audit_artifact = audit_local_retention(config, progress.next_epoch_id)?;
    let (storage_challenge_artifact, storage_challenge_error) = match run_local_storage_challenge(
        config,
        progress.next_epoch_id,
        &retention_audit_artifact,
    ) {
        Ok(artifact) => (Some(artifact), None),
        Err(err) => (None, Some(err.to_string())),
    };
    let p2p_challenge_activity = if let (Some(network), Some(storage_challenge_artifact)) =
        (network.as_deref_mut(), storage_challenge_artifact.as_ref())
    {
        publish_storage_challenge_if_needed(
            network,
            config,
            progress.next_epoch_id,
            storage_challenge_artifact,
        )?
    } else {
        None
    };
    let p2p_challenge_events_total = previous_heartbeat
        .as_ref()
        .and_then(|heartbeat| heartbeat.p2p_challenge_events_total)
        .unwrap_or(0)
        + u64::from(p2p_challenge_activity.is_some());
    let previous_kind = p2p_challenge_activity
        .as_ref()
        .map(|activity| activity.kind);
    let p2p_bad_batch_challenge_events = previous_heartbeat
        .as_ref()
        .and_then(|heartbeat| heartbeat.p2p_bad_batch_challenge_events)
        .unwrap_or(0)
        + u64::from(matches!(
            previous_kind,
            Some(crate::primitives::ChallengeKind::BadBatch)
        ));
    let p2p_omission_challenge_events = previous_heartbeat
        .as_ref()
        .and_then(|heartbeat| heartbeat.p2p_omission_challenge_events)
        .unwrap_or(0)
        + u64::from(matches!(
            previous_kind,
            Some(crate::primitives::ChallengeKind::Omission)
        ));
    let p2p_bad_aggregate_challenge_events = previous_heartbeat
        .as_ref()
        .and_then(|heartbeat| heartbeat.p2p_bad_aggregate_challenge_events)
        .unwrap_or(0)
        + u64::from(matches!(
            previous_kind,
            Some(crate::primitives::ChallengeKind::BadAggregate)
        ));
    let p2p_bad_reward_challenge_events = previous_heartbeat
        .as_ref()
        .and_then(|heartbeat| heartbeat.p2p_bad_reward_challenge_events)
        .unwrap_or(0)
        + u64::from(matches!(
            previous_kind,
            Some(crate::primitives::ChallengeKind::BadReward)
        ));
    let p2p_bad_storage_challenge_events = previous_heartbeat
        .as_ref()
        .and_then(|heartbeat| heartbeat.p2p_bad_storage_challenge_events)
        .unwrap_or(0)
        + u64::from(matches!(
            previous_kind,
            Some(crate::primitives::ChallengeKind::BadStorage)
        ));
    let p2p_challenge_delivered_events_total = previous_heartbeat
        .as_ref()
        .and_then(|heartbeat| heartbeat.p2p_challenge_delivered_events_total)
        .unwrap_or(0)
        + u64::from(matches!(
            p2p_challenge_activity
                .as_ref()
                .map(|activity| activity.recipients > 0),
            Some(true)
        ));
    let p2p_challenge_zero_recipient_events_total = previous_heartbeat
        .as_ref()
        .and_then(|heartbeat| heartbeat.p2p_challenge_zero_recipient_events_total)
        .unwrap_or(0)
        + u64::from(matches!(
            p2p_challenge_activity
                .as_ref()
                .map(|activity| activity.recipients == 0),
            Some(true)
        ));
    let updated_at_millis = current_unix_millis()?;
    let mut recent_p2p_challenge_history = previous_heartbeat
        .as_ref()
        .map(|heartbeat| heartbeat.recent_p2p_challenge_history.clone())
        .unwrap_or_default();
    if let Some(activity) = &p2p_challenge_activity {
        recent_p2p_challenge_history.push(P2pChallengeHistoryEntry {
            kind: challenge_kind_name(activity.kind).to_string(),
            epoch_id: activity.epoch_id,
            payload_cid: activity.payload_cid.clone(),
            recipients: activity.recipients,
            recorded_at_millis: updated_at_millis,
        });
        if recent_p2p_challenge_history.len() > RECENT_P2P_CHALLENGE_HISTORY_LIMIT {
            let excess = recent_p2p_challenge_history.len() - RECENT_P2P_CHALLENGE_HISTORY_LIMIT;
            recent_p2p_challenge_history.drain(0..excess);
        }
    }
    let retention_prune_outcome = prune_retention(config, progress.next_epoch_id)?;
    let (last_p2p_retrieval_ok, last_p2p_retrieval_error) = if let Some(network) =
        network.as_deref()
    {
        let (ok, error) =
            attached_network_retrieval_health(network, config.node_id()?, &outcome.assembled_batch);
        (Some(ok), error)
    } else {
        (None, None)
    };
    let attached_network_topology = network
        .as_deref()
        .map(summarize_attached_network_topology)
        .transpose()?;
    let heartbeat = NodeHeartbeat {
        tick_epoch_id: artifact.epoch_id,
        tick_slot_id: artifact.slot_id,
        ticks_completed: progress.ticks_completed,
        last_payload_cid: artifact.payload_cid.clone(),
        updated_at_millis,
        last_p2p_batch_recipients: attached_network.then_some(outcome.batch_recipients),
        last_p2p_receipt_recipients: attached_network.then_some(outcome.receipt_recipients),
        last_p2p_retrieval_ok,
        last_p2p_retrieval_error,
        last_p2p_transport: attached_network_topology
            .as_ref()
            .map(|summary| summary.transport.clone()),
        last_p2p_known_peer_count: attached_network_topology
            .as_ref()
            .map(|summary| summary.known_peer_count),
        last_p2p_learned_remote_peer_count: attached_network_topology
            .as_ref()
            .map(|summary| summary.learned_remote_peer_count),
        last_p2p_batch_listener_count: attached_network_topology
            .as_ref()
            .map(|summary| summary.batch_listener_count),
        last_p2p_receipt_listener_count: attached_network_topology
            .as_ref()
            .map(|summary| summary.receipt_listener_count),
        last_p2p_challenge_listener_count: attached_network_topology
            .as_ref()
            .map(|summary| summary.challenge_listener_count),
        last_p2p_coordination_sent_count: attached_network_topology
            .as_ref()
            .map(|summary| summary.coordination_stats.sent_count),
        last_p2p_coordination_received_count: attached_network_topology
            .as_ref()
            .map(|summary| summary.coordination_stats.received_count),
        last_p2p_hello_sent_count: attached_network_topology
            .as_ref()
            .map(|summary| summary.coordination_stats.hello_sent_count),
        last_p2p_hint_sent_count: attached_network_topology
            .as_ref()
            .map(|summary| summary.coordination_stats.hint_sent_count),
        last_p2p_goodbye_sent_count: attached_network_topology
            .as_ref()
            .map(|summary| summary.coordination_stats.goodbye_sent_count),
        last_p2p_hello_received_count: attached_network_topology
            .as_ref()
            .map(|summary| summary.coordination_stats.hello_received_count),
        last_p2p_hint_received_count: attached_network_topology
            .as_ref()
            .map(|summary| summary.coordination_stats.hint_received_count),
        last_p2p_goodbye_received_count: attached_network_topology
            .as_ref()
            .map(|summary| summary.coordination_stats.goodbye_received_count),
        last_p2p_challenge_recipients: p2p_challenge_activity
            .as_ref()
            .map(|activity| activity.recipients),
        last_p2p_challenge_kind: p2p_challenge_activity
            .as_ref()
            .map(|activity| challenge_kind_name(activity.kind).to_string()),
        last_p2p_challenge_epoch_id: p2p_challenge_activity
            .as_ref()
            .map(|activity| activity.epoch_id),
        last_p2p_challenge_payload_cid: p2p_challenge_activity
            .as_ref()
            .and_then(|activity| activity.payload_cid.clone()),
        p2p_challenge_events_total: Some(p2p_challenge_events_total),
        p2p_bad_batch_challenge_events: Some(p2p_bad_batch_challenge_events),
        p2p_omission_challenge_events: Some(p2p_omission_challenge_events),
        p2p_bad_aggregate_challenge_events: Some(p2p_bad_aggregate_challenge_events),
        p2p_bad_reward_challenge_events: Some(p2p_bad_reward_challenge_events),
        p2p_bad_storage_challenge_events: Some(p2p_bad_storage_challenge_events),
        p2p_challenge_delivered_events_total: Some(p2p_challenge_delivered_events_total),
        p2p_challenge_zero_recipient_events_total: Some(p2p_challenge_zero_recipient_events_total),
        recent_p2p_challenge_history,
        last_retention_all_retrievable: Some(retention_audit_artifact.all_retrievable),
        last_retention_retained_payload_count: Some(
            retention_audit_artifact.retained_payload_count,
        ),
        last_retention_retrievable_payload_count: Some(
            retention_audit_artifact.retrievable_payload_count,
        ),
        last_retention_missing_payload_count: Some(retention_audit_artifact.missing_payload_count),
        last_retention_corrupted_payload_count: Some(
            retention_audit_artifact.corrupted_payload_count,
        ),
        last_storage_challenge_all_passed: storage_challenge_artifact
            .as_ref()
            .map(|artifact| artifact.all_passed),
        last_storage_challenge_checked_payload_count: storage_challenge_artifact
            .as_ref()
            .map(|artifact| artifact.checked_payload_count),
        last_storage_challenge_failed_payload_count: storage_challenge_artifact
            .as_ref()
            .map(|artifact| artifact.failed_payload_count),
        last_storage_challenge_error: storage_challenge_error.clone(),
        last_auto_settlement_pending_epoch_count: Some(unresolved_auto_settlement_epochs.len()),
        last_auto_settled_epoch: settlement_artifacts
            .last()
            .map(|artifact| artifact.epoch_id),
        last_auto_settlement_reward_claimed: settlement_artifacts
            .last()
            .map(|artifact| artifact.local_reward_claimed),
        last_auto_settlement_error: auto_settlement_error.clone(),
    };
    heartbeat.save_json(heartbeat_path(config))?;

    Ok(CollectTickResult {
        progress: progress.clone(),
        artifact,
        outcome,
        player_reward_tick_artifact,
        aggregation_artifact,
        reward_artifact,
        verification_report,
        epoch_commit_artifact,
        settlement_artifacts,
        auto_settlement_pending_epochs,
        auto_settlement_enabled,
        auto_settlement_error,
        p2p_challenge_recipients: p2p_challenge_activity
            .as_ref()
            .map(|activity| activity.recipients),
        p2p_challenge_kind: p2p_challenge_activity
            .as_ref()
            .map(|activity| challenge_kind_name(activity.kind).to_string()),
        p2p_challenge_epoch_id: p2p_challenge_activity
            .as_ref()
            .map(|activity| activity.epoch_id),
        p2p_challenge_payload_cid: p2p_challenge_activity
            .as_ref()
            .and_then(|activity| activity.payload_cid.clone()),
        retention_audit_artifact,
        storage_challenge_artifact,
        storage_challenge_error,
        retention_prune_outcome,
    })
}

fn collect_tick_context(
    config: &NodeConfig,
    progress: &mut LocalNodeProgress,
    client: &dyn HttpTextClient,
) -> Result<TickCollectionContext, NodeDaemonError> {
    let samples = collect_activity_samples(config, client)?;
    let active_game_processes = detect_active_game_processes(config);
    let foreground_process = detect_foreground_process_name();
    let sampled_interval_secs =
        effective_collect_interval_secs(config, &active_game_processes).max(1);
    let reward_adjustment_period_index = progress
        .current_reward_adjustment_period_index(config.reward.reward_adjustment_period_blocks);
    let fixed_block_reward =
        current_fixed_block_reward(config, progress, reward_adjustment_period_index, &samples);
    persist_adjustment_cycle_artifact(config, progress)?;
    let player_reward_tick = record_player_reward_tick(
        config,
        RewardTickContext {
            epoch_id: progress.next_epoch_id,
            slot_id: progress.next_slot_id,
            sampled_interval_secs,
            fixed_block_reward,
            fixed_player_reward: fixed_block_reward,
            reward_adjustment_period_index,
            adjustment_cycle_index: reward_adjustment_period_index,
            fixed_block_reward_basis_period_index: progress
                .current_fixed_block_reward_basis_period_index,
            fixed_player_reward_basis_cycle_index: progress
                .current_fixed_block_reward_basis_period_index,
            fixed_block_reward_basis_network_weight_units: progress
                .current_fixed_block_reward_basis_network_weight_units,
            fixed_player_reward_basis_total_network_weight_units: progress
                .current_fixed_block_reward_basis_network_weight_units,
        },
        progress.pending_reward_block.take(),
        foreground_process.as_deref(),
        &active_game_processes,
        &samples,
    )?;
    progress.pending_reward_block = player_reward_tick.pending_state.clone();

    Ok(TickCollectionContext {
        samples,
        player_reward_tick_artifact: player_reward_tick.artifact,
    })
}

fn current_adjustment_cycle_artifact(
    config: &NodeConfig,
    progress: &LocalNodeProgress,
) -> RewardAdjustmentArtifact {
    RewardAdjustmentArtifact {
        adjustment_cycle_index: progress.current_reward_adjustment_period_index,
        basis_cycle_index: progress.current_fixed_block_reward_basis_period_index,
        basis_total_network_weight_units: progress
            .current_fixed_block_reward_basis_network_weight_units,
        target_total_network_weight_units: effective_target_network_weight_units(config),
        reward_adjustment_cap_bps: effective_reward_adjustment_cap_bps(config),
        adjustment_cycle_blocks: config.reward.reward_adjustment_period_blocks,
        reward_block_secs: effective_reward_block_secs(config),
        base_fixed_player_reward: effective_player_block_reward(config),
        fixed_player_reward: progress.current_fixed_block_reward,
    }
}

fn persist_adjustment_cycle_artifact(
    config: &NodeConfig,
    progress: &LocalNodeProgress,
) -> Result<(), NodeDaemonError> {
    let artifact = current_adjustment_cycle_artifact(config, progress);
    let artifact_path =
        adjustment_cycle_artifact_path(config, progress.current_reward_adjustment_period_index);
    artifact.save_json(&artifact_path)?;

    let index_path = adjustment_cycle_index_path(config);
    let mut index = RewardAdjustmentArtifactIndex::load_or_default_json(&index_path)?;
    index.upsert_adjustment_artifact(
        artifact.adjustment_cycle_index,
        artifact.basis_cycle_index,
        artifact.fixed_player_reward,
        artifact_path.to_string_lossy().into_owned(),
    );
    index.save_json(&index_path)?;

    let summary = RewardAdjustmentArtifactSummary {
        adjustment_artifact_count: index.adjustment_artifacts.len(),
        adjustment_cycle_artifact_count: index.adjustment_artifacts.len(),
        latest_period_index: index
            .adjustment_artifacts
            .last()
            .map(|entry| entry.adjustment_cycle_index),
        latest_adjustment_cycle_index: index
            .adjustment_artifacts
            .last()
            .map(|entry| entry.adjustment_cycle_index),
        latest_basis_period_index: index
            .adjustment_artifacts
            .last()
            .map(|entry| entry.basis_cycle_index),
        latest_basis_cycle_index: index
            .adjustment_artifacts
            .last()
            .map(|entry| entry.basis_cycle_index),
        latest_adjusted_player_block_reward: index
            .adjustment_artifacts
            .last()
            .map(|entry| entry.fixed_player_reward),
        latest_fixed_player_reward: index
            .adjustment_artifacts
            .last()
            .map(|entry| entry.fixed_player_reward),
        artifact_index_path: index_path.to_string_lossy().into_owned(),
    };
    summary.save_json(adjustment_cycle_summary_path(config))
}

fn collect_activity_samples(
    config: &NodeConfig,
    client: &dyn HttpTextClient,
) -> Result<Vec<crate::node_pipeline::SteamCurrentPlayersSample>, NodeDaemonError> {
    if config.runtime.activity_sources.is_empty() {
        return config
            .runtime
            .target_app_ids
            .iter()
            .map(|app_id| fetch_current_players_live(client, *app_id))
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into);
    }

    let observed_at_millis = current_unix_millis()?;
    config
        .runtime
        .activity_sources
        .iter()
        .map(|source| {
            let endpoint_url = source
                .endpoint_url
                .as_deref()
                .map(|template| template.replace("{app_id}", &source.app_id.to_string()));
            collect_configured_activity_source(
                client,
                source.source_kind,
                source.app_id,
                observed_at_millis,
                endpoint_url.as_deref(),
                source.inline_json.as_deref(),
            )
            .map_err(|err| NodeDaemonError::Steam(SteamCollectorError::Http(err.to_string())))
        })
        .collect()
}

fn current_fixed_block_reward(
    config: &NodeConfig,
    progress: &mut LocalNodeProgress,
    reward_adjustment_period_index: u64,
    samples: &[crate::node_pipeline::SteamCurrentPlayersSample],
) -> u128 {
    let base_block_reward = effective_player_block_reward(config).max(1);
    if progress.current_fixed_player_reward == 0 {
        progress.set_current_fixed_player_reward(base_block_reward);
    }
    if progress.reward_blocks_completed == 0 {
        progress.set_current_adjustment_cycle_index(reward_adjustment_period_index);
        progress.set_current_fixed_player_reward(base_block_reward);
        progress.set_fixed_player_reward_basis(reward_adjustment_period_index, 0);
        return base_block_reward;
    }
    if reward_adjustment_period_index != progress.current_adjustment_cycle_index {
        let previous_period_network_weight_units =
            progress.current_adjustment_cycle_total_network_weight_units;
        let previous_period_index = progress.current_adjustment_cycle_index;
        let fixed_block_reward = if previous_period_network_weight_units == 0 {
            base_block_reward
        } else {
            adjusted_player_block_reward(
                base_block_reward,
                effective_target_network_weight_units(config),
                previous_period_network_weight_units,
                effective_reward_adjustment_cap_bps(config),
            )
        };
        progress.set_previous_adjustment_cycle_total_network_weight_units(
            previous_period_network_weight_units,
        );
        progress.set_current_adjustment_cycle_total_network_weight_units(0);
        progress.set_current_adjustment_cycle_index(reward_adjustment_period_index);
        progress.set_current_fixed_player_reward(fixed_block_reward.max(1));
        progress.set_fixed_player_reward_basis(
            previous_period_index,
            previous_period_network_weight_units,
        );
        return progress.current_fixed_player_reward;
    }
    let _ = samples;
    progress.current_fixed_player_reward.max(1)
}

fn completed_fixed_player_reward_cycle_total_network_weight_units(
    artifact: &PlayerRewardTickArtifact,
) -> u128 {
    let mut seen_block_indices = std::collections::BTreeSet::new();
    artifact
        .records
        .iter()
        .filter(|record| seen_block_indices.insert(record.block_index))
        .map(|record| record.total_network_weight_units)
        .sum()
}

fn persist_collect_tick_artifact(
    config: &NodeConfig,
    progress: &LocalNodeProgress,
    outcome: &CollectAndStoreOutcome,
    player_reward_tick_artifact: &PlayerRewardTickArtifact,
) -> Result<CollectTickArtifact, NodeDaemonError> {
    let artifact = CollectTickArtifact {
        epoch_id: progress.next_epoch_id,
        slot_id: progress.next_slot_id,
        payload_cid: outcome.assembled_batch.payload_cid.clone(),
        payload_hash_hex: crate::hex_32(outcome.assembled_batch.payload_hash),
        batch_root_hex: crate::hex_32(outcome.assembled_batch.batch_commit.batch.root),
        obs_count: outcome.assembled_batch.batch_commit.obs_count,
        player_reward_block_count: player_reward_tick_artifact.completed_reward_block_count,
        player_reward_total: player_reward_tick_artifact.total_player_reward,
        reward_process_name: player_reward_tick_artifact
            .records
            .first()
            .map(|record| record.process_name.clone()),
        stored_payload_cid: outcome
            .stored_payload
            .as_ref()
            .map(|record| record.payload_cid.clone()),
        retention_until_epoch: outcome
            .stored_payload
            .as_ref()
            .map(|record| record.retention_until_epoch),
    };
    artifact.save_json(batch_artifact_path(
        config,
        progress.next_epoch_id,
        progress.next_slot_id,
        &artifact.payload_cid,
    ))?;
    Ok(artifact)
}

fn build_tick_epoch_artifacts(
    config: &NodeConfig,
    progress: &LocalNodeProgress,
) -> Result<TickEpochArtifacts, NodeDaemonError> {
    let aggregation_artifact = if config.inline_verify_enabled() || config.inline_propose_enabled()
    {
        Some(aggregate_local_epoch(config, progress.next_epoch_id)?)
    } else {
        None
    };
    let aggregates_root = aggregation_artifact
        .as_ref()
        .map(|artifact| {
            aggregate_record_root(
                &artifact
                    .records
                    .iter()
                    .map(|record| record.aggregate.clone())
                    .collect::<Vec<_>>(),
            )
        })
        .transpose()?
        .unwrap_or([0u8; 32]);
    let reward_artifact = if config.inline_verify_enabled() || config.inline_propose_enabled() {
        Some(reward_local_epoch(config, progress.next_epoch_id)?)
    } else {
        None
    };
    let rewards_root = reward_artifact
        .as_ref()
        .map(|artifact| artifact.reward_root_hex.as_str())
        .map(decode_hex_32)
        .transpose()?
        .unwrap_or([0u8; 32]);
    let verification_report = if config.inline_verify_enabled() {
        let report = verify_local_epoch(config, progress.next_epoch_id)?;
        report.save_json(epoch_verification_artifact_path(
            config,
            progress.next_epoch_id,
        ))?;
        Some(report)
    } else {
        None
    };
    let epoch_commit_artifact = if config.inline_propose_enabled() {
        let (_commit, artifact) = build_epoch_commit_from_local_data(
            config,
            progress.next_epoch_id,
            progress.ticks_completed + 1,
            effective_challenge_window_blocks(config),
            aggregates_root,
            rewards_root,
        )?;
        Some(artifact)
    } else {
        None
    };

    Ok(TickEpochArtifacts {
        aggregation_artifact,
        reward_artifact,
        verification_report,
        epoch_commit_artifact,
    })
}

pub fn run_collect_loop_with_client(
    config: &NodeConfig,
    client: &dyn HttpTextClient,
    max_ticks: Option<u64>,
) -> Result<Vec<CollectTickResult>, NodeDaemonError> {
    run_collect_loop_with_client_inner(config, client, max_ticks, None)
}

pub fn run_collect_loop_with_client_and_network(
    config: &NodeConfig,
    client: &dyn HttpTextClient,
    max_ticks: Option<u64>,
    network: &mut impl P2pNetwork,
) -> Result<Vec<CollectTickResult>, NodeDaemonError> {
    run_collect_loop_with_client_inner(config, client, max_ticks, Some(network))
}

pub fn summarize_collect_loop_with_client(
    config: &NodeConfig,
    client: &dyn HttpTextClient,
    max_ticks: Option<u64>,
) -> Result<CollectLoopSummary, NodeDaemonError> {
    run_collect_loop_summary_with_client_inner(config, client, max_ticks, None)
}

pub fn summarize_collect_loop_with_client_and_network(
    config: &NodeConfig,
    client: &dyn HttpTextClient,
    max_ticks: Option<u64>,
    network: &mut impl P2pNetwork,
) -> Result<CollectLoopSummary, NodeDaemonError> {
    run_collect_loop_summary_with_client_inner(config, client, max_ticks, Some(network))
}

fn run_collect_loop_with_client_inner(
    config: &NodeConfig,
    client: &dyn HttpTextClient,
    max_ticks: Option<u64>,
    mut network: Option<&mut (dyn P2pNetwork + '_)>,
) -> Result<Vec<CollectTickResult>, NodeDaemonError> {
    apply_background_runtime_hints(config);
    let mut progress = LocalNodeProgress::load_or_default(progress_path(config), config)?;
    let mut results = Vec::new();

    let mut ticks_run = 0u64;
    loop {
        let result = if let Some(network) = network.as_deref_mut() {
            run_collect_tick_with_client_inner(config, &mut progress, client, Some(network))?
        } else {
            run_collect_tick_with_client(config, &mut progress, client)?
        };
        results.push(result);
        ticks_run += 1;

        if let Some(limit) = max_ticks {
            if ticks_run >= limit {
                break;
            }
        }

        let active_game_processes = detect_active_game_processes(config);
        let sleep_secs = effective_collect_interval_secs(config, &active_game_processes);
        thread::sleep(Duration::from_secs(sleep_secs));
    }

    Ok(results)
}

fn run_collect_loop_summary_with_client_inner(
    config: &NodeConfig,
    client: &dyn HttpTextClient,
    max_ticks: Option<u64>,
    mut network: Option<&mut (dyn P2pNetwork + '_)>,
) -> Result<CollectLoopSummary, NodeDaemonError> {
    apply_background_runtime_hints(config);
    let mut progress = LocalNodeProgress::load_or_default(progress_path(config), config)?;
    let mut summary = CollectLoopSummary::new(config);

    let mut ticks_run = 0u64;
    loop {
        let result = if let Some(network) = network.as_deref_mut() {
            run_collect_tick_with_client_inner(config, &mut progress, client, Some(network))?
        } else {
            run_collect_tick_with_client(config, &mut progress, client)?
        };
        summary.record_result(&result);
        ticks_run += 1;

        if let Some(limit) = max_ticks {
            if ticks_run >= limit {
                break;
            }
        }

        let active_game_processes = detect_active_game_processes(config);
        let sleep_secs = effective_collect_interval_secs(config, &active_game_processes);
        thread::sleep(Duration::from_secs(sleep_secs));
    }

    Ok(summary)
}

pub fn load_status(config: &NodeConfig) -> Result<NodeStatusSummary, NodeDaemonError> {
    let progress = LocalNodeProgress::load_or_default(progress_path(config), config)?;
    let retention_book = LocalRetentionBook::load_or_default_json(
        retention_book_path(config),
        config.storage.quota_gb,
    )?;
    let heartbeat = NodeHeartbeat::load_json(heartbeat_path(config)).ok();
    let last_aggregate = load_latest_aggregation_entry(config)?;
    let reward_adjustment_artifact_path =
        adjustment_cycle_artifact_path(config, progress.current_reward_adjustment_period_index);
    let reward_adjustment_index_path = adjustment_cycle_index_path(config);
    let reward_adjustment_summary_path = adjustment_cycle_summary_path(config);
    let reward_adjustment_artifact_count =
        RewardAdjustmentArtifactIndex::load_or_default_json(&reward_adjustment_index_path)?
            .adjustment_artifacts
            .len();

    let mut summary = NodeStatusSummary {
        next_epoch_id: progress.next_epoch_id,
        next_slot_id: progress.next_slot_id,
        ticks_completed: progress.ticks_completed,
        target_app_ids: config.runtime.target_app_ids.clone(),
        low_impact_mode: config.runtime.low_impact_mode,
        os_background_priority: config.runtime.os_background_priority,
        inline_verify_enabled: config.inline_verify_enabled(),
        inline_propose_enabled: config.inline_propose_enabled(),
        reward_block_secs: effective_reward_block_secs(config),
        reward_source: config.reward.reward_source_label().to_string(),
        emission_year: config.reward.emission_year,
        challenge_window_blocks: effective_challenge_window_blocks(config),
        reward_adjustment_period_blocks: 0,
        adjustment_cycle_blocks: config.reward.reward_adjustment_period_blocks,
        configured_player_block_reward: 0,
        effective_player_block_reward: 0,
        configured_fixed_player_reward: config.reward.player_block_reward,
        effective_fixed_player_reward: effective_player_block_reward(config),
        effective_min_retention_epochs: effective_min_retention_epochs(config),
        effective_app_weight_override_count: crate::node_rewards::latest_activated_protocol_params(
            config,
        )
        .map(|params| params.rewards.app_weight_overrides.len())
        .unwrap_or(0),
        target_network_weight_units: effective_target_network_weight_units(config),
        reward_adjustment_cap_bps: effective_reward_adjustment_cap_bps(config),
        reward_blocks_completed: progress.reward_blocks_completed,
        current_reward_adjustment_period_index: 0,
        current_adjustment_cycle_index: progress.current_adjustment_cycle_index,
        current_fixed_block_reward: 0,
        current_fixed_player_reward: progress.current_fixed_player_reward,
        current_fixed_block_reward_basis_period_index: 0,
        current_fixed_player_reward_basis_cycle_index: progress
            .current_fixed_player_reward_basis_cycle_index,
        current_fixed_block_reward_basis_network_weight_units: 0,
        current_fixed_player_reward_basis_total_network_weight_units: progress
            .current_fixed_player_reward_basis_total_network_weight_units,
        current_reward_adjustment_artifact_path: reward_adjustment_artifact_path
            .to_string_lossy()
            .into_owned(),
        current_reward_adjustment_artifact_exists: reward_adjustment_artifact_path.exists(),
        reward_adjustment_artifact_count: 0,
        adjustment_cycle_artifact_count: reward_adjustment_artifact_count,
        reward_adjustment_artifact_index_path: reward_adjustment_index_path
            .to_string_lossy()
            .into_owned(),
        reward_adjustment_artifact_summary_path: reward_adjustment_summary_path
            .to_string_lossy()
            .into_owned(),
        previous_reward_adjustment_period_network_weight_units: 0,
        previous_adjustment_cycle_total_network_weight_units: progress
            .previous_adjustment_cycle_total_network_weight_units,
        current_reward_adjustment_period_network_weight_units: 0,
        current_adjustment_cycle_total_network_weight_units: progress
            .current_adjustment_cycle_total_network_weight_units,
        pending_reward_block_seconds: progress
            .pending_reward_block
            .as_ref()
            .map(|state| state.sampled_interval_secs)
            .unwrap_or(0),
        pending_reward_block_ticks: progress
            .pending_reward_block
            .as_ref()
            .map(|state| {
                state
                    .sampled_interval_secs
                    .div_ceil(config.runtime.poll_interval_secs.max(1))
            })
            .unwrap_or(0),
        pending_reward_block_remaining_seconds: effective_reward_block_secs(config).saturating_sub(
            progress
                .pending_reward_block
                .as_ref()
                .map(|state| state.sampled_interval_secs)
                .unwrap_or(0),
        ),
        pending_reward_block_entry_count: progress
            .pending_reward_block
            .as_ref()
            .map(|state| state.entries.len())
            .unwrap_or(0),
        stored_payload_count: retention_book.payloads.len(),
        used_bytes: retention_book.used_bytes,
        quota_bytes: retention_book.quota_bytes,
        configured_p2p_batch_listener_count: config.runtime.p2p_simulation.batch_listener_count,
        configured_p2p_receipt_listener_count: config.runtime.p2p_simulation.receipt_listener_count,
        configured_p2p_dual_listener_count: config.runtime.p2p_simulation.dual_listener_count,
        last_tick_at_millis: heartbeat.as_ref().map(|item| item.updated_at_millis),
        last_payload_cid: heartbeat.as_ref().map(|item| item.last_payload_cid.clone()),
        last_aggregate_epoch_id: last_aggregate.as_ref().map(|(epoch_id, _)| *epoch_id),
        last_aggregate_gvs_tier: last_aggregate
            .as_ref()
            .map(|(_, entry)| format!("{:?}", entry.aggregate.gvs_tier)),
        last_aggregate_source_kind: last_aggregate
            .as_ref()
            .map(|(_, entry)| format!("{:?}", entry.aggregate.primary_source_kind)),
        last_aggregate_source_confidence_ppm: last_aggregate
            .as_ref()
            .map(|(_, entry)| entry.aggregate.source_confidence_ppm),
        last_p2p_batch_recipients: heartbeat
            .as_ref()
            .and_then(|item| item.last_p2p_batch_recipients),
        last_p2p_receipt_recipients: heartbeat
            .as_ref()
            .and_then(|item| item.last_p2p_receipt_recipients),
        last_p2p_retrieval_ok: heartbeat
            .as_ref()
            .and_then(|item| item.last_p2p_retrieval_ok),
        last_p2p_retrieval_error: heartbeat
            .as_ref()
            .and_then(|item| item.last_p2p_retrieval_error.clone()),
        last_p2p_transport: heartbeat
            .as_ref()
            .and_then(|item| item.last_p2p_transport.clone()),
        last_p2p_known_peer_count: heartbeat
            .as_ref()
            .and_then(|item| item.last_p2p_known_peer_count),
        last_p2p_learned_remote_peer_count: heartbeat
            .as_ref()
            .and_then(|item| item.last_p2p_learned_remote_peer_count),
        last_p2p_batch_listener_count: heartbeat
            .as_ref()
            .and_then(|item| item.last_p2p_batch_listener_count),
        last_p2p_receipt_listener_count: heartbeat
            .as_ref()
            .and_then(|item| item.last_p2p_receipt_listener_count),
        last_p2p_challenge_listener_count: heartbeat
            .as_ref()
            .and_then(|item| item.last_p2p_challenge_listener_count),
        last_p2p_coordination_sent_count: heartbeat
            .as_ref()
            .and_then(|item| item.last_p2p_coordination_sent_count),
        last_p2p_coordination_received_count: heartbeat
            .as_ref()
            .and_then(|item| item.last_p2p_coordination_received_count),
        last_p2p_hello_sent_count: heartbeat
            .as_ref()
            .and_then(|item| item.last_p2p_hello_sent_count),
        last_p2p_hint_sent_count: heartbeat
            .as_ref()
            .and_then(|item| item.last_p2p_hint_sent_count),
        last_p2p_goodbye_sent_count: heartbeat
            .as_ref()
            .and_then(|item| item.last_p2p_goodbye_sent_count),
        last_p2p_hello_received_count: heartbeat
            .as_ref()
            .and_then(|item| item.last_p2p_hello_received_count),
        last_p2p_hint_received_count: heartbeat
            .as_ref()
            .and_then(|item| item.last_p2p_hint_received_count),
        last_p2p_goodbye_received_count: heartbeat
            .as_ref()
            .and_then(|item| item.last_p2p_goodbye_received_count),
        last_p2p_challenge_recipients: heartbeat
            .as_ref()
            .and_then(|item| item.last_p2p_challenge_recipients),
        last_p2p_challenge_kind: heartbeat
            .as_ref()
            .and_then(|item| item.last_p2p_challenge_kind.clone()),
        last_p2p_challenge_epoch_id: heartbeat
            .as_ref()
            .and_then(|item| item.last_p2p_challenge_epoch_id),
        last_p2p_challenge_payload_cid: heartbeat
            .as_ref()
            .and_then(|item| item.last_p2p_challenge_payload_cid.clone()),
        p2p_challenge_events_total: heartbeat
            .as_ref()
            .and_then(|item| item.p2p_challenge_events_total),
        p2p_bad_batch_challenge_events: heartbeat
            .as_ref()
            .and_then(|item| item.p2p_bad_batch_challenge_events),
        p2p_omission_challenge_events: heartbeat
            .as_ref()
            .and_then(|item| item.p2p_omission_challenge_events),
        p2p_bad_aggregate_challenge_events: heartbeat
            .as_ref()
            .and_then(|item| item.p2p_bad_aggregate_challenge_events),
        p2p_bad_reward_challenge_events: heartbeat
            .as_ref()
            .and_then(|item| item.p2p_bad_reward_challenge_events),
        p2p_bad_storage_challenge_events: heartbeat
            .as_ref()
            .and_then(|item| item.p2p_bad_storage_challenge_events),
        p2p_challenge_delivered_events_total: heartbeat
            .as_ref()
            .and_then(|item| item.p2p_challenge_delivered_events_total),
        p2p_challenge_zero_recipient_events_total: heartbeat
            .as_ref()
            .and_then(|item| item.p2p_challenge_zero_recipient_events_total),
        recent_p2p_challenge_events: heartbeat
            .as_ref()
            .map(|item| item.recent_p2p_challenge_history.len()),
        recent_p2p_bad_batch_challenge_events: heartbeat.as_ref().map(|item| {
            item.recent_p2p_challenge_history
                .iter()
                .filter(|entry| {
                    challenge_kind_matches_name(
                        &entry.kind,
                        crate::primitives::ChallengeKind::BadBatch,
                    )
                })
                .count()
        }),
        recent_p2p_omission_challenge_events: heartbeat.as_ref().map(|item| {
            item.recent_p2p_challenge_history
                .iter()
                .filter(|entry| {
                    challenge_kind_matches_name(
                        &entry.kind,
                        crate::primitives::ChallengeKind::Omission,
                    )
                })
                .count()
        }),
        recent_p2p_bad_aggregate_challenge_events: heartbeat.as_ref().map(|item| {
            item.recent_p2p_challenge_history
                .iter()
                .filter(|entry| {
                    challenge_kind_matches_name(
                        &entry.kind,
                        crate::primitives::ChallengeKind::BadAggregate,
                    )
                })
                .count()
        }),
        recent_p2p_bad_reward_challenge_events: heartbeat.as_ref().map(|item| {
            item.recent_p2p_challenge_history
                .iter()
                .filter(|entry| {
                    challenge_kind_matches_name(
                        &entry.kind,
                        crate::primitives::ChallengeKind::BadReward,
                    )
                })
                .count()
        }),
        recent_p2p_bad_storage_challenge_events: heartbeat.as_ref().map(|item| {
            item.recent_p2p_challenge_history
                .iter()
                .filter(|entry| entry.kind == "BadStorage")
                .count()
        }),
        recent_p2p_challenge_delivered_events: heartbeat.as_ref().map(|item| {
            item.recent_p2p_challenge_history
                .iter()
                .filter(|entry| entry.recipients > 0)
                .count()
        }),
        recent_p2p_challenge_zero_recipient_events: heartbeat.as_ref().map(|item| {
            item.recent_p2p_challenge_history
                .iter()
                .filter(|entry| entry.recipients == 0)
                .count()
        }),
        recent_p2p_challenge_recipient_sum: heartbeat.as_ref().map(|item| {
            item.recent_p2p_challenge_history
                .iter()
                .map(|entry| entry.recipients)
                .sum()
        }),
        last_retention_all_retrievable: heartbeat
            .as_ref()
            .and_then(|item| item.last_retention_all_retrievable),
        last_retention_retained_payload_count: heartbeat
            .as_ref()
            .and_then(|item| item.last_retention_retained_payload_count),
        last_retention_retrievable_payload_count: heartbeat
            .as_ref()
            .and_then(|item| item.last_retention_retrievable_payload_count),
        last_retention_missing_payload_count: heartbeat
            .as_ref()
            .and_then(|item| item.last_retention_missing_payload_count),
        last_retention_corrupted_payload_count: heartbeat
            .as_ref()
            .and_then(|item| item.last_retention_corrupted_payload_count),
        last_storage_challenge_all_passed: heartbeat
            .as_ref()
            .and_then(|item| item.last_storage_challenge_all_passed),
        last_storage_challenge_checked_payload_count: heartbeat
            .as_ref()
            .and_then(|item| item.last_storage_challenge_checked_payload_count),
        last_storage_challenge_failed_payload_count: heartbeat
            .as_ref()
            .and_then(|item| item.last_storage_challenge_failed_payload_count),
        last_storage_challenge_error: heartbeat
            .as_ref()
            .and_then(|item| item.last_storage_challenge_error.clone()),
        last_auto_settlement_pending_epoch_count: heartbeat
            .as_ref()
            .and_then(|item| item.last_auto_settlement_pending_epoch_count),
        last_auto_settled_epoch: heartbeat
            .as_ref()
            .and_then(|item| item.last_auto_settled_epoch),
        last_auto_settlement_reward_claimed: heartbeat
            .as_ref()
            .and_then(|item| item.last_auto_settlement_reward_claimed),
        last_auto_settlement_error: heartbeat
            .as_ref()
            .and_then(|item| item.last_auto_settlement_error.clone()),
    };
    summary.normalize_adjustment_cycle_fields();
    Ok(summary)
}

fn load_latest_aggregation_entry(
    config: &NodeConfig,
) -> Result<Option<(u64, crate::node_aggregator::EpochAggregateEntry)>, NodeDaemonError> {
    let aggregates_dir = data_dir_subpath(config, "aggregates");
    if !aggregates_dir.exists() {
        return Ok(None);
    }

    let mut latest_path = None;
    let mut latest_epoch_id = 0u64;
    for entry in fs::read_dir(&aggregates_dir)? {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        if let Some(epoch_id) = epoch_id_from_artifact_path(&path) {
            if latest_path.is_none() || epoch_id > latest_epoch_id {
                latest_epoch_id = epoch_id;
                latest_path = Some(path);
            }
        }
    }

    let Some(path) = latest_path else {
        return Ok(None);
    };
    let content = fs::read_to_string(path)?;
    let artifact: EpochAggregationArtifact = serde_json::from_str(&content)?;
    Ok(artifact
        .records
        .into_iter()
        .max_by_key(|entry| entry.aggregate.gvs_microunits)
        .map(|entry| (artifact.epoch_id, entry)))
}

fn auto_settlement_enabled(config: &NodeConfig) -> bool {
    config.capabilities.propose
}

fn pending_auto_settlement_epochs(config: &NodeConfig, progress: &LocalNodeProgress) -> Vec<u64> {
    let latest_completed_epoch = progress.next_epoch_id.saturating_sub(1);
    if latest_completed_epoch < config.collect.default_epoch_id {
        return Vec::new();
    }

    let mut pending = Vec::new();
    for epoch_id in config.collect.default_epoch_id..=latest_completed_epoch {
        if !epoch_settlement_artifact_path(config, epoch_id).exists() {
            pending.push(epoch_id);
        }
    }
    pending
}

#[derive(Debug, Default)]
struct AutoSettlementOutcome {
    artifacts: Vec<EpochSettlementArtifact>,
    error: Option<String>,
}

fn auto_settle_pending_epochs(config: &NodeConfig, epoch_ids: &[u64]) -> AutoSettlementOutcome {
    let mut artifacts = Vec::new();
    for &epoch_id in epoch_ids {
        let submission_height = match suggested_settlement_height(config) {
            Ok(height) => height,
            Err(err) => {
                return AutoSettlementOutcome {
                    artifacts,
                    error: Some(format!("epoch {epoch_id}: {err}")),
                };
            }
        };
        let artifact = match settle_local_epoch(
            config,
            epoch_id,
            submission_height,
            effective_challenge_window_blocks(config),
        ) {
            Ok(artifact) => artifact,
            Err(err) => {
                return AutoSettlementOutcome {
                    artifacts,
                    error: Some(format!("epoch {epoch_id}: {err}")),
                };
            }
        };
        artifacts.push(artifact);
    }
    AutoSettlementOutcome {
        artifacts,
        error: None,
    }
}

pub fn prune_retention(
    config: &NodeConfig,
    current_epoch: u64,
) -> Result<PruneOutcome, NodeDaemonError> {
    let ledger_path = retention_book_path(config);
    let mut retention_book =
        LocalRetentionBook::load_or_default_json(&ledger_path, config.storage.quota_gb)?;
    let removed = retention_book.prune_expired(current_epoch);
    retention_book.save_json(&ledger_path)?;

    for record in &removed {
        let path = payload_path(config, &record.payload_cid);
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    prune_batch_artifacts_for_payloads(
        config,
        &removed
            .iter()
            .map(|record| record.payload_cid.clone())
            .collect::<BTreeSet<_>>(),
    )?;
    prune_expired_retention_audit_artifacts(config, current_epoch)?;
    prune_expired_settled_epoch_intermediate_artifacts(config, current_epoch)?;

    Ok(PruneOutcome {
        current_epoch,
        removed_payloads: removed
            .into_iter()
            .map(|record| record.payload_cid)
            .collect(),
    })
}

fn prune_batch_artifacts_for_payloads(
    config: &NodeConfig,
    removed_payload_cids: &BTreeSet<String>,
) -> Result<(), NodeDaemonError> {
    if removed_payload_cids.is_empty() {
        return Ok(());
    }

    let batches_dir = data_dir_subpath(config, "batches");
    if !batches_dir.exists() {
        return Ok(());
    }

    let mut entries = fs::read_dir(&batches_dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let artifact = CollectTickArtifact::load_json(&path)?;
        if removed_payload_cids.contains(&artifact.payload_cid) {
            fs::remove_file(path)?;
        }
    }

    Ok(())
}

fn prune_expired_settled_epoch_intermediate_artifacts(
    config: &NodeConfig,
    current_epoch: u64,
) -> Result<(), NodeDaemonError> {
    let oldest_epoch_to_keep =
        current_epoch.saturating_sub(u64::from(effective_min_retention_epochs(config)));
    for dir_name in [
        "epochs",
        "prepared-epochs",
        "verifications",
        "aggregates",
        "rewards",
        "player-reward-blocks",
    ] {
        prune_expired_settled_epoch_artifacts_in_dir(config, dir_name, oldest_epoch_to_keep)?;
    }

    Ok(())
}

fn prune_expired_settled_epoch_artifacts_in_dir(
    config: &NodeConfig,
    dir_name: &str,
    oldest_epoch_to_keep: u64,
) -> Result<(), NodeDaemonError> {
    let artifact_dir = data_dir_subpath(config, dir_name);
    if !artifact_dir.exists() {
        return Ok(());
    }

    let mut entries = fs::read_dir(&artifact_dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let Some(epoch_id) = epoch_id_from_artifact_path(&path) else {
            continue;
        };
        if epoch_id < oldest_epoch_to_keep
            && epoch_settlement_artifact_path(config, epoch_id).exists()
        {
            fs::remove_file(path)?;
        }
    }

    Ok(())
}

fn prune_expired_retention_audit_artifacts(
    config: &NodeConfig,
    current_epoch: u64,
) -> Result<(), NodeDaemonError> {
    let audits_dir = data_dir_subpath(config, "retention-audits");
    if !audits_dir.exists() {
        return Ok(());
    }

    let oldest_epoch_to_keep =
        current_epoch.saturating_sub(u64::from(effective_min_retention_epochs(config)));
    let mut entries = fs::read_dir(&audits_dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let content = fs::read_to_string(&path)?;
        let artifact = serde_json::from_str::<RetentionAuditArtifact>(&content)?;
        if artifact.current_epoch < oldest_epoch_to_keep {
            fs::remove_file(path)?;
        }
    }

    Ok(())
}

pub fn build_epoch_commit_from_local_data(
    config: &NodeConfig,
    epoch_id: u64,
    current_height: u64,
    challenge_window_blocks: u32,
    aggregates_root: Hash32,
    rewards_root: Hash32,
) -> Result<(EpochCommit, EpochCommitArtifact), NodeDaemonError> {
    let batches = load_batches_for_epoch(config, epoch_id)?;
    let resolved_aggregates_root = if aggregates_root == [0u8; 32] {
        let artifact = aggregate_local_epoch(config, epoch_id)?;
        aggregate_record_root(
            &artifact
                .records
                .iter()
                .map(|record| record.aggregate.clone())
                .collect::<Vec<_>>(),
        )?
    } else {
        aggregates_root
    };
    let resolved_rewards_root = if rewards_root == [0u8; 32] {
        let artifact = reward_local_epoch(config, epoch_id)?;
        decode_hex_32(&artifact.reward_root_hex)?
    } else {
        rewards_root
    };
    let retention_book = LocalRetentionBook::load_or_default_json(
        retention_book_path(config),
        config.storage.quota_gb,
    )?;
    let stored_payloads = retention_book
        .payloads
        .values()
        .filter(|record| record.epoch_id == epoch_id)
        .cloned()
        .collect::<Vec<_>>();

    let runtime = LocalNodeRuntime::new(config.clone(), retention_book);
    let epoch_commit = runtime.build_epoch_commit(crate::node_runtime::EpochCommitInputs {
        epoch_id,
        current_height,
        challenge_window_blocks,
        batches: &batches,
        stored_payloads: &stored_payloads,
        aggregates_root: resolved_aggregates_root,
        rewards_root: resolved_rewards_root,
    })?;

    let artifact = EpochCommitArtifact {
        epoch_id,
        current_height,
        challenge_deadline_height: epoch_commit.challenge_deadline_height,
        batch_count: epoch_commit.accepted_batches.leaf_count,
        payload_count: epoch_commit.availability.leaf_count,
        accepted_batches_root_hex: crate::hex_32(epoch_commit.accepted_batches.root),
        observations_root_hex: crate::hex_32(epoch_commit.observations.root),
        availability_root_hex: crate::hex_32(epoch_commit.availability.root),
        aggregates_root_hex: crate::hex_32(epoch_commit.aggregates.root),
        rewards_root_hex: crate::hex_32(epoch_commit.rewards.root),
        randomness_seed_hex: crate::hex_32(epoch_commit.randomness_seed),
    };
    artifact.save_json(epoch_commit_artifact_path(config, epoch_id))?;

    Ok((epoch_commit, artifact))
}

pub fn progress_path(config: &NodeConfig) -> PathBuf {
    data_dir_file_path(config, "runtime_state.json")
}

pub fn retention_book_path(config: &NodeConfig) -> PathBuf {
    data_dir_file_path(config, "retention_book.json")
}

pub fn heartbeat_path(config: &NodeConfig) -> PathBuf {
    data_dir_file_path(config, "heartbeat.json")
}

pub fn payload_path(config: &NodeConfig, payload_cid: &str) -> PathBuf {
    data_dir_subpath(config, "payloads").join(format!("{}.bin", payload_filename_stub(payload_cid)))
}

pub fn batch_artifact_path(
    config: &NodeConfig,
    epoch_id: u64,
    slot_id: u64,
    payload_cid: &str,
) -> PathBuf {
    let payload_stub = payload_artifact_stub(payload_cid);
    data_dir_subpath(config, "batches").join(format!(
        "epoch-{epoch_id:06}-slot-{slot_id:06}-{payload_stub}.json"
    ))
}

pub fn epoch_commit_artifact_path(config: &NodeConfig, epoch_id: u64) -> PathBuf {
    epoch_artifact_path(config, "epochs", epoch_id)
}

pub fn epoch_verification_artifact_path(config: &NodeConfig, epoch_id: u64) -> PathBuf {
    epoch_artifact_path(config, "verifications", epoch_id)
}

pub fn epoch_aggregation_artifact_path(config: &NodeConfig, epoch_id: u64) -> PathBuf {
    epoch_artifact_path(config, "aggregates", epoch_id)
}

pub fn epoch_reward_artifact_path(config: &NodeConfig, epoch_id: u64) -> PathBuf {
    epoch_artifact_path(config, "rewards", epoch_id)
}

pub fn epoch_preparation_artifact_path(config: &NodeConfig, epoch_id: u64) -> PathBuf {
    epoch_artifact_path(config, "prepared-epochs", epoch_id)
}

pub fn epoch_settlement_artifact_path(config: &NodeConfig, epoch_id: u64) -> PathBuf {
    epoch_artifact_path(config, "settlements", epoch_id)
}

pub fn local_chain_runtime_path(config: &NodeConfig) -> PathBuf {
    data_dir_subpath(config, "local-chain").join("runtime.json")
}

pub fn local_chain_store_path(config: &NodeConfig) -> PathBuf {
    data_dir_subpath(config, "local-chain").join("store.bin")
}

pub fn governance_proposal_artifact_path(config: &NodeConfig, proposal_id_hex: &str) -> PathBuf {
    data_dir_subpath(config, "local-chain")
        .join("governance")
        .join("proposals")
        .join(format!("{proposal_id_hex}.json"))
}

pub fn governance_scheduled_artifact_path(config: &NodeConfig, epoch_id: u64) -> PathBuf {
    data_dir_subpath(config, "local-chain")
        .join("governance")
        .join("scheduled")
        .join(format!("epoch-{epoch_id:06}.json"))
}

pub fn governance_index_artifact_path(config: &NodeConfig) -> PathBuf {
    data_dir_subpath(config, "local-chain")
        .join("governance")
        .join("index.json")
}

pub fn governance_summary_artifact_path(config: &NodeConfig) -> PathBuf {
    data_dir_subpath(config, "local-chain")
        .join("governance")
        .join("summary.json")
}

pub fn reward_adjustment_artifact_path(config: &NodeConfig, period_index: u64) -> PathBuf {
    data_dir_subpath(config, "reward-adjustments").join(format!("period-{period_index:06}.json"))
}

pub fn adjustment_cycle_artifact_path(config: &NodeConfig, cycle_index: u64) -> PathBuf {
    reward_adjustment_artifact_path(config, cycle_index)
}

pub fn reward_adjustment_index_path(config: &NodeConfig) -> PathBuf {
    data_dir_subpath(config, "reward-adjustments").join("index.json")
}

pub fn adjustment_cycle_index_path(config: &NodeConfig) -> PathBuf {
    reward_adjustment_index_path(config)
}

pub fn reward_adjustment_summary_path(config: &NodeConfig) -> PathBuf {
    data_dir_subpath(config, "reward-adjustments").join("summary.json")
}

pub fn adjustment_cycle_summary_path(config: &NodeConfig) -> PathBuf {
    reward_adjustment_summary_path(config)
}

fn data_dir_path(config: &NodeConfig) -> &Path {
    Path::new(&config.runtime.data_dir)
}

fn data_dir_file_path(config: &NodeConfig, file_name: &str) -> PathBuf {
    data_dir_path(config).join(file_name)
}

fn data_dir_subpath(config: &NodeConfig, dir_name: &str) -> PathBuf {
    data_dir_path(config).join(dir_name)
}

fn epoch_artifact_path(config: &NodeConfig, dir_name: &str, epoch_id: u64) -> PathBuf {
    data_dir_subpath(config, dir_name).join(format!("epoch-{epoch_id:06}.json"))
}

fn epoch_id_from_artifact_path(path: &Path) -> Option<u64> {
    let stem = path.file_stem()?.to_str()?;
    let epoch_suffix = stem.strip_prefix("epoch-")?;
    let digits = epoch_suffix
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<u64>().ok()
}

fn payload_filename_stub(payload_cid: &str) -> String {
    payload_cid.replace("cid://", "").replace('/', "_")
}

fn payload_artifact_stub(payload_cid: &str) -> String {
    payload_filename_stub(payload_cid).replace(':', "_")
}

fn write_payload_file(
    config: &NodeConfig,
    payload_cid: &str,
    payload_bytes: &[u8],
) -> Result<(), NodeDaemonError> {
    let path = payload_path(config, payload_cid);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, payload_bytes)?;
    Ok(())
}

fn current_unix_millis() -> Result<u64, NodeDaemonError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| NodeDaemonError::Io(io::Error::other(err.to_string())))?;
    Ok(duration.as_millis() as u64)
}

fn apply_background_runtime_hints(config: &NodeConfig) {
    if !config.runtime.os_background_priority {
        return;
    }

    #[cfg(windows)]
    {
        let script = format!(
            "$p = Get-Process -Id {}; $p.PriorityClass = 'Idle'",
            std::process::id()
        );
        let _ = Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &script,
            ])
            .status();
    }
}

pub fn detect_active_game_processes(config: &NodeConfig) -> Vec<String> {
    detect_active_process_names(&config.runtime.game_process_names)
}

pub fn detect_foreground_process_name() -> Option<String> {
    if let Ok(override_name) = env::var("POLE_CLIENT_FOREGROUND_PROCESS_OVERRIDE") {
        return display_process_name(&override_name);
    }

    #[cfg(windows)]
    {
        let script = r#"
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class PoLEForegroundWindow {
    [DllImport("user32.dll")]
    public static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);
}
"@
$hwnd = [PoLEForegroundWindow]::GetForegroundWindow()
if ($hwnd -eq [IntPtr]::Zero) { return }
$pid = 0
[PoLEForegroundWindow]::GetWindowThreadProcessId($hwnd, [ref]$pid) | Out-Null
if ($pid -gt 0) {
    Get-Process -Id $pid -ErrorAction SilentlyContinue | Select-Object -ExpandProperty ProcessName
}
"#;
        let output = run_powershell_script(script)?;
        let name = output
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())?;
        display_process_name(name)
    }

    #[cfg(not(windows))]
    {
        None
    }
}

pub fn effective_collect_interval_secs(
    config: &NodeConfig,
    active_game_processes: &[String],
) -> u64 {
    if config.runtime.low_impact_mode && !active_game_processes.is_empty() {
        config
            .runtime
            .poll_interval_secs
            .max(config.runtime.game_active_poll_interval_secs)
    } else {
        config.runtime.poll_interval_secs
    }
}

fn detect_active_process_names(process_names: &[String]) -> Vec<String> {
    let configured = normalize_process_names(process_names);
    if configured.is_empty() {
        return Vec::new();
    }

    #[cfg(windows)]
    {
        let targets = configured
            .iter()
            .map(|name| format!("'{}'", name.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(",");
        let script = format!(
            "$targets = @({targets}); Get-Process -ErrorAction SilentlyContinue | Where-Object {{ $targets -contains $_.ProcessName.ToLowerInvariant() }} | Select-Object -ExpandProperty ProcessName -Unique"
        );
        if let Some(output) = run_powershell_script(&script) {
            let running = output
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(|line| line.to_string())
                .collect::<Vec<_>>();
            return match_configured_process_names(&configured, &running);
        }
    }

    Vec::new()
}

fn normalize_process_names(process_names: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();
    for name in process_names {
        let name = normalize_process_name(name);
        if !name.is_empty() && seen.insert(name.clone()) {
            normalized.push(name);
        }
    }
    normalized
}

fn match_configured_process_names(configured: &[String], running: &[String]) -> Vec<String> {
    let running_set = normalize_process_names(running)
        .into_iter()
        .collect::<BTreeSet<_>>();
    configured
        .iter()
        .filter(|name| running_set.contains(*name))
        .cloned()
        .collect()
}

fn normalize_process_name(input: &str) -> String {
    input
        .trim()
        .to_ascii_lowercase()
        .trim_end_matches(".exe")
        .to_string()
}

fn display_process_name(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let base = if trimmed.len() >= 4 && trimmed[trimmed.len() - 4..].eq_ignore_ascii_case(".exe") {
        &trimmed[..trimmed.len() - 4]
    } else {
        trimmed
    };
    Some(format!("{base}.exe"))
}

fn run_powershell_script(script: &str) -> Option<String> {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn load_batches_for_epoch(
    config: &NodeConfig,
    epoch_id: u64,
) -> Result<Vec<AssembledBatch>, NodeDaemonError> {
    let batches_dir = Path::new(&config.runtime.data_dir).join("batches");
    if !batches_dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = fs::read_dir(&batches_dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    let collector_id = config.node_id()?;
    let mut batches = Vec::new();
    for entry in entries {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let artifact = CollectTickArtifact::load_json(&path)?;
        if artifact.epoch_id != epoch_id {
            continue;
        }

        let payload_file = payload_path(config, &artifact.payload_cid);
        let payload_bytes = fs::read(&payload_file)?;
        let observations = borsh::from_slice::<Vec<ObservationRecord>>(&payload_bytes)
            .map_err(|err| NodeDaemonError::Storage(StorageBookError::Borsh(err.to_string())))?;
        let payload_hash = decode_hex_32(&artifact.payload_hash_hex)?;
        let batch_root = decode_hex_32(&artifact.batch_root_hex)?;
        let slot_id = artifact.slot_id;

        batches.push(AssembledBatch {
            batch_commit: crate::records::BatchCommit {
                epoch_id: artifact.epoch_id,
                collector_id,
                slot_start: slot_id,
                slot_end: slot_id,
                batch: crate::MerkleCommitment {
                    root: batch_root,
                    leaf_count: artifact.obs_count,
                },
                payload_cid: artifact.payload_cid.clone(),
                obs_count: artifact.obs_count,
                submitted_at_height: 0,
            },
            payload_hash,
            payload_cid: artifact.payload_cid,
            payload_bytes,
            observations,
        });
    }

    Ok(batches)
}

fn decode_hex_32(input: &str) -> Result<Hash32, NodeDaemonError> {
    if input.len() != 64 {
        return Err(NodeDaemonError::InvalidHexLength {
            expected: 64,
            actual: input.len(),
        });
    }

    let mut out = [0u8; 32];
    let bytes = input.as_bytes();
    for index in 0..32 {
        let hi = decode_nibble(bytes[index * 2], index * 2)?;
        let lo = decode_nibble(bytes[index * 2 + 1], index * 2 + 1)?;
        out[index] = (hi << 4) | lo;
    }
    Ok(out)
}

fn decode_nibble(byte: u8, index: usize) -> Result<u8, NodeDaemonError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(NodeDaemonError::InvalidHexCharacter { index, byte }),
    }
}

fn attached_network_retrieval_health(
    network: &dyn P2pNetwork,
    local_node_id: Hash32,
    batch: &AssembledBatch,
) -> (bool, Option<String>) {
    let Some(requester) = network
        .known_peers()
        .ok()
        .unwrap_or_default()
        .into_iter()
        .find(|peer_id| peer_id != &local_node_id)
    else {
        return (false, Some("no remote retrieval peer available".into()));
    };

    match network.request_payload(requester, &batch.payload_cid) {
        Ok(response) if response.payload_cid != batch.payload_cid => (
            false,
            Some("retrieved payload cid did not match published payload".into()),
        ),
        Ok(response) if response.payload_hash != batch.payload_hash => (
            false,
            Some("retrieved payload hash did not match published payload".into()),
        ),
        Ok(response) if response.payload_bytes != batch.payload_bytes => (
            false,
            Some("retrieved payload bytes did not match published payload".into()),
        ),
        Ok(_) => (true, None),
        Err(err) => (false, Some(err.to_string())),
    }
}

fn publish_storage_challenge_if_needed(
    network: &mut (impl P2pNetwork + ?Sized),
    config: &NodeConfig,
    current_epoch: u64,
    artifact: &StorageChallengeArtifact,
) -> Result<Option<P2pChallengeActivity>, NodeDaemonError> {
    if artifact.failed_payload_count == 0 {
        return Ok(None);
    }

    let Some(failed_record) = artifact.records.iter().find(|record| {
        !record.payload_retrievable
            || !record.receipt_payload_matches
            || !record.receipt_epoch_matches
            || !record.receipt_storer_matches
            || !record.receipt_retention_matches
            || !record.receipt_signature_matches
    }) else {
        return Ok(None);
    };

    let local_node_id = config.node_id()?;
    network
        .register_peer(local_node_id)
        .map_err(|err| NodeDaemonError::Io(io::Error::other(err.to_string())))?;
    let challenge = Challenge {
        challenge_id: crate::stable_hash32(
            format!(
                "storage-challenge:{}:{}:{}",
                current_epoch, failed_record.epoch_id, failed_record.payload_cid
            )
            .as_bytes(),
        ),
        kind: crate::primitives::ChallengeKind::BadStorage,
        epoch_id: failed_record.epoch_id,
        target_node: Some(local_node_id),
        challenger: [0x55; 32],
        bond: 0,
        opened_at_height: 0,
        deadline_height: 0,
        state: crate::primitives::ChallengeState::Open,
        evidence: ChallengeEvidenceRef {
            batch_root: None,
            aggregate_root: None,
            reward_root: None,
            payload_cid: Some(failed_record.payload_cid.clone()),
            merkle_proof: Vec::new(),
        },
    };

    let recipients = publish_p2p_best_effort(
        network,
        local_node_id,
        P2pMessage::Challenge(crate::challenge_announcement_from_challenge(&challenge)),
    )?;
    Ok(Some(P2pChallengeActivity {
        recipients,
        kind: challenge.kind,
        epoch_id: challenge.epoch_id,
        payload_cid: challenge.evidence.payload_cid.clone(),
    }))
}

fn publish_p2p_best_effort(
    network: &mut (impl P2pNetwork + ?Sized),
    from: Hash32,
    message: P2pMessage,
) -> Result<usize, NodeDaemonError> {
    match network.publish(from, message) {
        Ok(count) => Ok(count),
        Err(crate::p2p::P2pError::NoSubscribers(P2pTopic::Challenges))
        | Err(crate::p2p::P2pError::NoSubscribers(_)) => Ok(0),
        Err(err) => Err(NodeDaemonError::Io(io::Error::other(err.to_string()))),
    }
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

fn challenge_kind_matches_name(name: &str, kind: crate::primitives::ChallengeKind) -> bool {
    name == challenge_kind_name(kind)
}

fn summarize_attached_network_topology(
    network: &dyn P2pNetwork,
) -> Result<AttachedNetworkTopologySummary, NodeDaemonError> {
    let transport = network.backend_kind().to_string();
    let peer_ids = network
        .known_peers()
        .map_err(|err| NodeDaemonError::Io(io::Error::other(err.to_string())))?;
    let learned_remote_peer_count = network
        .learned_remote_peer_count()
        .map_err(|err| NodeDaemonError::Io(io::Error::other(err.to_string())))?;
    let coordination_stats = network
        .coordination_stats()
        .map_err(|err| NodeDaemonError::Io(io::Error::other(err.to_string())))?;
    let mut batch_listener_count = 0usize;
    let mut receipt_listener_count = 0usize;
    let mut challenge_listener_count = 0usize;

    for peer_id in &peer_ids {
        let subscriptions = network
            .subscriptions_for(*peer_id)
            .map_err(|err| NodeDaemonError::Io(io::Error::other(err.to_string())))?;
        if subscriptions.contains(&crate::p2p::P2pTopic::Batches) {
            batch_listener_count += 1;
        }
        if subscriptions.contains(&crate::p2p::P2pTopic::Receipts) {
            receipt_listener_count += 1;
        }
        if subscriptions.contains(&crate::p2p::P2pTopic::Challenges) {
            challenge_listener_count += 1;
        }
    }

    Ok(AttachedNetworkTopologySummary {
        transport,
        known_peer_count: peer_ids.len(),
        learned_remote_peer_count,
        batch_listener_count,
        receipt_listener_count,
        challenge_listener_count,
        coordination_stats,
    })
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct P2pChallengeActivity {
    recipients: usize,
    kind: crate::primitives::ChallengeKind,
    epoch_id: u64,
    payload_cid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AttachedNetworkTopologySummary {
    transport: String,
    known_peer_count: usize,
    learned_remote_peer_count: usize,
    batch_listener_count: usize,
    receipt_listener_count: usize,
    challenge_listener_count: usize,
    coordination_stats: crate::p2p::P2pCoordinationStats,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct P2pChallengeHistoryEntry {
    pub kind: String,
    pub epoch_id: u64,
    pub payload_cid: Option<String>,
    pub recipients: usize,
    pub recorded_at_millis: u64,
}

const RECENT_P2P_CHALLENGE_HISTORY_LIMIT: usize = 10;
