use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::PathBuf;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::params::ProtocolParams;
use crate::primitives::{Address, EpochId, Hash32, Height, NodeId};
use crate::records::{
    BatchCommit, Challenge, ChallengeResolutionRecord, ChallengeResponseRecord, DelegationRecord,
    EpochCommit, GovernanceParamsUpdateProposalRecord, GovernanceVoteRecord, RewardRecord,
    UnbondingRecord,
};
use crate::state::{AccountState, NodeRegistry, StorageOffer};

pub type BatchKey = (EpochId, NodeId, Hash32);
pub type RewardKey = (EpochId, NodeId);
pub type DelegationKey = (Address, NodeId);
pub type VoteKey = (Hash32, Address);

pub trait ProtocolStore {
    fn account(&self, address: &Address) -> Option<&AccountState>;
    fn account_mut(&mut self, address: &Address) -> Option<&mut AccountState>;
    fn insert_account(&mut self, account: AccountState);
    fn accounts_iter(&self) -> Vec<AccountState>;

    fn node(&self, node_id: &NodeId) -> Option<&NodeRegistry>;
    fn node_mut(&mut self, node_id: &NodeId) -> Option<&mut NodeRegistry>;
    fn insert_node(&mut self, node: NodeRegistry);

    fn storage_offer(&self, node_id: &NodeId) -> Option<&StorageOffer>;
    fn insert_storage_offer(&mut self, offer: StorageOffer);

    fn batch(&self, key: &BatchKey) -> Option<&BatchCommit>;
    fn insert_batch(&mut self, key: BatchKey, batch: BatchCommit);
    fn has_any_batch_for_epoch(&self, epoch_id: EpochId) -> bool;

    fn epoch_commit(&self, epoch_id: &EpochId) -> Option<&EpochCommit>;
    fn insert_epoch_commit(&mut self, epoch_id: EpochId, commit: EpochCommit);

    fn open_challenge(&self, challenge_id: &Hash32) -> Option<&Challenge>;
    fn insert_open_challenge(&mut self, challenge_id: Hash32, challenge: Challenge);
    fn remove_open_challenge(&mut self, challenge_id: &Hash32) -> Option<Challenge>;
    fn has_open_challenges_for_epoch(&self, epoch_id: EpochId) -> bool;
    fn insert_resolved_challenge(&mut self, challenge_id: Hash32, challenge: Challenge);
    fn challenge_resolution(&self, challenge_id: &Hash32) -> Option<&ChallengeResolutionRecord>;
    fn insert_challenge_resolution(
        &mut self,
        challenge_id: Hash32,
        resolution: ChallengeResolutionRecord,
    );

    fn reward_record(&self, key: &RewardKey) -> Option<&RewardRecord>;
    fn insert_reward_record(&mut self, key: RewardKey, reward: RewardRecord);
    fn is_reward_claimed(&self, key: &RewardKey) -> bool;
    fn mark_reward_claimed(&mut self, key: RewardKey);

    fn delegation(&self, key: &DelegationKey) -> Option<&DelegationRecord>;
    fn insert_delegation(&mut self, key: DelegationKey, delegation: DelegationRecord);
    fn remove_delegation(&mut self, key: &DelegationKey) -> Option<DelegationRecord>;

    fn queue_unbonding(&mut self, request: UnbondingRecord);
    fn drain_mature_unbondings(&mut self, current_height: Height) -> Vec<UnbondingRecord>;

    fn vote_record(&self, key: &VoteKey) -> Option<&GovernanceVoteRecord>;
    fn insert_vote_record(&mut self, key: VoteKey, vote: GovernanceVoteRecord);
    fn vote_records_for_proposal(&self, proposal_id: &Hash32) -> Vec<GovernanceVoteRecord>;
    fn params_update_proposal(
        &self,
        proposal_id: &Hash32,
    ) -> Option<&GovernanceParamsUpdateProposalRecord>;
    fn params_update_proposals_iter(&self) -> Vec<GovernanceParamsUpdateProposalRecord>;
    fn params_update_proposal_mut(
        &mut self,
        proposal_id: &Hash32,
    ) -> Option<&mut GovernanceParamsUpdateProposalRecord>;
    fn insert_params_update_proposal(
        &mut self,
        proposal_id: Hash32,
        proposal: GovernanceParamsUpdateProposalRecord,
    );

