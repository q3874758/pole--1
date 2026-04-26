use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::node_config::NodeConfig;
use crate::node_daemon::NodeDaemonError;
use crate::node_gvs::compute_gvs_factors;
use crate::node_pipeline::{merkle_root, stable_hash32};
use crate::primitives::{ActivitySourceKind, AppId, EpochId, Hash32, NodeId, SlotId};
use crate::records::{AggregateRecord, BatchCommit, ObservationRecord};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpochAggregationGroup {
    pub slot_id: SlotId,
    pub app_id: AppId,
    pub total_observations: u32,
    pub unique_collectors: u32,
    pub trimmed_observations: u32,
    pub accepted_observations: Vec<ObservationRecord>,
    pub aggregate: AggregateRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpochAggregationComputation {
    pub artifact: EpochAggregationArtifact,
    pub groups: Vec<EpochAggregationGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpochAggregateEntry {
    pub slot_id: SlotId,
    pub app_id: AppId,
    pub total_observations: u32,
    pub unique_collectors: u32,
    pub trimmed_observations: u32,
    pub aggregate: AggregateRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpochAggregationArtifact {
    pub epoch_id: EpochId,
    pub aggregate_count: usize,
    pub total_observation_count: u32,
    pub deduped_observation_count: u32,
    pub accepted_observation_count: u32,
    pub aggregate_root_hex: String,
    pub records: Vec<EpochAggregateEntry>,
}

#[derive(Debug)]
pub enum NodeAggregationError {
    Upstream(String),
    Io(std::io::Error),
    Json(serde_json::Error),
    Borsh(String),
}

impl fmt::Display for NodeAggregationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Upstream(err) => write!(f, "upstream error: {err}"),
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::Json(err) => write!(f, "json error: {err}"),
            Self::Borsh(err) => write!(f, "borsh error: {err}"),
        }
    }
}

impl std::error::Error for NodeAggregationError {}

impl From<NodeDaemonError> for NodeAggregationError {
    fn from(value: NodeDaemonError) -> Self {
        Self::Upstream(value.to_string())
    }
}

impl From<std::io::Error> for NodeAggregationError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for NodeAggregationError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AggregateInput {
    observation: ObservationRecord,
    batch_commit_hash: Hash32,
}

