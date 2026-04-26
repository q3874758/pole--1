use std::fmt;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::node_aggregator::{
    aggregate_local_epoch, aggregate_record_root, EpochAggregationArtifact, NodeAggregationError,
};
use crate::node_config::NodeConfig;
use crate::node_daemon::{
    build_epoch_commit_from_local_data, epoch_preparation_artifact_path,
    epoch_verification_artifact_path, EpochCommitArtifact, NodeDaemonError,
};
use crate::node_rewards::{
    reward_local_epoch, reward_record_root, EpochRewardArtifact, NodeRewardError,
};
use crate::node_verifier::{verify_local_epoch, EpochVerificationReport, NodeVerificationError};
use crate::primitives::{Amount, EpochId};
use crate::records::EpochCommit;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpochPreparationArtifact {
    pub epoch_id: EpochId,
    pub current_height: u64,
    pub challenge_window_blocks: u32,
    pub challenge_deadline_height: u64,
    pub batch_count: u32,
    pub payload_count: u32,
    pub verification_batch_count: usize,
    pub stored_payload_count: usize,
    pub aggregate_count: usize,
    pub reward_count: usize,
    #[serde(default)]
    pub player_reward_block_count: usize,
    pub total_observation_count: u32,
    pub deduped_observation_count: u32,
    pub accepted_observation_count: u32,
    #[serde(default)]
    pub local_player_reward_total: Amount,
    pub total_gvs_units: Amount,
    pub total_distributed: Amount,
    pub verification_all_valid: bool,
    pub accepted_batches_root_hex: String,
    pub observations_root_hex: String,
    pub availability_root_hex: String,
    pub aggregates_root_hex: String,
    pub rewards_root_hex: String,
    pub randomness_seed_hex: String,
    pub ready_for_submission: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpochPreparationComputation {
    pub artifact: EpochPreparationArtifact,
    pub aggregation_artifact: EpochAggregationArtifact,
    pub reward_artifact: EpochRewardArtifact,
    pub verification_report: EpochVerificationReport,
    pub epoch_commit: EpochCommit,
    pub epoch_commit_artifact: EpochCommitArtifact,
}

#[derive(Debug)]
pub enum NodePreparationError {
    Aggregation(NodeAggregationError),
    Reward(NodeRewardError),
    Verification(NodeVerificationError),
    Daemon(NodeDaemonError),
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl fmt::Display for NodePreparationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Aggregation(err) => write!(f, "aggregation error: {err}"),
            Self::Reward(err) => write!(f, "reward error: {err}"),
            Self::Verification(err) => write!(f, "verification error: {err}"),
            Self::Daemon(err) => write!(f, "daemon error: {err}"),
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::Json(err) => write!(f, "json error: {err}"),
        }
    }
}

impl std::error::Error for NodePreparationError {}

impl From<NodeAggregationError> for NodePreparationError {
    fn from(value: NodeAggregationError) -> Self {
        Self::Aggregation(value)
    }
}

impl From<NodeRewardError> for NodePreparationError {
    fn from(value: NodeRewardError) -> Self {
        Self::Reward(value)
    }
}

impl From<NodeVerificationError> for NodePreparationError {
    fn from(value: NodeVerificationError) -> Self {
        Self::Verification(value)
    }
}

impl From<NodeDaemonError> for NodePreparationError {
    fn from(value: NodeDaemonError) -> Self {
        Self::Daemon(value)
    }
}

impl From<std::io::Error> for NodePreparationError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for NodePreparationError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl EpochPreparationArtifact {
    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), NodePreparationError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }
}

pub fn prepare_local_epoch(
    config: &NodeConfig,
    epoch_id: EpochId,
    current_height: u64,
    challenge_window_blocks: u32,
) -> Result<EpochPreparationArtifact, NodePreparationError> {
    let computation =
        compute_local_epoch_preparation(config, epoch_id, current_height, challenge_window_blocks)?;
    computation
        .artifact
        .save_json(epoch_preparation_artifact_path(config, epoch_id))?;
    Ok(computation.artifact)
}

pub fn compute_local_epoch_preparation(
    config: &NodeConfig,
    epoch_id: EpochId,
    current_height: u64,
    challenge_window_blocks: u32,
) -> Result<EpochPreparationComputation, NodePreparationError> {
    let aggregation_artifact = aggregate_local_epoch(config, epoch_id)?;
    let aggregates_root = aggregate_record_root(
        &aggregation_artifact
            .records
            .iter()
            .map(|record| record.aggregate.clone())
            .collect::<Vec<_>>(),
    )?;

    let reward_artifact = reward_local_epoch(config, epoch_id)?;
    let rewards_root = reward_record_root(
        &reward_artifact
            .records
            .iter()
            .map(|record| record.reward.clone())
            .collect::<Vec<_>>(),
    )?;

    let verification_report = verify_local_epoch(config, epoch_id)?;
    verification_report.save_json(epoch_verification_artifact_path(config, epoch_id))?;

    let (epoch_commit, epoch_commit_artifact) = build_epoch_commit_from_local_data(
        config,
        epoch_id,
        current_height,
        challenge_window_blocks,
        aggregates_root,
        rewards_root,
    )?;

    let ready_for_submission = verification_report.all_valid
        && epoch_commit_artifact.batch_count > 0
        && epoch_commit_artifact.payload_count > 0
        && epoch_commit_artifact.challenge_deadline_height >= current_height;

    let artifact = EpochPreparationArtifact {
        epoch_id,
        current_height,
        challenge_window_blocks,
        challenge_deadline_height: epoch_commit_artifact.challenge_deadline_height,
        batch_count: epoch_commit_artifact.batch_count,
        payload_count: epoch_commit_artifact.payload_count,
        verification_batch_count: verification_report.batch_count,
        stored_payload_count: verification_report.stored_payload_count,
        aggregate_count: aggregation_artifact.aggregate_count,
        reward_count: reward_artifact.reward_count,
        player_reward_block_count: reward_artifact.player_reward_block_count,
        total_observation_count: aggregation_artifact.total_observation_count,
        deduped_observation_count: aggregation_artifact.deduped_observation_count,
        accepted_observation_count: aggregation_artifact.accepted_observation_count,
        local_player_reward_total: reward_artifact.local_player_reward_total,
        total_gvs_units: reward_artifact.total_gvs_units,
        total_distributed: reward_artifact.total_distributed,
        verification_all_valid: verification_report.all_valid,
        accepted_batches_root_hex: epoch_commit_artifact.accepted_batches_root_hex.clone(),
        observations_root_hex: epoch_commit_artifact.observations_root_hex.clone(),
        availability_root_hex: epoch_commit_artifact.availability_root_hex.clone(),
        aggregates_root_hex: epoch_commit_artifact.aggregates_root_hex.clone(),
        rewards_root_hex: epoch_commit_artifact.rewards_root_hex.clone(),
        randomness_seed_hex: epoch_commit_artifact.randomness_seed_hex.clone(),
        ready_for_submission,
    };

    Ok(EpochPreparationComputation {
        artifact,
        aggregation_artifact,
        reward_artifact,
        verification_report,
        epoch_commit,
        epoch_commit_artifact,
    })
}
