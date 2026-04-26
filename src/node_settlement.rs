use std::fmt;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::json_file::{load_json_or_default, save_pretty_json};
use crate::node_config::{NodeConfig, NodeConfigError};
use crate::node_daemon::{
    epoch_preparation_artifact_path, epoch_settlement_artifact_path,
    governance_index_artifact_path, governance_proposal_artifact_path,
    governance_scheduled_artifact_path, load_batches_for_epoch, local_chain_runtime_path,
    local_chain_store_path,
};
use crate::node_prepare::{compute_local_epoch_preparation, NodePreparationError};
use crate::node_rewards::{
    effective_min_retention_epochs, effective_player_block_reward,
    effective_reward_adjustment_cap_bps, effective_reward_block_secs,
    effective_target_network_weight_units,
};
use crate::params::{AppWeightOverride, FeeParams, ProtocolParams, RewardParams, SlashingParams};
use crate::primitives::{Address, Amount, Capability, EpochId, Height, NodeStatus};
use crate::records::{EpochCommit, GovernanceProposalState};
use crate::state::{AccountState, NodeRegistry, ReputationSnapshot};
use crate::store::{PersistentStoreStub, ProtocolStore};
use crate::tokenomics::{
    INITIAL_EMISSION_RATE_BPS, PLAYER_REWARD_ALLOCATION_BPS, SERVICE_REWARD_ALLOCATION_BPS,
};
use crate::transactions::{ClaimRewardTx, CommitEpochTx, SubmitBatchTx};
use crate::transitions::{ProtocolState, TransitionError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalChainRuntimeState {
    pub height: Height,
    pub current_epoch: EpochId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpochSettlementArtifact {
    pub epoch_id: EpochId,
    pub submission_height: Height,
    pub challenge_window_blocks: u32,
    pub challenge_deadline_height: Height,
    pub finalization_height: Height,
    pub prepared_ready_for_submission: bool,
    pub batch_count: u32,
    pub payload_count: u32,
    pub batch_submission_count: usize,
    pub batch_already_present_count: usize,
    pub reward_record_count: usize,
    pub accepted_batches_root_hex: String,
    pub observations_root_hex: String,
    pub availability_root_hex: String,
    pub aggregates_root_hex: String,
    pub rewards_root_hex: String,
    pub randomness_seed_hex: String,
    pub commit_applied: bool,
    pub commit_already_present: bool,
    pub epoch_finalized: bool,
    pub epoch_already_finalized: bool,
    pub local_node_id_hex: String,
    pub local_reward_available: Amount,
    pub local_reward_claimed: bool,
    pub local_reward_already_claimed: bool,
    pub local_reward_balance: Amount,
    pub current_epoch_after: EpochId,
    pub local_chain_runtime_path: String,
    pub local_chain_store_path: String,
    pub prepared_epoch_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceProposalArtifact {
    pub proposal_id_hex: String,
    pub proposer_hex: String,
    pub effective_epoch: u64,
    pub submitted_height: u64,
    pub bond_amount: u128,
    pub params_hash_hex: String,
    pub proposal_state: String,
    pub vote_record_count: usize,
    pub yes_voting_power: u128,
    pub no_voting_power: u128,
    pub abstain_voting_power: u128,
    pub params: ProtocolParams,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceScheduledParamsArtifact {
    pub epoch_id: u64,
    pub current_epoch: u64,
    pub scheduled: bool,
    pub params: Option<ProtocolParams>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceProposalIndexEntry {
    pub proposal_id_hex: String,
    pub proposal_state: String,
    pub effective_epoch: u64,
    pub artifact_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceScheduledIndexEntry {
    pub epoch_id: u64,
    pub scheduled: bool,
    pub artifact_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GovernanceArtifactIndex {
    pub proposal_artifacts: Vec<GovernanceProposalIndexEntry>,
    pub scheduled_artifacts: Vec<GovernanceScheduledIndexEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GovernanceArtifactSummary {
    pub pending_proposal_count: usize,
    pub scheduled_proposal_count: usize,
    pub activated_proposal_count: usize,
    pub expired_proposal_count: usize,
    pub proposal_artifact_count: usize,
    pub scheduled_artifact_count: usize,
    pub latest_effective_epoch: Option<u64>,
    pub artifact_index_path: String,
}

#[derive(Debug)]
pub enum NodeSettlementError {
    Config(NodeConfigError),
    Preparation(NodePreparationError),
    Transition(TransitionError),
    Io(std::io::Error),
    Json(serde_json::Error),
    SubmissionHeightRegression { requested: Height, current: Height },
}

impl fmt::Display for NodeSettlementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(err) => write!(f, "config error: {err}"),
            Self::Preparation(err) => write!(f, "preparation error: {err}"),
            Self::Transition(err) => write!(f, "transition error: {err:?}"),
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::Json(err) => write!(f, "json error: {err}"),
            Self::SubmissionHeightRegression { requested, current } => write!(
                f,
                "submission height regression: requested {requested}, current local chain height is {current}"
            ),
        }
    }
}

impl std::error::Error for NodeSettlementError {}

impl From<NodeConfigError> for NodeSettlementError {
    fn from(value: NodeConfigError) -> Self {
        Self::Config(value)
    }
}

impl From<NodePreparationError> for NodeSettlementError {
    fn from(value: NodePreparationError) -> Self {
        Self::Preparation(value)
    }
}

impl From<TransitionError> for NodeSettlementError {
    fn from(value: TransitionError) -> Self {
        Self::Transition(value)
    }
}

impl From<std::io::Error> for NodeSettlementError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for NodeSettlementError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl LocalChainRuntimeState {
    pub fn load_or_default(
        path: impl AsRef<Path>,
        default_height: Height,
        default_epoch: EpochId,
    ) -> Result<Self, NodeSettlementError> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self {
                height: default_height,
                current_epoch: default_epoch,
            });
        }
        let content = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), NodeSettlementError> {
        save_pretty_json(self, path)
    }
}

impl EpochSettlementArtifact {
    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), NodeSettlementError> {
        save_pretty_json(self, path)
    }
}

impl GovernanceProposalArtifact {
    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), NodeSettlementError> {
        save_pretty_json(self, path)
    }
}

