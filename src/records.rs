use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::node_gvs::GvsTier;
use crate::params::ProtocolParams;
use crate::primitives::{
    ActivitySourceKind, Address, Amount, AppId, ChallengeKind, ChallengeState, ContentId, EpochId,
    Hash32, Height, MerkleCommitment, NodeId, SignatureBytes, SlotId, UnixMillis, VoteChoice,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ObservationRecord {
    pub epoch_id: EpochId,
    pub slot_id: SlotId,
    pub app_id: AppId,
    pub source_kind: ActivitySourceKind,
    pub source_confidence_ppm: u32,
    pub observed_players: u64,
    pub observed_at_millis: UnixMillis,
    pub collector_id: NodeId,
    pub raw_body_cid: ContentId,
    pub raw_body_hash: Hash32,
    pub collector_signature: SignatureBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct BatchCommit {
    pub epoch_id: EpochId,
    pub collector_id: NodeId,
    pub slot_start: SlotId,
    pub slot_end: SlotId,
    pub batch: MerkleCommitment,
    pub payload_cid: ContentId,
    pub obs_count: u32,
    pub submitted_at_height: Height,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ReplicaReceipt {
    pub epoch_id: EpochId,
    pub payload_cid: ContentId,
    pub storer_id: NodeId,
    pub retention_until_epoch: EpochId,
    pub receipt_signature: SignatureBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct AggregateRecord {
    pub epoch_id: EpochId,
    pub slot_id: SlotId,
    pub app_id: AppId,
    pub gvs_tier: GvsTier,
    pub primary_source_kind: ActivitySourceKind,
    pub source_confidence_ppm: u32,
    pub accepted_observations: u32,
    pub median_players: u64,
    pub base_glv_microunits: u64,
    pub tier_weight_ppm: u32,
    pub time_decay_ppm: u32,
    pub coverage_bonus_ppm: u32,
    pub gvs_microunits: u64,
    pub source_batch_root: Hash32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct RewardRecord {
    pub epoch_id: EpochId,
    pub node_id: NodeId,
    #[serde(default)]
    pub player_reward: Amount,
    pub collect_reward: Amount,
    pub store_reward: Amount,
    pub verify_reward: Amount,
    pub propose_reward: Amount,
    pub slash_debit: Amount,
    pub net_reward: Amount,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct AvailabilityRecord {
    pub epoch_id: EpochId,
    pub node_id: NodeId,
    pub payload_cid: ContentId,
    pub retention_until_epoch: EpochId,
    pub receipt_hash: Hash32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct EpochCommit {
    pub epoch_id: EpochId,
    pub accepted_batches: MerkleCommitment,
    pub observations: MerkleCommitment,
    pub aggregates: MerkleCommitment,
    pub rewards: MerkleCommitment,
    pub availability: MerkleCommitment,
    pub randomness_seed: Hash32,
    pub proposer_id: NodeId,
    pub challenge_open_height: Height,
    pub challenge_deadline_height: Height,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ChallengeEvidenceRef {
    pub batch_root: Option<Hash32>,
    pub aggregate_root: Option<Hash32>,
    pub reward_root: Option<Hash32>,
    pub payload_cid: Option<ContentId>,
    pub merkle_proof: Vec<Hash32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Challenge {
    pub challenge_id: Hash32,
    pub kind: ChallengeKind,
    pub epoch_id: EpochId,
    pub target_node: Option<NodeId>,
    pub challenger: Address,
    pub bond: Amount,
    pub opened_at_height: Height,
    pub deadline_height: Height,
    pub state: ChallengeState,
    pub evidence: ChallengeEvidenceRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct DelegationRecord {
    pub delegator: Address,
    pub operator: NodeId,
    pub amount: Amount,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct UnbondingRecord {
    pub delegator: Address,
    pub operator: NodeId,
    pub amount: Amount,
    pub unlock_height: Height,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct GovernanceVoteRecord {
    pub proposal_id: Hash32,
    pub voter: Address,
    pub choice: VoteChoice,
    pub voting_power: Amount,
    pub recorded_height: Height,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum GovernanceProposalState {
    Pending,
    Scheduled,
    Activated,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum GovernanceProposalKind {
    FastParams,
    SlowParams,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct GovernanceParamsUpdateProposalRecord {
    pub proposal_id: Hash32,
    pub proposer: Address,
    pub kind: GovernanceProposalKind,
    pub effective_epoch: EpochId,
    pub submitted_height: Height,
    pub bond_amount: Amount,
    pub params_hash: Hash32,
    pub params: ProtocolParams,
    pub state: GovernanceProposalState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ChallengeResponseRecord {
    pub challenge_id: Hash32,
    pub responder: NodeId,
    pub response_payload_cid: Option<ContentId>,
    pub response_hash: Option<Hash32>,
    pub responded_at_height: Height,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum ChallengeResolutionDetails {
    Rejected,
    BadBatch {
        rejected_batch_root: Option<Hash32>,
    },
    Omission {
        omitted_batch_root: Hash32,
    },
    BadAggregate {
        corrected_aggregate_root: Option<Hash32>,
    },
    BadReward {
        corrected_reward_root: Option<Hash32>,
    },
    BadStorage {
        missing_payload_cid: Option<ContentId>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ChallengeResolutionRecord {
    pub challenge_id: Hash32,
    pub kind: ChallengeKind,
    pub slash_amount: Amount,
    pub challenger_reward: Amount,
    pub details: ChallengeResolutionDetails,
    pub resolved_at_height: Height,
}