    fn challenge_response(&self, challenge_id: &Hash32) -> Option<&ChallengeResponseRecord>;
    fn insert_challenge_response(
        &mut self,
        challenge_id: Hash32,
        response: ChallengeResponseRecord,
    );

    fn scheduled_protocol_params(&self, epoch_id: &EpochId) -> Option<&ProtocolParams>;
    fn insert_scheduled_protocol_params(&mut self, epoch_id: EpochId, params: ProtocolParams);
    fn take_scheduled_protocol_params(&mut self, epoch_id: &EpochId) -> Option<ProtocolParams>;

    fn is_epoch_finalized(&self, epoch_id: EpochId) -> bool;
    fn mark_epoch_finalized(&mut self, epoch_id: EpochId);
}

#[derive(
    Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct InMemoryStore {
    pub accounts: BTreeMap<Address, AccountState>,
    pub nodes: BTreeMap<NodeId, NodeRegistry>,
    pub storage_offers: BTreeMap<NodeId, StorageOffer>,
    pub batches: BTreeMap<BatchKey, BatchCommit>,
    pub epoch_commits: BTreeMap<EpochId, EpochCommit>,
    pub open_challenges: BTreeMap<Hash32, Challenge>,
    pub resolved_challenges: BTreeMap<Hash32, Challenge>,
    pub challenge_resolutions: BTreeMap<Hash32, ChallengeResolutionRecord>,
    pub reward_records: BTreeMap<RewardKey, RewardRecord>,
    pub claimed_rewards: BTreeSet<RewardKey>,
    pub delegations: BTreeMap<DelegationKey, DelegationRecord>,
    pub unbonding_queue: Vec<UnbondingRecord>,
    pub votes: BTreeMap<VoteKey, GovernanceVoteRecord>,
    pub params_update_proposals: BTreeMap<Hash32, GovernanceParamsUpdateProposalRecord>,
    pub challenge_responses: BTreeMap<Hash32, ChallengeResponseRecord>,
    pub scheduled_protocol_params: BTreeMap<EpochId, ProtocolParams>,
    pub finalized_epochs: BTreeSet<EpochId>,
}

impl ProtocolStore for InMemoryStore {
    fn account(&self, address: &Address) -> Option<&AccountState> {
        self.accounts.get(address)
    }

    fn account_mut(&mut self, address: &Address) -> Option<&mut AccountState> {
        self.accounts.get_mut(address)
    }

    fn insert_account(&mut self, account: AccountState) {
        self.accounts.insert(account.address, account);
    }

    fn accounts_iter(&self) -> Vec<AccountState> {
        self.accounts.values().cloned().collect()
    }

    fn node(&self, node_id: &NodeId) -> Option<&NodeRegistry> {
        self.nodes.get(node_id)
    }

    fn node_mut(&mut self, node_id: &NodeId) -> Option<&mut NodeRegistry> {
        self.nodes.get_mut(node_id)
    }

    fn insert_node(&mut self, node: NodeRegistry) {
        self.nodes.insert(node.node_id, node);
    }

    fn storage_offer(&self, node_id: &NodeId) -> Option<&StorageOffer> {
        self.storage_offers.get(node_id)
    }

    fn insert_storage_offer(&mut self, offer: StorageOffer) {
        self.storage_offers.insert(offer.node_id, offer);
    }

    fn batch(&self, key: &BatchKey) -> Option<&BatchCommit> {
        self.batches.get(key)
    }

    fn insert_batch(&mut self, key: BatchKey, batch: BatchCommit) {
        self.batches.insert(key, batch);
    }

    fn has_any_batch_for_epoch(&self, epoch_id: EpochId) -> bool {
        self.batches
            .keys()
            .any(|(candidate, _, _)| *candidate == epoch_id)
    }

    fn epoch_commit(&self, epoch_id: &EpochId) -> Option<&EpochCommit> {
        self.epoch_commits.get(epoch_id)
    }

    fn insert_epoch_commit(&mut self, epoch_id: EpochId, commit: EpochCommit) {
        self.epoch_commits.insert(epoch_id, commit);
    }

    fn open_challenge(&self, challenge_id: &Hash32) -> Option<&Challenge> {
        self.open_challenges.get(challenge_id)
    }