impl GovernanceScheduledParamsArtifact {
    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), NodeSettlementError> {
        save_pretty_json(self, path)
    }
}

impl GovernanceArtifactIndex {
    pub fn load_or_default_json(path: impl AsRef<Path>) -> Result<Self, NodeSettlementError> {
        load_json_or_default(path)
    }

    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), NodeSettlementError> {
        save_pretty_json(self, path)
    }

    pub fn upsert_proposal_artifact(
        &mut self,
        proposal_id_hex: String,
        proposal_state: String,
        effective_epoch: u64,
        artifact_path: String,
    ) {
        if let Some(entry) = self
            .proposal_artifacts
            .iter_mut()
            .find(|entry| entry.proposal_id_hex == proposal_id_hex)
        {
            entry.proposal_state = proposal_state;
            entry.effective_epoch = effective_epoch;
            entry.artifact_path = artifact_path;
        } else {
            self.proposal_artifacts.push(GovernanceProposalIndexEntry {
                proposal_id_hex,
                proposal_state,
                effective_epoch,
                artifact_path,
            });
        }
        self.proposal_artifacts
            .sort_by(|left, right| left.proposal_id_hex.cmp(&right.proposal_id_hex));
    }

    pub fn upsert_scheduled_artifact(
        &mut self,
        epoch_id: u64,
        scheduled: bool,
        artifact_path: String,
    ) {
        if let Some(entry) = self
            .scheduled_artifacts
            .iter_mut()
            .find(|entry| entry.epoch_id == epoch_id)
        {
            entry.scheduled = scheduled;
            entry.artifact_path = artifact_path;
        } else {
            self.scheduled_artifacts
                .push(GovernanceScheduledIndexEntry {
                    epoch_id,
                    scheduled,
                    artifact_path,
                });
        }
        self.scheduled_artifacts.sort_by_key(|entry| entry.epoch_id);
    }
}

