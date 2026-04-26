use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::node_aggregator::{
    compute_local_epoch_aggregation, EpochAggregationComputation, NodeAggregationError,
};
use crate::node_config::{NodeConfig, NodeConfigError, RewardSourceMode};
use crate::node_daemon::local_chain_store_path;
use crate::node_pipeline::SteamCurrentPlayersSample;
use crate::params::ProtocolParams;
use crate::primitives::{Amount, EpochId, Hash32, NodeId};
use crate::records::{GovernanceProposalState, RewardRecord};
use crate::storage_book::{LocalRetentionBook, StorageBookError};
use crate::store::{PersistentStoreStub, ProtocolStore};
use crate::tokenomics::base_service_reward_per_block;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpochRewardEntry {
    pub node_id_hex: String,
    pub player_block_count: usize,
    pub player_weight_units: Amount,
    pub player_reward_units: Amount,
    pub collect_score_units: Amount,
    pub storage_score_units: Amount,
    pub reward: RewardRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpochRewardArtifact {
    pub epoch_id: EpochId,
    pub reward_block_secs: u64,
    pub player_block_reward: Amount,
    #[serde(default)]
    pub reward_adjustment_period_blocks: u64,
    #[serde(default)]
    pub target_network_weight_units: Amount,
    #[serde(default)]
    pub reward_adjustment_cap_bps: u16,
    pub player_reward_block_count: usize,
    pub player_reward_pool: Amount,
    pub local_player_weight_units: Amount,
    pub total_network_weight_units: Amount,
    pub local_player_reward_total: Amount,
    pub total_gvs_units: Amount,
    pub collect_pool: Amount,
    pub store_pool: Amount,
    pub verify_pool: Amount,
    pub propose_pool: Amount,
    pub total_distributed: Amount,
    pub reward_count: usize,
    pub reward_root_hex: String,
    pub records: Vec<EpochRewardEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpochRewardComputation {
    pub artifact: EpochRewardArtifact,
    pub records: Vec<RewardRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PendingPlayerRewardEntry {
    pub process_name: String,
    pub app_id: u32,
    pub game_coefficient_ppm: u32,
    pub play_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PendingPlayerRewardBlockState {
    pub sampled_interval_secs: u64,
    pub total_network_weight_units: Amount,
    #[serde(default)]
    pub total_adjustment_cycle_network_weight_units: Amount,
    pub fixed_block_reward: Amount,
    #[serde(default)]
    pub fixed_player_reward: Amount,
    pub reward_adjustment_period_index: u64,
    #[serde(default)]
    pub adjustment_cycle_index: u64,
    pub entries: Vec<PendingPlayerRewardEntry>,
}

impl PendingPlayerRewardBlockState {
    pub fn normalize_whitepaper_fields(&mut self) {
        if self.total_adjustment_cycle_network_weight_units == 0 {
            self.total_adjustment_cycle_network_weight_units = self.total_network_weight_units;
        }
        if self.fixed_player_reward == 0 {
            self.fixed_player_reward = self.fixed_block_reward;
        }
        if self.adjustment_cycle_index == 0 {
            self.adjustment_cycle_index = self.reward_adjustment_period_index;
        }

        self.total_network_weight_units = self.total_adjustment_cycle_network_weight_units;
        self.fixed_block_reward = self.fixed_player_reward;
        self.reward_adjustment_period_index = self.adjustment_cycle_index;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RewardTickContext {
    pub epoch_id: EpochId,
    pub slot_id: u64,
    pub sampled_interval_secs: u64,
    pub fixed_block_reward: Amount,
    pub fixed_player_reward: Amount,
    pub reward_adjustment_period_index: u64,
    pub adjustment_cycle_index: u64,
    pub fixed_block_reward_basis_period_index: u64,
    pub fixed_player_reward_basis_cycle_index: u64,
    pub fixed_block_reward_basis_network_weight_units: Amount,
    pub fixed_player_reward_basis_total_network_weight_units: Amount,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerRewardBlockRecord {
    pub epoch_id: EpochId,
    pub slot_id: u64,
    pub block_index: u32,
    pub process_name: String,
    pub app_id: u32,
    pub play_seconds: u64,
    pub reward_block_secs: u64,
    pub sampled_interval_secs: u64,
    pub game_coefficient_ppm: u32,
    pub observed_network_players: u64,
    pub player_weight_units: Amount,
    pub total_network_weight_units: Amount,
    #[serde(default)]
    pub reward_adjustment_period_index: u64,
    #[serde(default)]
    pub adjustment_cycle_index: u64,
    #[serde(default)]
    pub reward_adjustment_period_blocks: u64,
    #[serde(default)]
    pub adjustment_cycle_blocks: u64,
    #[serde(default)]
    pub target_network_weight_units: Amount,
    #[serde(default)]
    pub target_total_network_weight_units: Amount,
    #[serde(default)]
    pub reward_adjustment_cap_bps: u16,
    #[serde(default)]
    pub fixed_block_reward_basis_period_index: u64,
    #[serde(default)]
    pub fixed_player_reward_basis_cycle_index: u64,
    #[serde(default)]
    pub fixed_block_reward_basis_network_weight_units: Amount,
    #[serde(default)]
    pub fixed_player_reward_basis_total_network_weight_units: Amount,
    pub block_reward: Amount,
    #[serde(default)]
    pub fixed_player_reward: Amount,
    pub player_reward: Amount,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerRewardTickArtifact {
    pub epoch_id: EpochId,
    pub slot_id: u64,
    pub sampled_interval_secs: u64,
    pub reward_block_secs: u64,
    #[serde(default)]
    pub reward_adjustment_period_index: u64,
    #[serde(default)]
    pub adjustment_cycle_index: u64,
    #[serde(default)]
    pub reward_adjustment_period_blocks: u64,
    #[serde(default)]
    pub adjustment_cycle_blocks: u64,
    #[serde(default)]
    pub completed_reward_block_count: usize,
    #[serde(default)]
    pub fixed_block_reward_basis_period_index: u64,
    #[serde(default)]
    pub fixed_player_reward_basis_cycle_index: u64,
    #[serde(default)]
    pub fixed_block_reward_basis_network_weight_units: Amount,
    #[serde(default)]
    pub fixed_player_reward_basis_total_network_weight_units: Amount,
    pub block_reward: Amount,
    #[serde(default)]
    pub fixed_player_reward: Amount,
    pub foreground_process: Option<String>,
    pub active_game_processes: Vec<String>,
    pub records: Vec<PlayerRewardBlockRecord>,
    pub total_player_reward: Amount,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerRewardTickComputation {
    pub artifact: PlayerRewardTickArtifact,
    pub pending_state: Option<PendingPlayerRewardBlockState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ServiceRewardPools {
    collect_pool: Amount,
    store_pool: Amount,
    verify_pool: Amount,
    propose_pool: Amount,
}

#[derive(Debug)]
pub enum NodeRewardError {
    Config(NodeConfigError),
    Aggregation(NodeAggregationError),
    Storage(StorageBookError),
    Io(std::io::Error),
    Json(serde_json::Error),
    Borsh(String),
}

impl fmt::Display for NodeRewardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(err) => write!(f, "config error: {err}"),
            Self::Aggregation(err) => write!(f, "aggregation error: {err}"),
            Self::Storage(err) => write!(f, "storage error: {err}"),
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::Json(err) => write!(f, "json error: {err}"),
            Self::Borsh(err) => write!(f, "borsh error: {err}"),
        }
    }
}

impl std::error::Error for NodeRewardError {}

impl From<NodeAggregationError> for NodeRewardError {
    fn from(value: NodeAggregationError) -> Self {
        Self::Aggregation(value)
    }
}

impl From<NodeConfigError> for NodeRewardError {
    fn from(value: NodeConfigError) -> Self {
        Self::Config(value)
    }
}

impl From<StorageBookError> for NodeRewardError {
    fn from(value: StorageBookError) -> Self {
        Self::Storage(value)
    }
}

impl From<std::io::Error> for NodeRewardError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for NodeRewardError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl EpochRewardArtifact {
    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), NodeRewardError> {
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

impl PlayerRewardBlockRecord {
    fn normalize_whitepaper_fields(&mut self) {
        if self.adjustment_cycle_index == 0 {
            self.adjustment_cycle_index = self.reward_adjustment_period_index;
        }
        if self.adjustment_cycle_blocks == 0 {
            self.adjustment_cycle_blocks = self.reward_adjustment_period_blocks;
        }
        if self.target_total_network_weight_units == 0 {
            self.target_total_network_weight_units = self.target_network_weight_units;
        }
        if self.fixed_player_reward_basis_cycle_index == 0 {
            self.fixed_player_reward_basis_cycle_index = self.fixed_block_reward_basis_period_index;
        }
        if self.fixed_player_reward_basis_total_network_weight_units == 0 {
            self.fixed_player_reward_basis_total_network_weight_units =
                self.fixed_block_reward_basis_network_weight_units;
        }
        if self.fixed_player_reward == 0 {
            self.fixed_player_reward = self.block_reward;
        }

        self.reward_adjustment_period_index = self.adjustment_cycle_index;
        self.reward_adjustment_period_blocks = self.adjustment_cycle_blocks;
        self.target_network_weight_units = self.target_total_network_weight_units;
        self.fixed_block_reward_basis_period_index = self.fixed_player_reward_basis_cycle_index;
        self.fixed_block_reward_basis_network_weight_units =
            self.fixed_player_reward_basis_total_network_weight_units;
        self.block_reward = self.fixed_player_reward;
    }
}

impl PlayerRewardTickArtifact {
    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), NodeRewardError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let mut normalized = self.clone();
        normalized.normalize_whitepaper_fields();
        let content = serde_json::to_string_pretty(&normalized)?;
        fs::write(path, content)?;
        Ok(())
    }

    fn normalize_whitepaper_fields(&mut self) {
        if self.adjustment_cycle_index == 0 {
            self.adjustment_cycle_index = self.reward_adjustment_period_index;
        }
        if self.adjustment_cycle_blocks == 0 {
            self.adjustment_cycle_blocks = self.reward_adjustment_period_blocks;
        }
        if self.fixed_player_reward_basis_cycle_index == 0 {
            self.fixed_player_reward_basis_cycle_index = self.fixed_block_reward_basis_period_index;
        }
        if self.fixed_player_reward_basis_total_network_weight_units == 0 {
            self.fixed_player_reward_basis_total_network_weight_units =
                self.fixed_block_reward_basis_network_weight_units;
        }
        if self.fixed_player_reward == 0 {
            self.fixed_player_reward = self.block_reward;
        }

        self.reward_adjustment_period_index = self.adjustment_cycle_index;
        self.reward_adjustment_period_blocks = self.adjustment_cycle_blocks;
        self.fixed_block_reward_basis_period_index = self.fixed_player_reward_basis_cycle_index;
        self.fixed_block_reward_basis_network_weight_units =
            self.fixed_player_reward_basis_total_network_weight_units;
        self.block_reward = self.fixed_player_reward;

        for record in &mut self.records {
            record.normalize_whitepaper_fields();
        }
    }
}

pub fn reward_local_epoch(
    config: &NodeConfig,
    epoch_id: EpochId,
) -> Result<EpochRewardArtifact, NodeRewardError> {
    let computation = compute_local_epoch_rewards(config, epoch_id)?;
    computation
        .artifact
        .save_json(crate::epoch_reward_artifact_path(config, epoch_id))?;
    Ok(computation.artifact)
}

pub fn compute_local_epoch_rewards(
    config: &NodeConfig,
    epoch_id: EpochId,
) -> Result<EpochRewardComputation, NodeRewardError> {
    let aggregation = compute_local_epoch_aggregation(config, epoch_id)?;
    let retention_book = LocalRetentionBook::load_or_default_json(
        crate::retention_book_path(config),
        config.storage.quota_gb,
    )?;
    let player_ticks = load_player_reward_ticks_for_epoch(config, epoch_id)?;
    let player_block_count = player_ticks
        .iter()
        .map(|tick| tick.completed_reward_block_count)
        .sum::<usize>();
    let player_reward_pool = player_ticks
        .iter()
        .flat_map(|tick| tick.records.iter())
        .map(|record| record.block_reward)
        .sum::<Amount>();
    let local_player_weight_units = player_ticks
        .iter()
        .flat_map(|tick| tick.records.iter())
        .map(|record| record.player_weight_units)
        .sum::<Amount>();
    let total_network_weight_units = player_ticks
        .iter()
        .flat_map(|tick| tick.records.iter())
        .map(|record| record.total_network_weight_units)
        .sum::<Amount>();
    let local_player_reward_total = player_ticks
        .iter()
        .map(|tick| tick.total_player_reward)
        .sum::<Amount>();

    let total_gvs_units = aggregation
        .groups
        .iter()
        .map(|group| reward_units_from_gvs(group.aggregate.gvs_microunits))
        .sum::<Amount>();

    let service_reward_pools = service_reward_pools_for_epoch(
        config,
        player_block_count,
        total_gvs_units,
        retention_book
            .payloads
            .values()
            .any(|record| record.epoch_id == epoch_id),
        !aggregation.groups.is_empty(),
    );

    let mut collect_scores = BTreeMap::<NodeId, Amount>::new();
    collect_collect_scores(&aggregation, &mut collect_scores);

    let mut storage_scores = BTreeMap::<NodeId, Amount>::new();
    for record in retention_book
        .payloads
        .values()
        .filter(|record| record.epoch_id == epoch_id)
    {
        *storage_scores.entry(record.receipt.storer_id).or_default() +=
            Amount::from(record.size_bytes.max(1));
    }

    let collect_rewards = allocate_proportional(&collect_scores, service_reward_pools.collect_pool);
    let store_rewards = allocate_proportional(&storage_scores, service_reward_pools.store_pool);

    let mut reward_map = BTreeMap::<NodeId, RewardRecord>::new();
    merge_reward_component(
        &mut reward_map,
        epoch_id,
        &collect_rewards,
        RewardComponent::Collect,
    );
    merge_reward_component(
        &mut reward_map,
        epoch_id,
        &store_rewards,
        RewardComponent::Store,
    );

    let local_node_id = config.node_id()?;
    if local_player_reward_total > 0 {
        reward_map
            .entry(local_node_id)
            .or_insert_with(|| empty_reward_record(epoch_id, local_node_id))
            .player_reward += local_player_reward_total;
    }
    if service_reward_pools.verify_pool > 0 {
        reward_map
            .entry(local_node_id)
            .or_insert_with(|| empty_reward_record(epoch_id, local_node_id))
            .verify_reward += service_reward_pools.verify_pool;
    }
    if service_reward_pools.propose_pool > 0 {
        reward_map
            .entry(local_node_id)
            .or_insert_with(|| empty_reward_record(epoch_id, local_node_id))
            .propose_reward += service_reward_pools.propose_pool;
    }

    let mut records = reward_map.into_values().collect::<Vec<_>>();
    records.sort_by_key(|record| record.node_id);
    for record in &mut records {
        record.net_reward = record
            .player_reward
            .saturating_add(record.collect_reward)
            .saturating_add(record.store_reward)
            .saturating_add(record.verify_reward)
            .saturating_add(record.propose_reward)
            .saturating_sub(record.slash_debit);
    }

    let reward_root = reward_record_root(&records)?;
    let local_player_block_count = player_ticks
        .iter()
        .map(|tick| tick.completed_reward_block_count)
        .sum::<usize>();
    let artifact = EpochRewardArtifact {
        epoch_id,
        reward_block_secs: effective_reward_block_secs(config),
        player_block_reward: effective_player_block_reward(config),
        reward_adjustment_period_blocks: config.reward.reward_adjustment_period_blocks,
        target_network_weight_units: effective_target_network_weight_units(config),
        reward_adjustment_cap_bps: effective_reward_adjustment_cap_bps(config),
        player_reward_block_count: player_block_count,
        player_reward_pool,
        local_player_weight_units,
        total_network_weight_units,
        local_player_reward_total,
        total_gvs_units,
        collect_pool: service_reward_pools.collect_pool,
        store_pool: service_reward_pools.store_pool,
        verify_pool: service_reward_pools.verify_pool,
        propose_pool: service_reward_pools.propose_pool,
        total_distributed: records.iter().map(|record| record.net_reward).sum(),
        reward_count: records.len(),
        reward_root_hex: crate::hex_32(reward_root),
        records: records
            .iter()
            .map(|record| EpochRewardEntry {
                node_id_hex: crate::hex_32(record.node_id),
                player_block_count: if record.node_id == local_node_id {
                    local_player_block_count
                } else {
                    0
                },
                player_weight_units: if record.node_id == local_node_id {
                    local_player_weight_units
                } else {
                    0
                },
                player_reward_units: if record.node_id == local_node_id {
                    local_player_reward_total
                } else {
                    0
                },
                collect_score_units: *collect_scores.get(&record.node_id).unwrap_or(&0),
                storage_score_units: *storage_scores.get(&record.node_id).unwrap_or(&0),
                reward: record.clone(),
            })
            .collect(),
    };

    Ok(EpochRewardComputation { artifact, records })
}

pub fn record_player_reward_tick(
    config: &NodeConfig,
    context: RewardTickContext,
    pending_state: Option<PendingPlayerRewardBlockState>,
    foreground_process: Option<&str>,
    active_game_processes: &[String],
    samples: &[SteamCurrentPlayersSample],
) -> Result<PlayerRewardTickComputation, NodeRewardError> {
    let reward_block_secs = effective_reward_block_secs(config).max(1);
    let sampled_interval_secs = context.sampled_interval_secs.max(1);
    let active_mapping =
        select_active_game_mapping(config, foreground_process, active_game_processes);

    let mut pending_state = pending_state.unwrap_or_default();
    pending_state.normalize_whitepaper_fields();
    if pending_state.fixed_block_reward == 0 || pending_state.sampled_interval_secs == 0 {
        pending_state.fixed_block_reward = context.fixed_block_reward;
        pending_state.fixed_player_reward = context.fixed_player_reward;
        pending_state.reward_adjustment_period_index = context.reward_adjustment_period_index;
        pending_state.adjustment_cycle_index = context.adjustment_cycle_index;
    }

    let mut completed_records = Vec::new();
    let mut completed_reward_block_count = 0usize;
    let mut remaining_secs = sampled_interval_secs;
    let mut next_block_index = 0u32;

    while remaining_secs > 0 {
        let needed_secs = reward_block_secs.saturating_sub(pending_state.sampled_interval_secs);
        let chunk_secs = remaining_secs.min(needed_secs.max(1));
        pending_state.sampled_interval_secs = pending_state
            .sampled_interval_secs
            .saturating_add(chunk_secs);
        pending_state.total_network_weight_units =
            pending_state.total_network_weight_units.saturating_add(
                network_weight_units_for_duration(config, samples, chunk_secs),
            );
        pending_state.total_adjustment_cycle_network_weight_units =
            pending_state.total_network_weight_units;

        if let Some((process_name, app_id, game_coefficient_ppm)) = &active_mapping {
            if let Some(entry) = pending_state.entries.iter_mut().find(|entry| {
                entry.app_id == *app_id
                    && entry.game_coefficient_ppm == *game_coefficient_ppm
                    && entry.process_name == *process_name
            }) {
                entry.play_seconds = entry.play_seconds.saturating_add(chunk_secs);
            } else {
                pending_state.entries.push(PendingPlayerRewardEntry {
                    process_name: process_name.clone(),
                    app_id: *app_id,
                    game_coefficient_ppm: *game_coefficient_ppm,
                    play_seconds: chunk_secs,
                });
            }
        }

        remaining_secs = remaining_secs.saturating_sub(chunk_secs);
        if pending_state.sampled_interval_secs < reward_block_secs {
            continue;
        }

        pending_state.entries.sort_by(|left, right| {
            (&left.process_name, left.app_id, left.game_coefficient_ppm).cmp(&(
                &right.process_name,
                right.app_id,
                right.game_coefficient_ppm,
            ))
        });

        for entry in &pending_state.entries {
            let player_weight_units = Amount::from(entry.play_seconds)
                .saturating_mul(Amount::from(entry.game_coefficient_ppm));
            let player_reward = proportional_amount(
                pending_state.fixed_block_reward,
                player_weight_units,
                pending_state.total_network_weight_units,
            );
            let observed_network_players = samples
                .iter()
                .find(|sample| sample.app_id == entry.app_id)
                .map(|sample| sample.observed_players)
                .unwrap_or(0);
            completed_records.push(PlayerRewardBlockRecord {
                epoch_id: context.epoch_id,
                slot_id: context.slot_id,
                block_index: next_block_index,
                process_name: entry.process_name.clone(),
                app_id: entry.app_id,
                play_seconds: entry.play_seconds,
                reward_block_secs,
                sampled_interval_secs: reward_block_secs,
                game_coefficient_ppm: entry.game_coefficient_ppm,
                observed_network_players,
                player_weight_units,
                total_network_weight_units: pending_state.total_network_weight_units,
                adjustment_cycle_index: pending_state.reward_adjustment_period_index,
                adjustment_cycle_blocks: config.reward.reward_adjustment_period_blocks,
                target_total_network_weight_units: effective_target_network_weight_units(config),
                reward_adjustment_cap_bps: effective_reward_adjustment_cap_bps(config),
                fixed_player_reward_basis_cycle_index: context
                    .fixed_player_reward_basis_cycle_index,
                fixed_player_reward_basis_total_network_weight_units: context
                    .fixed_player_reward_basis_total_network_weight_units,
                fixed_player_reward: pending_state.fixed_block_reward,
                reward_adjustment_period_index: 0,
                reward_adjustment_period_blocks: 0,
                target_network_weight_units: 0,
                fixed_block_reward_basis_period_index: 0,
                fixed_block_reward_basis_network_weight_units: 0,
                block_reward: 0,
                player_reward,
            });
        }

        completed_reward_block_count += 1;
        next_block_index += 1;
        pending_state = PendingPlayerRewardBlockState::default();
        if remaining_secs > 0 {
            pending_state.fixed_block_reward = context.fixed_block_reward;
            pending_state.fixed_player_reward = context.fixed_player_reward;
            pending_state.reward_adjustment_period_index = context.reward_adjustment_period_index;
            pending_state.adjustment_cycle_index = context.adjustment_cycle_index;
            pending_state.normalize_whitepaper_fields();
        }
    }

    let mut artifact = PlayerRewardTickArtifact {
        epoch_id: context.epoch_id,
        slot_id: context.slot_id,
        sampled_interval_secs,
        reward_block_secs,
        adjustment_cycle_index: context.adjustment_cycle_index,
        adjustment_cycle_blocks: config.reward.reward_adjustment_period_blocks,
        completed_reward_block_count,
        fixed_player_reward_basis_cycle_index: context.fixed_player_reward_basis_cycle_index,
        fixed_player_reward_basis_total_network_weight_units: context
            .fixed_player_reward_basis_total_network_weight_units,
        fixed_player_reward: completed_records
            .first()
            .map(|record| record.fixed_player_reward)
            .unwrap_or_else(|| {
                pending_state
                    .fixed_block_reward
                    .max(context.fixed_player_reward)
            }),
        reward_adjustment_period_index: 0,
        reward_adjustment_period_blocks: 0,
        fixed_block_reward_basis_period_index: 0,
        fixed_block_reward_basis_network_weight_units: 0,
        block_reward: 0,
        foreground_process: foreground_process.map(str::to_string),
        active_game_processes: active_game_processes.to_vec(),
        total_player_reward: completed_records
            .iter()
            .map(|record| record.player_reward)
            .sum(),
        records: completed_records,
    };
    artifact.normalize_whitepaper_fields();
    artifact.save_json(player_reward_tick_artifact_path(
        config,
        context.epoch_id,
        context.slot_id,
    ))?;
    Ok(PlayerRewardTickComputation {
        artifact,
        pending_state: (pending_state.sampled_interval_secs > 0).then_some(pending_state),
    })
}

pub fn reward_record_root(records: &[RewardRecord]) -> Result<Hash32, NodeRewardError> {
    let leaf_hashes = records
        .iter()
        .map(|record| {
            borsh::to_vec(record)
                .map(|encoded| crate::stable_hash32(&encoded))
                .map_err(|err: std::io::Error| NodeRewardError::Borsh(err.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(crate::merkle_root(&leaf_hashes))
}

fn collect_collect_scores(
    aggregation: &EpochAggregationComputation,
    scores: &mut BTreeMap<NodeId, Amount>,
) {
    for group in &aggregation.groups {
        if group.accepted_observations.is_empty() {
            continue;
        }
        let group_units = reward_units_from_gvs(group.aggregate.gvs_microunits);
        if group_units == 0 {
            continue;
        }
        let per_observation_weight =
            (group_units / group.accepted_observations.len() as Amount).max(1);
        for observation in &group.accepted_observations {
            *scores.entry(observation.collector_id).or_default() += per_observation_weight;
        }
    }
}

fn reward_units_from_gvs(gvs_microunits: u64) -> Amount {
    Amount::from(gvs_microunits / 1_000_000)
}

fn allocate_proportional(
    weights: &BTreeMap<NodeId, Amount>,
    pool: Amount,
) -> BTreeMap<NodeId, Amount> {
    if pool == 0 || weights.is_empty() {
        return BTreeMap::new();
    }

    let total_weight = weights.values().copied().sum::<Amount>();
    if total_weight == 0 {
        return BTreeMap::new();
    }

    let mut allocations = BTreeMap::new();
    let mut remainders = Vec::new();
    let mut distributed = 0u128;

    for (node_id, weight) in weights {
        let numerator = pool.saturating_mul(*weight);
        let share = numerator / total_weight;
        let remainder = numerator % total_weight;
        allocations.insert(*node_id, share);
        distributed = distributed.saturating_add(share);
        remainders.push((remainder, *node_id));
    }

    remainders.sort_by(|left, right| right.cmp(left));
    let mut leftover = pool.saturating_sub(distributed);
    let mut index = 0usize;
    while leftover > 0 && !remainders.is_empty() {
        let node_id = remainders[index % remainders.len()].1;
        *allocations.entry(node_id).or_default() += 1;
        leftover -= 1;
        index += 1;
    }

    allocations
}

fn proportional_amount(pool: Amount, weight: Amount, total_weight: Amount) -> Amount {
    if pool == 0 || weight == 0 || total_weight == 0 {
        return 0;
    }
    pool.saturating_mul(weight) / total_weight
}

fn service_reward_pools_for_epoch(
    config: &NodeConfig,
    player_block_count: usize,
    total_gvs_units: Amount,
    has_retention_records: bool,
    has_aggregation_groups: bool,
) -> ServiceRewardPools {
    match config.reward.reward_source {
        RewardSourceMode::Static => ServiceRewardPools {
            collect_pool: total_gvs_units,
            store_pool: if total_gvs_units > 0 && has_retention_records {
                (total_gvs_units / 5).max(1)
            } else {
                0
            },
            verify_pool: if total_gvs_units > 0
                && config.capabilities.verify
                && has_aggregation_groups
            {
                (total_gvs_units / 10).max(1)
            } else {
                0
            },
            propose_pool: if total_gvs_units > 0
                && config.capabilities.propose
                && has_aggregation_groups
            {
                (total_gvs_units / 20).max(1)
            } else {
                0
            },
        },
        RewardSourceMode::Tokenomics => {
            let base_service_pool = base_service_reward_per_block(
                config.reward.emission_year,
                config.reward.reward_block_secs,
            )
            .saturating_mul(player_block_count as Amount);
            let collect_pool =
                proportional_service_pool(base_service_pool, config.reward.collect_reward_bps);
            let store_pool = if has_retention_records {
                proportional_service_pool(base_service_pool, config.reward.store_reward_bps)
            } else {
                0
            };
            let verify_pool = if config.capabilities.verify && has_aggregation_groups {
                proportional_service_pool(base_service_pool, config.reward.verify_reward_bps)
            } else {
                0
            };
            let propose_pool = if config.capabilities.propose && has_aggregation_groups {
                proportional_service_pool(base_service_pool, config.reward.propose_reward_bps)
            } else {
                0
            };
            ServiceRewardPools {
                collect_pool,
                store_pool,
                verify_pool,
                propose_pool,
            }
        }
    }
}

fn proportional_service_pool(base_service_pool: Amount, bps: u16) -> Amount {
    if base_service_pool == 0 || bps == 0 {
        return 0;
    }
    (base_service_pool.saturating_mul(Amount::from(bps)) / 10_000).max(1)
}

pub fn adjusted_player_block_reward(
    base_block_reward: Amount,
    target_network_weight_units: Amount,
    current_network_weight_units: Amount,
    reward_adjustment_cap_bps: u16,
) -> Amount {
    if base_block_reward == 0 {
        return 0;
    }
    if target_network_weight_units == 0 || current_network_weight_units == 0 {
        return base_block_reward;
    }

    let cap_bps = reward_adjustment_cap_bps.min(10_000) as Amount;
    let lower_bound = base_block_reward.saturating_mul(10_000u128.saturating_sub(cap_bps)) / 10_000;
    let upper_bound = base_block_reward.saturating_mul(10_000u128.saturating_add(cap_bps)) / 10_000;

    let scaled_ratio = target_network_weight_units.saturating_mul(1_000_000_000_000u128)
        / current_network_weight_units;
    let ratio_sqrt = integer_sqrt(scaled_ratio);
    let adjusted = base_block_reward.saturating_mul(ratio_sqrt) / 1_000_000;
    adjusted.clamp(lower_bound, upper_bound).max(1)
}

pub fn current_network_weight_units_for_block(
    config: &NodeConfig,
    samples: &[SteamCurrentPlayersSample],
) -> Amount {
    network_weight_units_for_duration(config, samples, effective_reward_block_secs(config).max(1))
}

pub fn effective_reward_block_secs(config: &NodeConfig) -> u64 {
    latest_activated_protocol_params(config)
        .map(|params| params.rewards.reward_block_secs)
        .unwrap_or(config.reward.reward_block_secs)
}

pub fn effective_player_block_reward(config: &NodeConfig) -> Amount {
    latest_activated_protocol_params(config)
        .map(|params| params.rewards.effective_player_block_reward)
        .unwrap_or_else(|| config.reward.base_player_block_reward())
}

pub fn effective_target_network_weight_units(config: &NodeConfig) -> Amount {
    latest_activated_protocol_params(config)
        .map(|params| params.rewards.target_network_weight_units)
        .unwrap_or(config.reward.target_network_weight_units)
}

pub fn effective_reward_adjustment_cap_bps(config: &NodeConfig) -> u16 {
    latest_activated_protocol_params(config)
        .map(|params| params.rewards.reward_adjustment_cap_bps)
        .unwrap_or(config.reward.reward_adjustment_cap_bps)
}

pub fn effective_challenge_window_blocks(config: &NodeConfig) -> u32 {
    latest_activated_protocol_params(config)
        .map(|params| params.challenge_window_blocks)
        .unwrap_or(config.runtime.challenge_window_blocks)
}

pub fn effective_min_retention_epochs(config: &NodeConfig) -> u32 {
    latest_activated_protocol_params(config)
        .map(|params| params.min_retention_epochs)
        .unwrap_or(config.storage.retention_epochs)
}

fn integer_sqrt(value: Amount) -> Amount {
    if value < 2 {
        return value;
    }

    let mut x0 = value;
    let mut x1 = (x0 + value / x0) / 2;
    while x1 < x0 {
        x0 = x1;
        x1 = (x0 + value / x0) / 2;
    }
    x0
}

fn network_weight_units_for_duration(
    config: &NodeConfig,
    samples: &[SteamCurrentPlayersSample],
    duration_secs: u64,
) -> Amount {
    samples
        .iter()
        .map(|sample| {
            Amount::from(sample.observed_players)
                .saturating_mul(Amount::from(duration_secs))
                .saturating_mul(Amount::from(game_coefficient_ppm_for_app(
                    config,
                    sample.app_id,
                )))
        })
        .sum()
}

fn game_coefficient_ppm_for_app(config: &NodeConfig, app_id: u32) -> u32 {
    if let Some(override_ppm) = activated_protocol_game_coefficient_ppm_for_app(config, app_id) {
        return override_ppm;
    }
    config
        .reward
        .game_mappings
        .iter()
        .filter(|mapping| mapping.app_id == app_id)
        .map(|mapping| mapping.game_coefficient_ppm)
        .max()
        .unwrap_or(1_000_000)
}

fn select_active_game_mapping(
    config: &NodeConfig,
    foreground_process: Option<&str>,
    active_game_processes: &[String],
) -> Option<(String, u32, u32)> {
    let foreground = foreground_process.and_then(|process_name| {
        find_game_mapping(config, process_name).map(|mapping| {
            (
                canonical_process_name(&mapping.process_name),
                mapping.app_id,
                game_coefficient_ppm_for_app(config, mapping.app_id),
            )
        })
    });
    if foreground.is_some() {
        return foreground;
    }

    active_game_processes.iter().find_map(|process_name| {
        find_game_mapping(config, process_name).map(|mapping| {
            (
                canonical_process_name(&mapping.process_name),
                mapping.app_id,
                game_coefficient_ppm_for_app(config, mapping.app_id),
            )
        })
    })
}

fn activated_protocol_game_coefficient_ppm_for_app(
    config: &NodeConfig,
    app_id: u32,
) -> Option<u32> {
    let params = latest_activated_protocol_params(config)?;
    params
        .rewards
        .app_weight_overrides
        .into_iter()
        .find(|entry| entry.app_id == app_id)
        .map(|entry| entry.game_coefficient_ppm)
}

pub fn latest_activated_protocol_params(config: &NodeConfig) -> Option<ProtocolParams> {
    let store = PersistentStoreStub::open(local_chain_store_path(config)).ok()?;
    store
        .params_update_proposals_iter()
        .into_iter()
        .filter(|proposal| proposal.state == GovernanceProposalState::Activated)
        .max_by_key(|proposal| proposal.effective_epoch)
        .map(|proposal| proposal.params)
}

fn find_game_mapping<'a>(
    config: &'a NodeConfig,
    process_name: &str,
) -> Option<&'a crate::node_config::RewardGameMapping> {
    let normalized = normalize_process_name(process_name);
    config
        .reward
        .game_mappings
        .iter()
        .find(|mapping| normalize_process_name(&mapping.process_name) == normalized)
}

fn normalize_process_name(process_name: &str) -> String {
    process_name
        .trim()
        .to_ascii_lowercase()
        .trim_end_matches(".exe")
        .to_string()
}

fn canonical_process_name(process_name: &str) -> String {
    let trimmed = process_name.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let base = if trimmed.len() >= 4 && trimmed[trimmed.len() - 4..].eq_ignore_ascii_case(".exe") {
        &trimmed[..trimmed.len() - 4]
    } else {
        trimmed
    };
    format!("{base}.exe")
}

pub fn player_reward_tick_artifact_path(
    config: &NodeConfig,
    epoch_id: EpochId,
    slot_id: u64,
) -> std::path::PathBuf {
    Path::new(&config.runtime.data_dir)
        .join("player-reward-blocks")
        .join(format!("epoch-{epoch_id:06}-slot-{slot_id:06}.json"))
}

fn load_player_reward_ticks_for_epoch(
    config: &NodeConfig,
    epoch_id: EpochId,
) -> Result<Vec<PlayerRewardTickArtifact>, NodeRewardError> {
    let dir = Path::new(&config.runtime.data_dir).join("player-reward-blocks");
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    let mut artifacts = Vec::new();
    for entry in entries {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let content = fs::read_to_string(path)?;
        let mut artifact = serde_json::from_str::<PlayerRewardTickArtifact>(&content)?;
        artifact.normalize_whitepaper_fields();
        if artifact.epoch_id == epoch_id {
            artifacts.push(artifact);
        }
    }

    Ok(artifacts)
}

#[derive(Debug, Clone, Copy)]
enum RewardComponent {
    Collect,
    Store,
}

fn merge_reward_component(
    rewards: &mut BTreeMap<NodeId, RewardRecord>,
    epoch_id: EpochId,
    component_rewards: &BTreeMap<NodeId, Amount>,
    component: RewardComponent,
) {
    for (node_id, amount) in component_rewards {
        let reward = rewards
            .entry(*node_id)
            .or_insert_with(|| empty_reward_record(epoch_id, *node_id));
        match component {
            RewardComponent::Collect => reward.collect_reward += *amount,
            RewardComponent::Store => reward.store_reward += *amount,
        }
    }
}

fn empty_reward_record(epoch_id: EpochId, node_id: NodeId) -> RewardRecord {
    RewardRecord {
        epoch_id,
        node_id,
        player_reward: 0,
        collect_reward: 0,
        store_reward: 0,
        verify_reward: 0,
        propose_reward: 0,
        slash_debit: 0,
        net_reward: 0,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::*;
    use crate::node_config::{
        CapabilityConfig, CollectConfig, NodeConfig, RewardConfig, RewardGameMapping,
        RuntimeConfig, StorageConfig,
    };
    use crate::node_pipeline::SteamCurrentPlayersSample;
    use crate::primitives::NodeId;
    use crate::records::{
        GovernanceParamsUpdateProposalRecord, GovernanceProposalKind, GovernanceProposalState,
    };
    use crate::store::PersistentStoreStub;

    fn node_id(byte: u8) -> NodeId {
        let mut id = [0u8; 32];
        id[0] = byte;
        id
    }

    #[test]
    fn allocate_proportional_returns_empty_for_zero_pool_or_empty_weights() {
        assert!(allocate_proportional(&BTreeMap::new(), 100).is_empty());
        let mut w = BTreeMap::new();
        w.insert(node_id(1), 10u128);
        assert!(allocate_proportional(&w, 0).is_empty());
    }

    #[test]
    fn allocate_proportional_splits_even_weights_deterministically() {
        let mut w = BTreeMap::new();
        w.insert(node_id(1), 1);
        w.insert(node_id(2), 1);
        let out = allocate_proportional(&w, 100);
        assert_eq!(out.get(&node_id(1)).copied(), Some(50));
        assert_eq!(out.get(&node_id(2)).copied(), Some(50));
        assert_eq!(out.values().sum::<Amount>(), 100);
    }

    #[test]
    fn allocate_proportional_distributes_leftover_by_largest_remainder_round_robin() {
        let mut w = BTreeMap::new();
        w.insert(node_id(1), 1);
        w.insert(node_id(2), 1);
        // 101 * 1 / 2 = 50 rem 1 each; one extra unit to the larger NodeId in remainder tie-break
        let out = allocate_proportional(&w, 101);
        assert_eq!(out.get(&node_id(1)).copied(), Some(50));
        assert_eq!(out.get(&node_id(2)).copied(), Some(51));
        assert_eq!(out.values().sum::<Amount>(), 101);
    }

    #[test]
    fn allocate_proportional_three_way_pool_100() {
        let mut w = BTreeMap::new();
        w.insert(node_id(1), 1);
        w.insert(node_id(2), 1);
        w.insert(node_id(3), 1);
        let out = allocate_proportional(&w, 100);
        assert_eq!(out.values().sum::<Amount>(), 100);
        assert_eq!(out.get(&node_id(1)).copied(), Some(33));
        assert_eq!(out.get(&node_id(2)).copied(), Some(33));
        assert_eq!(out.get(&node_id(3)).copied(), Some(34));
    }

    #[test]
    fn adjusted_player_block_reward_is_unchanged_when_target_equals_current() {
        let base = 1_000u128;
        assert_eq!(
            adjusted_player_block_reward(base, 9_000_000, 9_000_000, 2_000),
            base
        );
    }

    #[test]
    fn adjusted_player_block_reward_returns_base_when_target_or_current_weight_is_zero() {
        let base = 5_000u128;
        assert_eq!(adjusted_player_block_reward(base, 0, 1_000, 500), base);
        assert_eq!(adjusted_player_block_reward(base, 1_000, 0, 500), base);
    }

    #[test]
    fn adjusted_player_block_reward_scales_with_sqrt_ratio_when_within_cap() {
        // scaled_ratio = (4e6 * 1e12) / 1e6 = 4e12 -> sqrt = 2e6 -> adjusted = base * 2
        let base = 1_000u128;
        assert_eq!(
            adjusted_player_block_reward(base, 4_000_000, 1_000_000, 10_000),
            2_000
        );
    }

    #[test]
    fn adjusted_player_block_reward_clamps_to_cap_bps() {
        let base = 1_000u128;
        let cap_bps = 1_000u16;
        let upper = base.saturating_mul(10_000u128 + u128::from(cap_bps)) / 10_000;
        let adjusted = adjusted_player_block_reward(base, 1_000_000_000, 1, cap_bps);
        assert_eq!(adjusted, upper);
    }

    #[test]
    fn tokenomics_service_reward_pools_scale_with_completed_player_blocks() {
        let config = NodeConfig {
            capabilities: CapabilityConfig {
                collect: true,
                store: true,
                verify: true,
                propose: true,
                archive: false,
            },
            reward: RewardConfig {
                reward_source: crate::node_config::RewardSourceMode::Tokenomics,
                emission_year: 1,
                reward_block_secs: 3_600,
                player_block_reward: 1_000,
                ..RewardConfig::default()
            },
            ..NodeConfig::default()
        };

        let pools = service_reward_pools_for_epoch(&config, 2, 0, true, true);
        assert_eq!(pools.collect_pool, 2_283);
        assert_eq!(pools.store_pool, 1_141);
        assert_eq!(pools.verify_pool, 684);
        assert_eq!(pools.propose_pool, 456);
    }

    #[test]
    fn tokenomics_service_reward_pools_disable_ineligible_lanes() {
        let config = NodeConfig {
            capabilities: CapabilityConfig {
                collect: true,
                store: false,
                verify: false,
                propose: false,
                archive: false,
            },
            reward: RewardConfig {
                reward_source: crate::node_config::RewardSourceMode::Tokenomics,
                emission_year: 1,
                reward_block_secs: 3_600,
                player_block_reward: 1_000,
                ..RewardConfig::default()
            },
            ..NodeConfig::default()
        };

        let pools = service_reward_pools_for_epoch(&config, 1, 0, false, false);
        assert_eq!(pools.collect_pool, 1_141);
        assert_eq!(pools.store_pool, 0);
        assert_eq!(pools.verify_pool, 0);
        assert_eq!(pools.propose_pool, 0);
    }

    #[test]
    fn activated_governance_app_weight_override_is_used_for_reward_calculation() {
        let data_dir =
            std::env::temp_dir().join(format!("pole-gov-app-weight-{}", std::process::id()));
        if data_dir.exists() {
            std::fs::remove_dir_all(&data_dir).unwrap();
        }

        let config = NodeConfig {
            chain_id: "pole-local".into(),
            node_id_hex: crate::hex_32([0x31; 32]),
            reward_address_hex: crate::hex_32([0x41; 32]),
            capabilities: CapabilityConfig {
                collect: true,
                store: false,
                verify: false,
                propose: false,
                archive: false,
            },
            collect: CollectConfig {
                enabled: true,
                default_epoch_id: 1,
                default_slot_id: 1,
            },
            runtime: RuntimeConfig {
                data_dir: data_dir.to_string_lossy().into_owned(),
                poll_interval_secs: 300,
                slots_per_epoch: 24,
                challenge_window_blocks: 20,
                low_impact_mode: true,
                os_background_priority: false,
                game_active_poll_interval_secs: 60,
                game_process_names: vec!["fixture_game.exe".into()],
                target_app_ids: vec![730],
                p2p_simulation: crate::node_config::P2pSimulationConfig::default(),
                p2p_socket: crate::node_config::P2pSocketConfig::default(),
                p2p_libp2p: crate::node_config::P2pLibp2pConfig::default(),
                activity_sources: Vec::new(),
            },
            storage: StorageConfig {
                quota_gb: 1,
                retention_epochs: 2,
            },
            reward: RewardConfig {
                reward_source: crate::node_config::RewardSourceMode::Static,
                emission_year: 1,
                reward_block_secs: 3_600,
                player_block_reward: 1_000,
                reward_adjustment_period_blocks: 288,
                target_network_weight_units: 1,
                reward_adjustment_cap_bps: 2_000,
                collect_reward_bps: 5_000,
                store_reward_bps: 2_500,
                verify_reward_bps: 1_500,
                propose_reward_bps: 1_000,
                tail_emission_start_year: 4,
                tail_emission_rate_bps: 200,
                game_mappings: vec![RewardGameMapping {
                    process_name: "fixture_game.exe".into(),
                    app_id: 730,
                    game_coefficient_ppm: 1_000_000,
                }],
            },
        };

        let store_path = PathBuf::from(&config.runtime.data_dir)
            .join("local-chain")
            .join("store.bin");
        let mut store = PersistentStoreStub::open(&store_path).unwrap();
        let mut params = crate::params::ProtocolParams {
            slot_seconds: 300,
            epoch_slots: 12,
            committee_size: 21,
            unbonding_blocks: 5,
            min_verify_bond: 100,
            min_propose_bond: 10_000,
            challenge_window_blocks: 20,
            max_emergency_brake_blocks: 100,
            min_retention_epochs: 2,
            fee: crate::params::FeeParams {
                base_gas_price_nano: 100,
                max_gas_price_nano: 1_000,
                gas_adjustment_ppm: 1_150_000,
                congestion_threshold_ppm: 500_000,
                fee_burn_bps: 2_500,
            },
            rewards: crate::params::RewardParams {
                reward_source_is_tokenomics: false,
                emission_year: 1,
                reward_block_secs: 3_600,
                initial_emission_rate_bps: 2_000,
                tail_emission_start_year: 4,
                tail_emission_rate_bps: 200,
                player_reward_allocation_bps: 8_000,
                service_reward_allocation_bps: 1_000,
                collect_reward_bps: 5_000,
                store_reward_bps: 2_500,
                verify_reward_bps: 1_500,
                propose_reward_bps: 1_000,
                configured_player_block_reward: 1_000,
                effective_player_block_reward: 1_000,
                target_network_weight_units: 1,
                reward_adjustment_cap_bps: 2_000,
                tier1_weight_ppm: 1_000_000,
                tier2_weight_min_ppm: 300_000,
                tier2_weight_max_ppm: 600_000,
                tier3_weight_min_ppm: 50_000,
                tier3_weight_max_ppm: 150_000,
                app_weight_overrides: Vec::new(),
                reward_burn_threshold: 10_000,
                reward_burn_bps: 1_000,
                governance_burn_bps: 100,
            },
            governance: crate::params::GovernanceParams {
                params_update_bond: 10_000,
                params_update_quorum_bps: 2_500,
                params_update_approval_bps: 6_000,
                slow_params_update_bond: 20_000,
                slow_params_update_quorum_bps: 3_300,
                slow_params_update_approval_bps: 7_500,
            },
            slashing: crate::params::SlashingParams {
                double_sign_bps: 5_000,
                offline_bps: 100,
                medium_deviation_bps: 500,
                severe_deviation_bps: 2_000,
            },
        };
        params.rewards.app_weight_overrides = vec![crate::params::AppWeightOverride {
            app_id: 730,
            game_coefficient_ppm: 850_000,
        }];
        store.insert_params_update_proposal(
            [0x99; 32],
            GovernanceParamsUpdateProposalRecord {
                proposal_id: [0x99; 32],
                proposer: [0x41; 32],
                kind: GovernanceProposalKind::FastParams,
                effective_epoch: 2,
                submitted_height: 1,
                bond_amount: 10_000,
                params_hash: [0x55; 32],
                params,
                state: GovernanceProposalState::Activated,
            },
        );
        store.flush().unwrap();

        assert_eq!(game_coefficient_ppm_for_app(&config, 730), 850_000);

        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    /// Fixture: one 1h-equivalent tick, single foreground game — `player_reward` is
    /// `fixed_block_reward * player_weight_units / total_network_weight_units` (integer division).
    #[test]
    fn record_player_reward_tick_full_block_proportional_split_fixture() {
        let data_dir =
            std::env::temp_dir().join(format!("pole-reward-fixture-{}", std::process::id()));
        if data_dir.exists() {
            std::fs::remove_dir_all(&data_dir).unwrap();
        }

        let config = NodeConfig {
            chain_id: "pole-local".into(),
            node_id_hex: crate::hex_32([0x31; 32]),
            reward_address_hex: crate::hex_32([0x41; 32]),
            capabilities: CapabilityConfig {
                collect: true,
                store: false,
                verify: false,
                propose: false,
                archive: false,
            },
            collect: CollectConfig {
                enabled: true,
                default_epoch_id: 1,
                default_slot_id: 1,
            },
            runtime: RuntimeConfig {
                data_dir: data_dir.to_string_lossy().into_owned(),
                poll_interval_secs: 300,
                slots_per_epoch: 24,
                challenge_window_blocks: 20,
                low_impact_mode: true,
                os_background_priority: false,
                game_active_poll_interval_secs: 60,
                game_process_names: Vec::new(),
                target_app_ids: vec![730],
                p2p_simulation: crate::node_config::P2pSimulationConfig::default(),
                p2p_socket: crate::node_config::P2pSocketConfig::default(),
                p2p_libp2p: crate::node_config::P2pLibp2pConfig::default(),
                activity_sources: Vec::new(),
            },
            storage: StorageConfig {
                quota_gb: 1,
                retention_epochs: 2,
            },
            reward: RewardConfig {
                reward_source: crate::node_config::RewardSourceMode::Static,
                emission_year: 1,
                reward_block_secs: 3_600,
                player_block_reward: 10_000,
                reward_adjustment_period_blocks: 288,
                target_network_weight_units: 1,
                reward_adjustment_cap_bps: 2_000,
                collect_reward_bps: 5_000,
                store_reward_bps: 2_500,
                verify_reward_bps: 1_500,
                propose_reward_bps: 1_000,
                tail_emission_start_year: 4,
                tail_emission_rate_bps: 200,
                game_mappings: vec![RewardGameMapping {
                    process_name: "fixture_game.exe".into(),
                    app_id: 730,
                    game_coefficient_ppm: 1_000_000,
                }],
            },
        };

        let context = RewardTickContext {
            epoch_id: 1,
            slot_id: 5,
            sampled_interval_secs: 3_600,
            fixed_block_reward: 8_888,
            fixed_player_reward: 8_888,
            reward_adjustment_period_index: 0,
            adjustment_cycle_index: 0,
            fixed_block_reward_basis_period_index: 0,
            fixed_player_reward_basis_cycle_index: 0,
            fixed_block_reward_basis_network_weight_units: 0,
            fixed_player_reward_basis_total_network_weight_units: 0,
        };
        let samples = vec![SteamCurrentPlayersSample::steam_current_players(
            730, 42, 0, "{}",
        )];

        let out = record_player_reward_tick(
            &config,
            context,
            None,
            Some("fixture_game.exe"),
            &[],
            &samples,
        )
        .unwrap();

        assert_eq!(out.artifact.completed_reward_block_count, 1);
        assert_eq!(out.artifact.records.len(), 1);
        let r = &out.artifact.records[0];
        assert_eq!(r.play_seconds, 3_600);
        let pw = 3_600u128.saturating_mul(1_000_000);
        let tw = 42u128.saturating_mul(3_600).saturating_mul(1_000_000);
        assert_eq!(r.player_weight_units, pw);
        assert_eq!(r.total_network_weight_units, tw);
        assert_eq!(r.block_reward, 8_888);
        let expected_share = 8_888u128.saturating_mul(pw) / tw;
        assert_eq!(r.player_reward, expected_share);
        assert_eq!(out.artifact.total_player_reward, expected_share);

        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    /// Twelve 5-minute slices must match one full reward block (Phase 4 traceability).
    #[test]
    fn record_player_reward_tick_twelve_ticks_match_one_hour_block_fixture() {
        let data_dir =
            std::env::temp_dir().join(format!("pole-reward-12tick-{}", std::process::id()));
        if data_dir.exists() {
            std::fs::remove_dir_all(&data_dir).unwrap();
        }

        let config = NodeConfig {
            chain_id: "pole-local".into(),
            node_id_hex: crate::hex_32([0x31; 32]),
            reward_address_hex: crate::hex_32([0x41; 32]),
            capabilities: CapabilityConfig {
                collect: true,
                store: false,
                verify: false,
                propose: false,
                archive: false,
            },
            collect: CollectConfig {
                enabled: true,
                default_epoch_id: 1,
                default_slot_id: 1,
            },
            runtime: RuntimeConfig {
                data_dir: data_dir.to_string_lossy().into_owned(),
                poll_interval_secs: 300,
                slots_per_epoch: 24,
                challenge_window_blocks: 20,
                low_impact_mode: true,
                os_background_priority: false,
                game_active_poll_interval_secs: 60,
                game_process_names: Vec::new(),
                target_app_ids: vec![730],
                p2p_simulation: crate::node_config::P2pSimulationConfig::default(),
                p2p_socket: crate::node_config::P2pSocketConfig::default(),
                p2p_libp2p: crate::node_config::P2pLibp2pConfig::default(),
                activity_sources: Vec::new(),
            },
            storage: StorageConfig {
                quota_gb: 1,
                retention_epochs: 2,
            },
            reward: RewardConfig {
                reward_source: crate::node_config::RewardSourceMode::Static,
                emission_year: 1,
                reward_block_secs: 3_600,
                player_block_reward: 1_000,
                reward_adjustment_period_blocks: 2,
                target_network_weight_units: 3_600_000_000_000,
                reward_adjustment_cap_bps: 2_000,
                collect_reward_bps: 5_000,
                store_reward_bps: 2_500,
                verify_reward_bps: 1_500,
                propose_reward_bps: 1_000,
                tail_emission_start_year: 4,
                tail_emission_rate_bps: 200,
                game_mappings: vec![RewardGameMapping {
                    process_name: "TestGame.exe".into(),
                    app_id: 730,
                    game_coefficient_ppm: 1_000_000,
                }],
            },
        };

        let samples = vec![SteamCurrentPlayersSample::steam_current_players(
            730, 1_000, 0, "{}",
        )];
        let fixed = 1_000u128;
        let mut pending: Option<PendingPlayerRewardBlockState> = None;
        let mut last_comp: Option<PlayerRewardTickComputation> = None;
        for tick in 0..12 {
            let ctx = RewardTickContext {
                epoch_id: 1,
                slot_id: tick + 1,
                sampled_interval_secs: 300,
                fixed_block_reward: fixed,
                fixed_player_reward: fixed,
                reward_adjustment_period_index: 0,
                adjustment_cycle_index: 0,
                fixed_block_reward_basis_period_index: 0,
                fixed_player_reward_basis_cycle_index: 0,
                fixed_block_reward_basis_network_weight_units: 0,
                fixed_player_reward_basis_total_network_weight_units: 0,
            };
            last_comp = Some(
                record_player_reward_tick(
                    &config,
                    ctx,
                    pending,
                    Some("TestGame.exe"),
                    &[],
                    &samples,
                )
                .unwrap(),
            );
            pending = last_comp.as_ref().unwrap().pending_state.clone();
        }
        let last_comp = last_comp.unwrap();
        let r = &last_comp.artifact.records[0];
        assert_eq!(last_comp.artifact.completed_reward_block_count, 1);
        assert_eq!(last_comp.artifact.records.len(), 1);
        assert_eq!(last_comp.artifact.records[0].play_seconds, 3_600);
        assert_eq!(
            last_comp.artifact.records[0].total_network_weight_units,
            3_600_000_000_000
        );
        assert_eq!(r.player_weight_units, 3_600u128.saturating_mul(1_000_000));
        assert_eq!(last_comp.artifact.records[0].block_reward, fixed);
        // Steam 样本每应用 1000 在线 ⇒ 全网权重是本地游玩权重的 1000 倍 ⇒ 份额约 1/1000。
        assert_eq!(last_comp.artifact.records[0].player_reward, 1);

        std::fs::remove_dir_all(&data_dir).unwrap();
    }
}