    fn insert_open_challenge(&mut self, challenge_id: Hash32, challenge: Challenge) {
        self.open_challenges.insert(challenge_id, challenge);
    }

    fn remove_open_challenge(&mut self, challenge_id: &Hash32) -> Option<Challenge> {
        self.open_challenges.remove(challenge_id)
    }

    fn has_open_challenges_for_epoch(&self, epoch_id: EpochId) -> bool {
        self.open_challenges
            .values()
            .any(|challenge| challenge.epoch_id == epoch_id)
    }

    fn insert_resolved_challenge(&mut self, challenge_id: Hash32, challenge: Challenge) {
        self.resolved_challenges.insert(challenge_id, challenge);
    }

    fn challenge_resolution(&self, challenge_id: &Hash32) -> Option<&ChallengeResolutionRecord> {
        self.challenge_resolutions.get(challenge_id)
    }

    fn insert_challenge_resolution(
        &mut self,
        challenge_id: Hash32,
        resolution: ChallengeResolutionRecord,
    ) {
        self.challenge_resolutions.insert(challenge_id, resolution);
    }

    fn reward_record(&self, key: &RewardKey) -> Option<&RewardRecord> {
        self.reward_records.get(key)
    }

    fn insert_reward_record(&mut self, key: RewardKey, reward: RewardRecord) {
        self.reward_records.insert(key, reward);
    }

    fn is_reward_claimed(&self, key: &RewardKey) -> bool {
        self.claimed_rewards.contains(key)
    }

    fn mark_reward_claimed(&mut self, key: RewardKey) {
        self.claimed_rewards.insert(key);
    }

    fn delegation(&self, key: &DelegationKey) -> Option<&DelegationRecord> {
        self.delegations.get(key)
    }

    fn insert_delegation(&mut self, key: DelegationKey, delegation: DelegationRecord) {
        self.delegations.insert(key, delegation);
    }

    fn remove_delegation(&mut self, key: &DelegationKey) -> Option<DelegationRecord> {
        self.delegations.remove(key)
    }

    fn queue_unbonding(&mut self, request: UnbondingRecord) {
        self.unbonding_queue.push(request);
    }

    fn drain_mature_unbondings(&mut self, current_height: Height) -> Vec<UnbondingRecord> {
        let pending = std::mem::take(&mut self.unbonding_queue);
        let mut matured = Vec::new();
        let mut remaining = Vec::new();

        for request in pending {
            if request.unlock_height <= current_height {
                matured.push(request);
            } else {
                remaining.push(request);
            }
        }

        self.unbonding_queue = remaining;
        matured
    }

    fn vote_record(&self, key: &VoteKey) -> Option<&GovernanceVoteRecord> {
        self.votes.get(key)
    }

    fn insert_vote_record(&mut self, key: VoteKey, vote: GovernanceVoteRecord) {
        self.votes.insert(key, vote);
    }

    fn vote_records_for_proposal(&self, proposal_id: &Hash32) -> Vec<GovernanceVoteRecord> {
        self.votes
            .iter()
            .filter(|((candidate_proposal_id, _), _)| candidate_proposal_id == proposal_id)
            .map(|(_, vote)| vote.clone())
            .collect()
    }

    fn params_update_proposal(
        &self,
        proposal_id: &Hash32,
    ) -> Option<&GovernanceParamsUpdateProposalRecord> {
        self.params_update_proposals.get(proposal_id)
    }

    fn params_update_proposals_iter(&self) -> Vec<GovernanceParamsUpdateProposalRecord> {
        self.params_update_proposals.values().cloned().collect()
    }

    fn params_update_proposal_mut(
        &mut self,
        proposal_id: &Hash32,
    ) -> Option<&mut GovernanceParamsUpdateProposalRecord> {
        self.params_update_proposals.get_mut(proposal_id)
    }

    fn insert_params_update_proposal(
        &mut self,
        proposal_id: Hash32,
        proposal: GovernanceParamsUpdateProposalRecord,
    ) {
        self.params_update_proposals.insert(proposal_id, proposal);
    }

    fn challenge_response(&self, challenge_id: &Hash32) -> Option<&ChallengeResponseRecord> {
        self.challenge_responses.get(challenge_id)
    }

    fn insert_challenge_response(
        &mut self,
        challenge_id: Hash32,
        response: ChallengeResponseRecord,
    ) {
        self.challenge_responses.insert(challenge_id, response);
    }