impl GovernanceArtifactSummary {
    pub fn load_or_default_json(path: impl AsRef<Path>) -> Result<Self, NodeSettlementError> {
        load_json_or_default(path)
    }

    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), NodeSettlementError> {
        save_pretty_json(self, path)
    }
}

pub fn suggested_settlement_height(config: &NodeConfig) -> Result<Height, NodeSettlementError> {
    let runtime = LocalChainRuntimeState::load_or_default(
        local_chain_runtime_path(config),
        0,
        config.collect.default_epoch_id,
    )?;
    Ok(runtime.height.saturating_add(1).max(1))
}

pub fn open_local_protocol_state(
    config: &NodeConfig,
    challenge_window_blocks: u32,
) -> Result<(LocalChainRuntimeState, ProtocolState<PersistentStoreStub>), NodeSettlementError> {
    let runtime_path = local_chain_runtime_path(config);
    let store_path = local_chain_store_path(config);
    let runtime =
        LocalChainRuntimeState::load_or_default(&runtime_path, 0, config.collect.default_epoch_id)?;
    let store = PersistentStoreStub::open(&store_path)?;
    let params = latest_activated_protocol_params_from_store(&store)
        .unwrap_or_else(|| local_protocol_params(config, challenge_window_blocks));
    let current_epoch = runtime.current_epoch.max(config.collect.default_epoch_id);
    let mut state = ProtocolState::with_store(params, runtime.height, current_epoch, store);
    bootstrap_local_chain_state(config, &mut state)?;
    Ok((runtime, state))
}

pub fn export_governance_artifacts(
    config: &NodeConfig,
    store: &PersistentStoreStub,
    current_epoch: EpochId,
) -> Result<GovernanceArtifactIndex, NodeSettlementError> {
    let index_path = governance_index_artifact_path(config);
    let mut index = GovernanceArtifactIndex::default();

    for proposal in store.params_update_proposals_iter() {
        let (artifact, artifact_path) =
            build_governance_proposal_artifact(config, store, &proposal)?;
        index.upsert_proposal_artifact(
            artifact.proposal_id_hex.clone(),
            artifact.proposal_state.clone(),
            artifact.effective_epoch,
            artifact_path.to_string_lossy().into_owned(),
        );
    }

    for (epoch_id, params) in &store.inner.scheduled_protocol_params {
        let artifact = GovernanceScheduledParamsArtifact {
            epoch_id: *epoch_id,
            current_epoch,
            scheduled: true,
            params: Some(params.clone()),
        };
        let artifact_path = governance_scheduled_artifact_path(config, *epoch_id);
        artifact.save_json(&artifact_path)?;
        index.upsert_scheduled_artifact(
            *epoch_id,
            true,
            artifact_path.to_string_lossy().into_owned(),
        );
    }

    index.save_json(&index_path)?;
    let summary = GovernanceArtifactSummary {
        pending_proposal_count: index
            .proposal_artifacts
            .iter()
            .filter(|entry| entry.proposal_state == "Pending")
            .count(),
        scheduled_proposal_count: index
            .proposal_artifacts
            .iter()
            .filter(|entry| entry.proposal_state == "Scheduled")
            .count(),
        activated_proposal_count: index
            .proposal_artifacts
            .iter()
            .filter(|entry| entry.proposal_state == "Activated")
            .count(),
        expired_proposal_count: index
            .proposal_artifacts
            .iter()
            .filter(|entry| entry.proposal_state == "Expired")
            .count(),
        proposal_artifact_count: index.proposal_artifacts.len(),
        scheduled_artifact_count: index.scheduled_artifacts.len(),
        latest_effective_epoch: index
            .proposal_artifacts
            .iter()
            .map(|entry| entry.effective_epoch)
            .max(),
        artifact_index_path: index_path.to_string_lossy().into_owned(),
    };
    summary.save_json(crate::node_daemon::governance_summary_artifact_path(config))?;
    Ok(index)
}