impl EpochAggregationArtifact {
    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), NodeAggregationError> {
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

pub fn aggregate_local_epoch(
    config: &NodeConfig,
    epoch_id: EpochId,
) -> Result<EpochAggregationArtifact, NodeAggregationError> {
    let computation = compute_local_epoch_aggregation(config, epoch_id)?;
    computation
        .artifact
        .save_json(crate::epoch_aggregation_artifact_path(config, epoch_id))?;
    Ok(computation.artifact)
}

pub fn compute_local_epoch_aggregation(
    config: &NodeConfig,
    epoch_id: EpochId,
) -> Result<EpochAggregationComputation, NodeAggregationError> {
    let batches = crate::load_batches_for_epoch(config, epoch_id)?;
    let mut groups = BTreeMap::<(SlotId, AppId), Vec<AggregateInput>>::new();
    let mut total_observation_count = 0u32;

    for batch in &batches {
        let batch_commit_hash = hash_batch_commit(&batch.batch_commit)?;
        for observation in &batch.observations {
            if observation.epoch_id != epoch_id {
                continue;
            }
            total_observation_count = total_observation_count.saturating_add(1);
            groups
                .entry((observation.slot_id, observation.app_id))
                .or_default()
                .push(AggregateInput {
                    observation: observation.clone(),
                    batch_commit_hash,
                });
        }
    }

    let mut deduped_observation_count = 0u32;
    let mut accepted_observation_count = 0u32;
    let mut group_results = Vec::new();

    for ((slot_id, app_id), inputs) in groups {
        let total_observations = inputs.len() as u32;
        let deduped = deduplicate_by_collector(inputs);
        let unique_collectors = deduped.len() as u32;
        deduped_observation_count = deduped_observation_count.saturating_add(unique_collectors);

        let accepted = trim_extreme_observations(deduped);
        let accepted_observation_count_for_group = accepted.len() as u32;
        let trimmed_observations =
            unique_collectors.saturating_sub(accepted_observation_count_for_group);
        accepted_observation_count =
            accepted_observation_count.saturating_add(accepted_observation_count_for_group);

        let source_batch_root = aggregate_source_batch_root(&accepted);
        let median_players = median_players(&accepted);
        let accepted_observations = accepted
            .iter()
            .map(|input| input.observation.clone())
            .collect::<Vec<_>>();

        let gvs = compute_gvs_factors(config, slot_id, unique_collectors, median_players);
        let primary_source_kind = dominant_source_kind(&accepted_observations);
        let source_confidence_ppm = average_source_confidence_ppm(&accepted_observations);

        let aggregate = AggregateRecord {
            epoch_id,
            slot_id,
            app_id,
            gvs_tier: gvs.tier,
            primary_source_kind,
            source_confidence_ppm,
            accepted_observations: accepted_observation_count_for_group,
            median_players,
            base_glv_microunits: gvs.base_glv_microunits,
            tier_weight_ppm: gvs.tier_weight_ppm,
            time_decay_ppm: gvs.time_decay_ppm,
            coverage_bonus_ppm: gvs.coverage_bonus_ppm,
            gvs_microunits: gvs.gvs_microunits,
            source_batch_root,
        };

        group_results.push(EpochAggregationGroup {
            slot_id,
            app_id,
            total_observations,
            unique_collectors,
            trimmed_observations,
            accepted_observations,
            aggregate,
        });
    }

    let aggregate_root = aggregate_record_root(
        &group_results
            .iter()
            .map(|group| group.aggregate.clone())
            .collect::<Vec<_>>(),
    )?;
    let artifact = EpochAggregationArtifact {
        epoch_id,
        aggregate_count: group_results.len(),
        total_observation_count,
        deduped_observation_count,
        accepted_observation_count,
        aggregate_root_hex: crate::hex_32(aggregate_root),
        records: group_results
            .iter()
            .map(|group| EpochAggregateEntry {
                slot_id: group.slot_id,
                app_id: group.app_id,
                total_observations: group.total_observations,
                unique_collectors: group.unique_collectors,
                trimmed_observations: group.trimmed_observations,
                aggregate: group.aggregate.clone(),
            })
            .collect(),
    };
    Ok(EpochAggregationComputation {
        artifact,
        groups: group_results,
    })
}

pub fn aggregate_record_root(records: &[AggregateRecord]) -> Result<Hash32, NodeAggregationError> {
    let leaf_hashes = records
        .iter()
        .map(hash_aggregate_record)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(merkle_root(&leaf_hashes))
}

fn hash_aggregate_record(record: &AggregateRecord) -> Result<Hash32, NodeAggregationError> {
    let encoded =
        borsh::to_vec(record).map_err(|err| NodeAggregationError::Borsh(err.to_string()))?;
    Ok(stable_hash32(&encoded))
}

fn hash_batch_commit(batch_commit: &BatchCommit) -> Result<Hash32, NodeAggregationError> {
    let encoded =
        borsh::to_vec(batch_commit).map_err(|err| NodeAggregationError::Borsh(err.to_string()))?;
    Ok(stable_hash32(&encoded))
}

fn deduplicate_by_collector(inputs: Vec<AggregateInput>) -> Vec<AggregateInput> {
    let mut selected = BTreeMap::<NodeId, AggregateInput>::new();
    for input in inputs {
        match selected.get(&input.observation.collector_id) {
            Some(existing) if !prefers_candidate(&input, existing) => {}
            _ => {
                selected.insert(input.observation.collector_id, input);
            }
        }
    }
    selected.into_values().collect()
}

fn prefers_candidate(candidate: &AggregateInput, current: &AggregateInput) -> bool {
    candidate_rank(candidate) < candidate_rank(current)
}

fn candidate_rank(input: &AggregateInput) -> (u64, u64, Hash32, Hash32, Vec<u8>) {
    (
        input.observation.observed_at_millis,
        input.observation.observed_players,
        input.observation.raw_body_hash,
        input.batch_commit_hash,
        input.observation.collector_signature.clone(),
    )
}

fn trim_extreme_observations(mut inputs: Vec<AggregateInput>) -> Vec<AggregateInput> {
    if inputs.len() < 5 {
        return inputs;
    }

    inputs.sort_by_key(trimmed_rank);
    inputs[1..inputs.len() - 1].to_vec()
}

fn trimmed_rank(input: &AggregateInput) -> (u64, NodeId, u64, Hash32, Hash32) {
    (
        input.observation.observed_players,
        input.observation.collector_id,
        input.observation.observed_at_millis,
        input.observation.raw_body_hash,
        input.batch_commit_hash,
    )
}

fn aggregate_source_batch_root(inputs: &[AggregateInput]) -> Hash32 {
    let leaves = inputs
        .iter()
        .map(|input| input.batch_commit_hash)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    merkle_root(&leaves)
}

fn dominant_source_kind(observations: &[ObservationRecord]) -> ActivitySourceKind {
    let mut counts = BTreeMap::<ActivitySourceKind, usize>::new();
    for observation in observations {
        *counts.entry(observation.source_kind).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(source_kind, count)| (*count, *source_kind))
        .map(|(source_kind, _)| source_kind)
        .unwrap_or(ActivitySourceKind::Steam)
}

fn average_source_confidence_ppm(observations: &[ObservationRecord]) -> u32 {
    if observations.is_empty() {
        return 1_000_000;
    }

    let total = observations
        .iter()
        .map(|observation| u64::from(observation.source_confidence_ppm))
        .sum::<u64>();
    (total / observations.len() as u64) as u32
}

fn median_players(inputs: &[AggregateInput]) -> u64 {
    if inputs.is_empty() {
        return 0;
    }

    let mut players = inputs
        .iter()
        .map(|input| input.observation.observed_players)
        .collect::<Vec<_>>();
    players.sort_unstable();

    let middle = players.len() / 2;
    if players.len() % 2 == 1 {
        players[middle]
    } else {
        players[middle - 1].saturating_add(players[middle]) / 2
    }
}