    fn scheduled_protocol_params(&self, epoch_id: &EpochId) -> Option<&ProtocolParams> {
        self.scheduled_protocol_params.get(epoch_id)
    }

    fn insert_scheduled_protocol_params(&mut self, epoch_id: EpochId, params: ProtocolParams) {
        self.scheduled_protocol_params.insert(epoch_id, params);
    }

    fn take_scheduled_protocol_params(&mut self, epoch_id: &EpochId) -> Option<ProtocolParams> {
        self.scheduled_protocol_params.remove(epoch_id)
    }

    fn is_epoch_finalized(&self, epoch_id: EpochId) -> bool {
        self.finalized_epochs.contains(&epoch_id)
    }

    fn mark_epoch_finalized(&mut self, epoch_id: EpochId) {
        self.finalized_epochs.insert(epoch_id);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistentStoreStub {
    pub path: PathBuf,
    pub inner: InMemoryStore,
}

impl PersistentStoreStub {
    pub fn open(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        let inner = if path.exists() {
            let bytes = fs::read(&path)?;
            InMemoryStore::try_from_slice(&bytes).map_err(|err| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("failed to decode store snapshot: {err}"),
                )
            })?
        } else {
            InMemoryStore::default()
        };

        Ok(Self { path, inner })
    }

    pub fn flush(&self) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }

        let bytes = borsh::to_vec(&self.inner).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to encode store snapshot: {err}"),
            )
        })?;
        let tmp_path = self.path.with_extension("tmp");
        fs::write(&tmp_path, bytes)?;
        fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }
}

impl ProtocolStore for PersistentStoreStub {
    fn account(&self, address: &Address) -> Option<&AccountState> {
        self.inner.account(address)
    }

    fn account_mut(&mut self, address: &Address) -> Option<&mut AccountState> {
        self.inner.account_mut(address)
    }

    fn insert_account(&mut self, account: AccountState) {
        self.inner.insert_account(account)
    }

    fn accounts_iter(&self) -> Vec<AccountState> {
        self.inner.accounts_iter()
    }

    fn node(&self, node_id: &NodeId) -> Option<&NodeRegistry> {
        self.inner.node(node_id)
    }

    fn node_mut(&mut self, node_id: &NodeId) -> Option<&mut NodeRegistry> {
        self.inner.node_mut(node_id)
    }

    fn insert_node(&mut self, node: NodeRegistry) {
        self.inner.insert_node(node)
    }

    fn storage_offer(&self, node_id: &NodeId) -> Option<&StorageOffer> {
        self.inner.storage_offer(node_id)
    }

    fn insert_storage_offer(&mut self, offer: StorageOffer) {
        self.inner.insert_storage_offer(offer)
    }

    fn batch(&self, key: &BatchKey) -> Option<&BatchCommit> {
        self.inner.batch(key)
    }

    fn insert_batch(&mut self, key: BatchKey, batch: BatchCommit) {
        self.inner.insert_batch(key, batch)
    }

    fn has_any_batch_for_epoch(&self, epoch_id: EpochId) -> bool {
        self.inner.has_any_batch_for_epoch(epoch_id)
    }

    fn epoch_commit(&self, epoch_id: &EpochId) -> Option<&EpochCommit> {
        self.inner.epoch_commit(epoch_id)
    }

    fn insert_epoch_commit(&mut self, epoch_id: EpochId, commit: EpochCommit) {
        self.inner.insert_epoch_commit(epoch_id, commit)
    }

    fn open_challenge(&self, challenge_id: &Hash32) -> Option<&Challenge> {
        self.inner.open_challenge(challenge_id)
    }

    fn insert_open_challenge(&mut self, challenge_id: Hash32, challenge: Challenge) {
        self.inner.insert_open_challenge(challenge_id, challenge)
    }

    fn remove_open_challenge(&mut self, challenge_id: &Hash32) -> Option<Challenge> {
        self.inner.remove_open_challenge(challenge_id)
    }

    fn has_open_challenges_for_epoch(&self, epoch_id: EpochId) -> bool {
        self.inner.has_open_challenges_for_epoch(epoch_id)
    }

    fn insert_resolved_challenge(&mut self, challenge_id: Hash32, challenge: Challenge) {
        self.inner
            .insert_resolved_challenge(challenge_id, challenge)
    }