pub fn export_governance_proposal_artifact(
    config: &NodeConfig,
    store: &PersistentStoreStub,
    proposal_id: &crate::Hash32,
) -> Result<
    Option<(
        GovernanceProposalArtifact,
        std::path::PathBuf,
        std::path::PathBuf,
    )>,
    NodeSettlementError,
> {
    let Some(proposal) = store.params_update_proposal(proposal_id).cloned() else {
        return Ok(None);
    };
    let (artifact, artifact_path) = build_governance_proposal_artifact(config, store, &proposal)?;
    let index_path = governance_index_artifact_path(config);
    let mut index = GovernanceArtifactIndex::load_or_default_json(&index_path)?;
    index.upsert_proposal_artifact(
        artifact.proposal_id_hex.clone(),
        artifact.proposal_state.clone(),
        artifact.effective_epoch,
        artifact_path.to_string_lossy().into_owned(),
    );
    index.save_json(&index_path)?;
    Ok(Some((artifact, artifact_path, index_path)))
}

pub fn export_governance_scheduled_artifact(
    config: &NodeConfig,
    current_epoch: EpochId,
    epoch_id: EpochId,
    params: Option<&ProtocolParams>,
) -> Result<
    (
        GovernanceScheduledParamsArtifact,
        std::path::PathBuf,
        std::path::PathBuf,
    ),
    NodeSettlementError,
> {
    let artifact = GovernanceScheduledParamsArtifact {
        epoch_id,
        current_epoch,
        scheduled: params.is_some(),
        params: params.cloned(),
    };
    let artifact_path = governance_scheduled_artifact_path(config, epoch_id);
    artifact.save_json(&artifact_path)?;
    let index_path = governance_index_artifact_path(config);
    let mut index = GovernanceArtifactIndex::load_or_default_json(&index_path)?;
    index.upsert_scheduled_artifact(
        epoch_id,
        artifact.scheduled,
        artifact_path.to_string_lossy().into_owned(),
    );
    index.save_json(&index_path)?;
    Ok((artifact, artifact_path, index_path))
}

fn build_governance_proposal_artifact(
    config: &NodeConfig,
    store: &PersistentStoreStub,
    proposal: &crate::GovernanceParamsUpdateProposalRecord,
) -> Result<(GovernanceProposalArtifact, std::path::PathBuf), NodeSettlementError> {
    let votes = store.vote_records_for_proposal(&proposal.proposal_id);
    let (yes, no, abstain) = summarize_governance_votes(&votes);
    let proposal_id_hex = crate::hex_32(proposal.proposal_id);
    let artifact = GovernanceProposalArtifact {
        proposal_id_hex: proposal_id_hex.clone(),
        proposer_hex: crate::hex_32(proposal.proposer),
        effective_epoch: proposal.effective_epoch,
        submitted_height: proposal.submitted_height,
        bond_amount: proposal.bond_amount,
        params_hash_hex: crate::hex_32(proposal.params_hash),
        proposal_state: format!("{:?}", proposal.state),
        vote_record_count: votes.len(),
        yes_voting_power: yes,
        no_voting_power: no,
        abstain_voting_power: abstain,
        params: proposal.params.clone(),
    };
    let artifact_path = governance_proposal_artifact_path(config, &proposal_id_hex);
    artifact.save_json(&artifact_path)?;
    Ok((artifact, artifact_path))
}

fn summarize_governance_votes(votes: &[crate::GovernanceVoteRecord]) -> (u128, u128, u128) {
    let mut yes = 0u128;
    let mut no = 0u128;
    let mut abstain = 0u128;
    for vote in votes {
        match vote.choice {
            crate::primitives::VoteChoice::Yes => yes += vote.voting_power,
            crate::primitives::VoteChoice::No => no += vote.voting_power,
            crate::primitives::VoteChoice::Abstain => abstain += vote.voting_power,
        }
    }
    (yes, no, abstain)
}