    fn challenge_resolution(&self, challenge_id: &Hash32) -> Option<&ChallengeResolutionRecord> {
        self.inner.challenge_resolution(challenge_id)
    }

    fn insert_challenge_resolution(
        &mut self,
        challenge_id: Hash32,
        resolution: ChallengeResolutionRecord,
    ) {
        self.inner
            .insert_challenge_resolution(challenge_id, resolution)
    }

    fn reward_record(&self, key: &RewardKey) -> Option<&RewardRecord> {
        self.inner.reward_record(key)
    }

    fn insert_reward_record(&mut self, key: RewardKey, reward: RewardRecord) {
        self.inner.insert_reward_record(key, reward)
    }

    fn is_reward_claimed(&self, key: &RewardKey) -> bool {
        self.inner.is_reward_claimed(key)
    }

    fn mark_reward_claimed(&mut self, key: RewardKey) {
        self.inner.mark_reward_claimed(key)
    }

    fn delegation(&self, key: &DelegationKey) -> Option<&DelegationRecord> {
        self.inner.delegation(key)
    }

    fn insert_delegation(&mut self, key: DelegationKey, delegation: DelegationRecord) {
        self.inner.insert_delegation(key, delegation)
    }

    fn remove_delegation(&mut self, key: &DelegationKey) -> Option<DelegationRecord> {
        self.inner.remove_delegation(key)
    }

    fn queue_unbonding(&mut self, request: UnbondingRecord) {
        self.inner.queue_unbonding(request)
    }

    fn drain_mature_unbondings(&mut self, current_height: Height) -> Vec<UnbondingRecord> {
        self.inner.drain_mature_unbondings(current_height)
    }

    fn vote_record(&self, key: &VoteKey) -> Option<&GovernanceVoteRecord> {
        self.inner.vote_record(key)
    }

    fn insert_vote_record(&mut self, key: VoteKey, vote: GovernanceVoteRecord) {
        self.inner.insert_vote_record(key, vote)
    }

    fn vote_records_for_proposal(&self, proposal_id: &Hash32) -> Vec<GovernanceVoteRecord> {
        self.inner.vote_records_for_proposal(proposal_id)
    }

    fn params_update_proposal(
        &self,
        proposal_id: &Hash32,
    ) -> Option<&GovernanceParamsUpdateProposalRecord> {
        self.inner.params_update_proposal(proposal_id)
    }

    fn params_update_proposals_iter(&self) -> Vec<GovernanceParamsUpdateProposalRecord> {
        self.inner.params_update_proposals_iter()
    }

    fn params_update_proposal_mut(
        &mut self,
        proposal_id: &Hash32,
    ) -> Option<&mut GovernanceParamsUpdateProposalRecord> {
        self.inner.params_update_proposal_mut(proposal_id)
    }

    fn insert_params_update_proposal(
        &mut self,
        proposal_id: Hash32,
        proposal: GovernanceParamsUpdateProposalRecord,
    ) {
        self.inner
            .insert_params_update_proposal(proposal_id, proposal)
    }

    fn challenge_response(&self, challenge_id: &Hash32) -> Option<&ChallengeResponseRecord> {
        self.inner.challenge_response(challenge_id)
    }

    fn insert_challenge_response(
        &mut self,
        challenge_id: Hash32,
        response: ChallengeResponseRecord,
    ) {
        self.inner.insert_challenge_response(challenge_id, response)
    }

    fn scheduled_protocol_params(&self, epoch_id: &EpochId) -> Option<&ProtocolParams> {
        self.inner.scheduled_protocol_params(epoch_id)
    }

    fn insert_scheduled_protocol_params(&mut self, epoch_id: EpochId, params: ProtocolParams) {
        self.inner
            .insert_scheduled_protocol_params(epoch_id, params)
    }

    fn take_scheduled_protocol_params(&mut self, epoch_id: &EpochId) -> Option<ProtocolParams> {
        self.inner.take_scheduled_protocol_params(epoch_id)
    }

    fn is_epoch_finalized(&self, epoch_id: EpochId) -> bool {
        self.inner.is_epoch_finalized(epoch_id)
    }

    fn mark_epoch_finalized(&mut self, epoch_id: EpochId) {
        self.inner.mark_epoch_finalized(epoch_id)
    }
}