fn latest_activated_protocol_params_from_store(
    store: &PersistentStoreStub,
) -> Option<ProtocolParams> {
    store
        .params_update_proposals_iter()
        .into_iter()
        .filter(|proposal| proposal.state == GovernanceProposalState::Activated)
        .max_by_key(|proposal| proposal.effective_epoch)
        .map(|proposal| proposal.params)
}

pub fn settle_local_epoch(
    config: &NodeConfig,
    epoch_id: EpochId,
    submission_height: Height,
    challenge_window_blocks: u32,
) -> Result<EpochSettlementArtifact, NodeSettlementError> {
    let preparation = compute_local_epoch_preparation(
        config,
        epoch_id,
        submission_height,
        challenge_window_blocks,
    )?;
    preparation
        .artifact
        .save_json(epoch_preparation_artifact_path(config, epoch_id))
        .map_err(NodeSettlementError::Preparation)?;

    let runtime_path = local_chain_runtime_path(config);
    let store_path = local_chain_store_path(config);
    let mut runtime = LocalChainRuntimeState::load_or_default(
        &runtime_path,
        submission_height.saturating_sub(1),
        epoch_id,
    )?;
    if submission_height <= runtime.height {
        return Err(NodeSettlementError::SubmissionHeightRegression {
            requested: submission_height,
            current: runtime.height,
        });
    }

    let store = PersistentStoreStub::open(&store_path)?;
    let params = local_protocol_params(config, challenge_window_blocks);
    let current_epoch = runtime.current_epoch.max(epoch_id);
    let mut state = ProtocolState::with_store(params, runtime.height, current_epoch, store);

    bootstrap_local_chain_state(config, &mut state)?;
    state.height = submission_height;

    let batches = load_batches_for_epoch(config, epoch_id)
        .map_err(|err| NodeSettlementError::Preparation(NodePreparationError::Daemon(err)))?;
    let mut batch_submission_count = 0usize;
    let mut batch_already_present_count = 0usize;
    for batch in &batches {
        match state.apply_submit_batch(SubmitBatchTx {
            batch_commit: batch.batch_commit.clone(),
            signature: vec![1],
        }) {
            Ok(_) => batch_submission_count += 1,
            Err(TransitionError::DuplicateBatch(_)) => batch_already_present_count += 1,
            Err(err) => return Err(err.into()),
        }
    }

    let (commit_applied, commit_already_present) =
        apply_epoch_commit(&mut state, &preparation.epoch_commit)?;

    for entry in &preparation.reward_artifact.records {
        state.upsert_reward_record(entry.reward.clone());
    }

    let finalization_height = preparation
        .artifact
        .challenge_deadline_height
        .saturating_add(1);
    state.height = state.height.max(finalization_height);
    let (epoch_finalized, epoch_already_finalized) = match state.finalize_epoch(epoch_id) {
        Ok(_) => (true, false),
        Err(TransitionError::EpochAlreadyFinalized(_)) => (false, true),
        Err(err) => return Err(err.into()),
    };

    let local_node_id = config.node_id()?;
    let reward_address = config.reward_address()?;
    let local_reward_available = preparation
        .reward_artifact
        .records
        .iter()
        .find(|entry| entry.reward.node_id == local_node_id)
        .map(|entry| entry.reward.net_reward)
        .unwrap_or(0);

    let (local_reward_claimed, local_reward_already_claimed) = if local_reward_available > 0 {
        match state.apply_claim_reward(ClaimRewardTx {
            claimer: reward_address,
            epoch_id,
            node_id: local_node_id,
            amount: local_reward_available,
            merkle_proof: Vec::new(),
            nonce: state
                .store
                .account(&reward_address)
                .map(|account| account.nonce)
                .unwrap_or(0),
            signature: vec![2],
        }) {
            Ok(_) => (true, false),
            Err(TransitionError::RewardAlreadyClaimed(_)) => (false, true),
            Err(err) => return Err(err.into()),
        }
    } else {
        (false, false)
    };

    runtime.height = state.height;
    runtime.current_epoch = state.current_epoch;
    runtime.save_json(&runtime_path)?;
    state.store.flush()?;
    let _ = export_governance_artifacts(config, &state.store, state.current_epoch)?;

    let local_reward_balance = state
        .store
        .account(&reward_address)
        .map(|account| account.balance)
        .unwrap_or(0);
    let artifact = EpochSettlementArtifact {
        epoch_id,
        submission_height,
        challenge_window_blocks,
        challenge_deadline_height: preparation.artifact.challenge_deadline_height,
        finalization_height,
        prepared_ready_for_submission: preparation.artifact.ready_for_submission,
        batch_count: preparation.epoch_commit_artifact.batch_count,
        payload_count: preparation.epoch_commit_artifact.payload_count,
        batch_submission_count,
        batch_already_present_count,
        reward_record_count: preparation.reward_artifact.reward_count,
        accepted_batches_root_hex: preparation
            .epoch_commit_artifact
            .accepted_batches_root_hex
            .clone(),
        observations_root_hex: preparation
            .epoch_commit_artifact
            .observations_root_hex
            .clone(),
        availability_root_hex: preparation
            .epoch_commit_artifact
            .availability_root_hex
            .clone(),
        aggregates_root_hex: preparation
            .epoch_commit_artifact
            .aggregates_root_hex
            .clone(),
        rewards_root_hex: preparation.epoch_commit_artifact.rewards_root_hex.clone(),
        randomness_seed_hex: preparation
            .epoch_commit_artifact
            .randomness_seed_hex
            .clone(),
        commit_applied,
        commit_already_present,
        epoch_finalized,
        epoch_already_finalized,
        local_node_id_hex: crate::hex_32(local_node_id),
        local_reward_available,
        local_reward_claimed,
        local_reward_already_claimed,
        local_reward_balance,
        current_epoch_after: state.current_epoch,
        local_chain_runtime_path: runtime_path.to_string_lossy().into_owned(),
        local_chain_store_path: store_path.to_string_lossy().into_owned(),
        prepared_epoch_path: epoch_preparation_artifact_path(config, epoch_id)
            .to_string_lossy()
            .into_owned(),
    };
    artifact.save_json(epoch_settlement_artifact_path(config, epoch_id))?;
    Ok(artifact)
}

fn apply_epoch_commit<S: ProtocolStore>(
    state: &mut ProtocolState<S>,
    epoch_commit: &EpochCommit,
) -> Result<(bool, bool), NodeSettlementError> {
    match state.apply_commit_epoch(CommitEpochTx {
        epoch_commit: epoch_commit.clone(),
        signature: vec![1],
    }) {
        Ok(_) => Ok((true, false)),
        Err(TransitionError::DuplicateEpochCommit(_)) => Ok((false, true)),
        Err(err) => Err(err.into()),
    }
}

fn bootstrap_local_chain_state<S: ProtocolStore>(
    config: &NodeConfig,
    state: &mut ProtocolState<S>,
) -> Result<(), NodeSettlementError> {
    let node_id = config.node_id()?;
    let reward_address = config.reward_address()?;

    if state.store.node(&node_id).is_none() {
        state.upsert_node(NodeRegistry {
            node_id,
            pubkey: vec![1, 2, 3],
            reward_address,
            bond: state
                .params
                .min_propose_bond
                .max(state.params.min_verify_bond),
            status: NodeStatus::Active,
            enabled_capabilities: enabled_capabilities(config),
            reputation: ReputationSnapshot {
                score_ppm: 1_000_000,
                successful_challenges: 0,
                failed_challenges: 0,
                challengeable_faults: 0,
                collection_successes: 0,
                collection_failures: 0,
                storage_proofs_passed: 0,
                storage_proofs_failed: 0,
                last_updated_epoch: config.collect.default_epoch_id,
            },
            joined_at_height: 1,
        });
    }

    ensure_account_exists(state, reward_address);
    Ok(())
}

fn ensure_account_exists<S: ProtocolStore>(state: &mut ProtocolState<S>, address: Address) {
    if state.store.account(&address).is_none() {
        state.upsert_account(AccountState {
            address,
            balance: 0,
            staked: 0,
            locked: 0,
            nonce: 0,
        });
    }
}

fn enabled_capabilities(config: &NodeConfig) -> Vec<Capability> {
    let mut capabilities = Vec::new();
    if config.capabilities.collect {
        capabilities.push(Capability::Collect);
    }
    if config.capabilities.store {
        capabilities.push(Capability::Store);
    }
    if config.capabilities.verify {
        capabilities.push(Capability::Verify);
    }
    if config.capabilities.propose {
        capabilities.push(Capability::Propose);
    }
    if config.capabilities.archive {
        capabilities.push(Capability::Archive);
    }
    capabilities
}

fn local_protocol_params(config: &NodeConfig, challenge_window_blocks: u32) -> ProtocolParams {
    ProtocolParams {
        slot_seconds: config
            .runtime
            .poll_interval_secs
            .clamp(1, u64::from(u32::MAX)) as u32,
        epoch_slots: config.runtime.slots_per_epoch.clamp(1, u64::from(u32::MAX)) as u32,
        committee_size: 21,
        unbonding_blocks: 5,
        min_verify_bond: 100,
        min_propose_bond: 10_000,
        challenge_window_blocks,
        max_emergency_brake_blocks: 100,
        min_retention_epochs: effective_min_retention_epochs(config),
        fee: FeeParams {
            base_gas_price_nano: 100,
            max_gas_price_nano: 1_000,
            gas_adjustment_ppm: 1_150_000,
            congestion_threshold_ppm: 500_000,
            fee_burn_bps: 2_500,
        },
        rewards: RewardParams {
            reward_source_is_tokenomics: matches!(
                config.reward.reward_source,
                crate::node_config::RewardSourceMode::Tokenomics
            ),
            emission_year: config.reward.emission_year,
            reward_block_secs: effective_reward_block_secs(config),
            initial_emission_rate_bps: INITIAL_EMISSION_RATE_BPS,
            tail_emission_start_year: config.reward.tail_emission_start_year,
            tail_emission_rate_bps: config.reward.tail_emission_rate_bps,
            player_reward_allocation_bps: PLAYER_REWARD_ALLOCATION_BPS,
            service_reward_allocation_bps: SERVICE_REWARD_ALLOCATION_BPS,
            collect_reward_bps: config.reward.collect_reward_bps,
            store_reward_bps: config.reward.store_reward_bps,
            verify_reward_bps: config.reward.verify_reward_bps,
            propose_reward_bps: config.reward.propose_reward_bps,
            configured_player_block_reward: config.reward.player_block_reward,
            effective_player_block_reward: effective_player_block_reward(config),
            target_network_weight_units: effective_target_network_weight_units(config),
            reward_adjustment_cap_bps: effective_reward_adjustment_cap_bps(config),
            tier1_weight_ppm: 1_000_000,
            tier2_weight_min_ppm: 300_000,
            tier2_weight_max_ppm: 600_000,
            tier3_weight_min_ppm: 50_000,
            tier3_weight_max_ppm: 150_000,
            app_weight_overrides: config
                .reward
                .game_mappings
                .iter()
                .map(|mapping| AppWeightOverride {
                    app_id: mapping.app_id,
                    game_coefficient_ppm: mapping.game_coefficient_ppm,
                })
                .collect(),
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
        slashing: SlashingParams {
            double_sign_bps: 5_000,
            offline_bps: 100,
            medium_deviation_bps: 500,
            severe_deviation_bps: 2_000,
        },
    }
}
