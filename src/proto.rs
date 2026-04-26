use std::convert::TryFrom;

use borsh::BorshDeserialize;
use prost::{Enumeration, Message};

use crate::params::ProtocolParams;
use crate::primitives::{
    ActivitySourceKind, ChallengeKind, ChallengeState, Hash32, MerkleCommitment,
};
use crate::records::{
    BatchCommit, Challenge, ChallengeEvidenceRef, EpochCommit, ObservationRecord,
};
use crate::transactions::{
    ChallengeResponseTx, CommitEpochTx, OpenChallengeTx, ProposeProtocolParamsUpdateTx, StakeTx,
    SubmitBatchTx, TransferTx, UnbondTx, VoteTx,
};
use crate::VoteChoice;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtoConversionError {
    InvalidLength {
        field: &'static str,
        expected: usize,
        actual: usize,
    },
    InvalidEnum {
        field: &'static str,
        value: i32,
    },
}

fn vec_to_fixed_32(bytes: Vec<u8>, field: &'static str) -> Result<[u8; 32], ProtoConversionError> {
    let actual = bytes.len();
    bytes
        .try_into()
        .map_err(|_| ProtoConversionError::InvalidLength {
            field,
            expected: 32,
            actual,
        })
}

fn opt_vec_to_fixed_32(
    bytes: Option<Vec<u8>>,
    field: &'static str,
) -> Result<Option<[u8; 32]>, ProtoConversionError> {
    bytes.map(|value| vec_to_fixed_32(value, field)).transpose()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Enumeration)]
#[repr(i32)]
pub enum ProtoActivitySourceKind {
    Steam = 0,
    Epic = 1,
    Ea = 2,
    Gog = 3,
    Community = 4,
}

impl From<ActivitySourceKind> for ProtoActivitySourceKind {
    fn from(value: ActivitySourceKind) -> Self {
        match value {
            ActivitySourceKind::Steam => Self::Steam,
            ActivitySourceKind::Epic => Self::Epic,
            ActivitySourceKind::Ea => Self::Ea,
            ActivitySourceKind::Gog => Self::Gog,
            ActivitySourceKind::Community => Self::Community,
        }
    }
}

impl TryFrom<i32> for ActivitySourceKind {
    type Error = ProtoConversionError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match ProtoActivitySourceKind::try_from(value).ok() {
            Some(ProtoActivitySourceKind::Steam) => Ok(Self::Steam),
            Some(ProtoActivitySourceKind::Epic) => Ok(Self::Epic),
            Some(ProtoActivitySourceKind::Ea) => Ok(Self::Ea),
            Some(ProtoActivitySourceKind::Gog) => Ok(Self::Gog),
            Some(ProtoActivitySourceKind::Community) => Ok(Self::Community),
            None => Err(ProtoConversionError::InvalidEnum {
                field: "source_kind",
                value,
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Enumeration)]
#[repr(i32)]
pub enum ProtoChallengeKind {
    BadBatch = 0,
    Omission = 1,
    BadAggregate = 2,
    BadReward = 3,
    BadStorage = 4,
}

impl From<ChallengeKind> for ProtoChallengeKind {
    fn from(value: ChallengeKind) -> Self {
        match value {
            ChallengeKind::BadBatch => Self::BadBatch,
            ChallengeKind::Omission => Self::Omission,
            ChallengeKind::BadAggregate => Self::BadAggregate,
            ChallengeKind::BadReward => Self::BadReward,
            ChallengeKind::BadStorage => Self::BadStorage,
        }
    }
}

impl TryFrom<i32> for ChallengeKind {
    type Error = ProtoConversionError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match ProtoChallengeKind::try_from(value).ok() {
            Some(ProtoChallengeKind::BadBatch) => Ok(Self::BadBatch),
            Some(ProtoChallengeKind::Omission) => Ok(Self::Omission),
            Some(ProtoChallengeKind::BadAggregate) => Ok(Self::BadAggregate),
            Some(ProtoChallengeKind::BadReward) => Ok(Self::BadReward),
            Some(ProtoChallengeKind::BadStorage) => Ok(Self::BadStorage),
            None => Err(ProtoConversionError::InvalidEnum {
                field: "kind",
                value,
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Enumeration)]
#[repr(i32)]
pub enum ProtoChallengeState {
    Open = 0,
    Responded = 1,
    Succeeded = 2,
    Rejected = 3,
    Expired = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Enumeration)]
#[repr(i32)]
pub enum ProtoVoteChoice {
    Yes = 0,
    No = 1,
    Abstain = 2,
}

impl From<VoteChoice> for ProtoVoteChoice {
    fn from(value: VoteChoice) -> Self {
        match value {
            VoteChoice::Yes => Self::Yes,
            VoteChoice::No => Self::No,
            VoteChoice::Abstain => Self::Abstain,
        }
    }
}

impl TryFrom<i32> for VoteChoice {
    type Error = ProtoConversionError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match ProtoVoteChoice::try_from(value).ok() {
            Some(ProtoVoteChoice::Yes) => Ok(Self::Yes),
            Some(ProtoVoteChoice::No) => Ok(Self::No),
            Some(ProtoVoteChoice::Abstain) => Ok(Self::Abstain),
            None => Err(ProtoConversionError::InvalidEnum {
                field: "vote_choice",
                value,
            }),
        }
    }
}

impl From<ChallengeState> for ProtoChallengeState {
    fn from(value: ChallengeState) -> Self {
        match value {
            ChallengeState::Open => Self::Open,
            ChallengeState::Responded => Self::Responded,
            ChallengeState::Succeeded => Self::Succeeded,
            ChallengeState::Rejected => Self::Rejected,
            ChallengeState::Expired => Self::Expired,
        }
    }
}

impl TryFrom<i32> for ChallengeState {
    type Error = ProtoConversionError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match ProtoChallengeState::try_from(value).ok() {
            Some(ProtoChallengeState::Open) => Ok(Self::Open),
            Some(ProtoChallengeState::Responded) => Ok(Self::Responded),
            Some(ProtoChallengeState::Succeeded) => Ok(Self::Succeeded),
            Some(ProtoChallengeState::Rejected) => Ok(Self::Rejected),
            Some(ProtoChallengeState::Expired) => Ok(Self::Expired),
            None => Err(ProtoConversionError::InvalidEnum {
                field: "state",
                value,
            }),
        }
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct ProtoMerkleCommitment {
    #[prost(bytes = "vec", tag = "1")]
    pub root: Vec<u8>,
    #[prost(uint32, tag = "2")]
    pub leaf_count: u32,
}

impl From<MerkleCommitment> for ProtoMerkleCommitment {
    fn from(value: MerkleCommitment) -> Self {
        Self {
            root: value.root.to_vec(),
            leaf_count: value.leaf_count,
        }
    }
}

impl TryFrom<ProtoMerkleCommitment> for MerkleCommitment {
    type Error = ProtoConversionError;

    fn try_from(value: ProtoMerkleCommitment) -> Result<Self, Self::Error> {
        Ok(Self {
            root: vec_to_fixed_32(value.root, "root")?,
            leaf_count: value.leaf_count,
        })
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct ProtoObservationRecord {
    #[prost(uint64, tag = "1")]
    pub epoch_id: u64,
    #[prost(uint64, tag = "2")]
    pub slot_id: u64,
    #[prost(uint32, tag = "3")]
    pub app_id: u32,
    #[prost(enumeration = "ProtoActivitySourceKind", tag = "4")]
    pub source_kind: i32,
    #[prost(uint32, tag = "5")]
    pub source_confidence_ppm: u32,
    #[prost(uint64, tag = "6")]
    pub observed_players: u64,
    #[prost(uint64, tag = "7")]
    pub observed_at_millis: u64,
    #[prost(bytes = "vec", tag = "8")]
    pub collector_id: Vec<u8>,
    #[prost(string, tag = "9")]
    pub raw_body_cid: String,
    #[prost(bytes = "vec", tag = "10")]
    pub raw_body_hash: Vec<u8>,
    #[prost(bytes = "vec", tag = "11")]
    pub collector_signature: Vec<u8>,
}

impl From<ObservationRecord> for ProtoObservationRecord {
    fn from(value: ObservationRecord) -> Self {
        Self {
            epoch_id: value.epoch_id,
            slot_id: value.slot_id,
            app_id: value.app_id,
            source_kind: ProtoActivitySourceKind::from(value.source_kind) as i32,
            source_confidence_ppm: value.source_confidence_ppm,
            observed_players: value.observed_players,
            observed_at_millis: value.observed_at_millis,
            collector_id: value.collector_id.to_vec(),
            raw_body_cid: value.raw_body_cid,
            raw_body_hash: value.raw_body_hash.to_vec(),
            collector_signature: value.collector_signature,
        }
    }
}

impl TryFrom<ProtoObservationRecord> for ObservationRecord {
    type Error = ProtoConversionError;

    fn try_from(value: ProtoObservationRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            epoch_id: value.epoch_id,
            slot_id: value.slot_id,
            app_id: value.app_id,
            source_kind: value.source_kind.try_into()?,
            source_confidence_ppm: value.source_confidence_ppm,
            observed_players: value.observed_players,
            observed_at_millis: value.observed_at_millis,
            collector_id: vec_to_fixed_32(value.collector_id, "collector_id")?,
            raw_body_cid: value.raw_body_cid,
            raw_body_hash: vec_to_fixed_32(value.raw_body_hash, "raw_body_hash")?,
            collector_signature: value.collector_signature,
        })
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct ProtoBatchCommit {
    #[prost(uint64, tag = "1")]
    pub epoch_id: u64,
    #[prost(bytes = "vec", tag = "2")]
    pub collector_id: Vec<u8>,
    #[prost(uint64, tag = "3")]
    pub slot_start: u64,
    #[prost(uint64, tag = "4")]
    pub slot_end: u64,
    #[prost(message, optional, tag = "5")]
    pub batch: Option<ProtoMerkleCommitment>,
    #[prost(string, tag = "6")]
    pub payload_cid: String,
    #[prost(uint32, tag = "7")]
    pub obs_count: u32,
    #[prost(uint64, tag = "8")]
    pub submitted_at_height: u64,
}

impl From<BatchCommit> for ProtoBatchCommit {
    fn from(value: BatchCommit) -> Self {
        Self {
            epoch_id: value.epoch_id,
            collector_id: value.collector_id.to_vec(),
            slot_start: value.slot_start,
            slot_end: value.slot_end,
            batch: Some(value.batch.into()),
            payload_cid: value.payload_cid,
            obs_count: value.obs_count,
            submitted_at_height: value.submitted_at_height,
        }
    }
}

impl TryFrom<ProtoBatchCommit> for BatchCommit {
    type Error = ProtoConversionError;

    fn try_from(value: ProtoBatchCommit) -> Result<Self, Self::Error> {
        Ok(Self {
            epoch_id: value.epoch_id,
            collector_id: vec_to_fixed_32(value.collector_id, "collector_id")?,
            slot_start: value.slot_start,
            slot_end: value.slot_end,
            batch: value
                .batch
                .ok_or(ProtoConversionError::InvalidLength {
                    field: "batch",
                    expected: 1,
                    actual: 0,
                })?
                .try_into()?,
            payload_cid: value.payload_cid,
            obs_count: value.obs_count,
            submitted_at_height: value.submitted_at_height,
        })
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct ProtoEpochCommit {
    #[prost(uint64, tag = "1")]
    pub epoch_id: u64,
    #[prost(message, optional, tag = "2")]
    pub accepted_batches: Option<ProtoMerkleCommitment>,
    #[prost(message, optional, tag = "3")]
    pub observations: Option<ProtoMerkleCommitment>,
    #[prost(message, optional, tag = "4")]
    pub aggregates: Option<ProtoMerkleCommitment>,
    #[prost(message, optional, tag = "5")]
    pub rewards: Option<ProtoMerkleCommitment>,
    #[prost(message, optional, tag = "6")]
    pub availability: Option<ProtoMerkleCommitment>,
    #[prost(bytes = "vec", tag = "7")]
    pub randomness_seed: Vec<u8>,
    #[prost(bytes = "vec", tag = "8")]
    pub proposer_id: Vec<u8>,
    #[prost(uint64, tag = "9")]
    pub challenge_open_height: u64,
    #[prost(uint64, tag = "10")]
    pub challenge_deadline_height: u64,
}

impl From<EpochCommit> for ProtoEpochCommit {
    fn from(value: EpochCommit) -> Self {
        Self {
            epoch_id: value.epoch_id,
            accepted_batches: Some(value.accepted_batches.into()),
            observations: Some(value.observations.into()),
            aggregates: Some(value.aggregates.into()),
            rewards: Some(value.rewards.into()),
            availability: Some(value.availability.into()),
            randomness_seed: value.randomness_seed.to_vec(),
            proposer_id: value.proposer_id.to_vec(),
            challenge_open_height: value.challenge_open_height,
            challenge_deadline_height: value.challenge_deadline_height,
        }
    }
}

impl TryFrom<ProtoEpochCommit> for EpochCommit {
    type Error = ProtoConversionError;

    fn try_from(value: ProtoEpochCommit) -> Result<Self, Self::Error> {
        Ok(Self {
            epoch_id: value.epoch_id,
            accepted_batches: value
                .accepted_batches
                .ok_or(ProtoConversionError::InvalidLength {
                    field: "accepted_batches",
                    expected: 1,
                    actual: 0,
                })?
                .try_into()?,
            observations: value
                .observations
                .ok_or(ProtoConversionError::InvalidLength {
                    field: "observations",
                    expected: 1,
                    actual: 0,
                })?
                .try_into()?,
            aggregates: value
                .aggregates
                .ok_or(ProtoConversionError::InvalidLength {
                    field: "aggregates",
                    expected: 1,
                    actual: 0,
                })?
                .try_into()?,
            rewards: value
                .rewards
                .ok_or(ProtoConversionError::InvalidLength {
                    field: "rewards",
                    expected: 1,
                    actual: 0,
                })?
                .try_into()?,
            availability: value
                .availability
                .ok_or(ProtoConversionError::InvalidLength {
                    field: "availability",
                    expected: 1,
                    actual: 0,
                })?
                .try_into()?,
            randomness_seed: vec_to_fixed_32(value.randomness_seed, "randomness_seed")?,
            proposer_id: vec_to_fixed_32(value.proposer_id, "proposer_id")?,
            challenge_open_height: value.challenge_open_height,
            challenge_deadline_height: value.challenge_deadline_height,
        })
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct ProtoChallengeEvidenceRef {
    #[prost(bytes = "vec", optional, tag = "1")]
    pub batch_root: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "2")]
    pub aggregate_root: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "3")]
    pub reward_root: Option<Vec<u8>>,
    #[prost(string, optional, tag = "4")]
    pub payload_cid: Option<String>,
    #[prost(bytes = "vec", repeated, tag = "5")]
    pub merkle_proof: Vec<Vec<u8>>,
}

impl From<ChallengeEvidenceRef> for ProtoChallengeEvidenceRef {
    fn from(value: ChallengeEvidenceRef) -> Self {
        Self {
            batch_root: value.batch_root.map(|v| v.to_vec()),
            aggregate_root: value.aggregate_root.map(|v| v.to_vec()),
            reward_root: value.reward_root.map(|v| v.to_vec()),
            payload_cid: value.payload_cid,
            merkle_proof: value.merkle_proof.into_iter().map(|v| v.to_vec()).collect(),
        }
    }
}

impl TryFrom<ProtoChallengeEvidenceRef> for ChallengeEvidenceRef {
    type Error = ProtoConversionError;

    fn try_from(value: ProtoChallengeEvidenceRef) -> Result<Self, Self::Error> {
        Ok(Self {
            batch_root: opt_vec_to_fixed_32(value.batch_root, "batch_root")?,
            aggregate_root: opt_vec_to_fixed_32(value.aggregate_root, "aggregate_root")?,
            reward_root: opt_vec_to_fixed_32(value.reward_root, "reward_root")?,
            payload_cid: value.payload_cid,
            merkle_proof: value
                .merkle_proof
                .into_iter()
                .map(|item| vec_to_fixed_32(item, "merkle_proof"))
                .collect::<Result<Vec<Hash32>, _>>()?,
        })
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct ProtoChallenge {
    #[prost(bytes = "vec", tag = "1")]
    pub challenge_id: Vec<u8>,
    #[prost(enumeration = "ProtoChallengeKind", tag = "2")]
    pub kind: i32,
    #[prost(uint64, tag = "3")]
    pub epoch_id: u64,
    #[prost(bytes = "vec", optional, tag = "4")]
    pub target_node: Option<Vec<u8>>,
    #[prost(bytes = "vec", tag = "5")]
    pub challenger: Vec<u8>,
    #[prost(uint64, tag = "6")]
    pub bond_lo: u64,
    #[prost(uint64, tag = "7")]
    pub bond_hi: u64,
    #[prost(uint64, tag = "8")]
    pub opened_at_height: u64,
    #[prost(uint64, tag = "9")]
    pub deadline_height: u64,
    #[prost(enumeration = "ProtoChallengeState", tag = "10")]
    pub state: i32,
    #[prost(message, optional, tag = "11")]
    pub evidence: Option<ProtoChallengeEvidenceRef>,
}

fn split_amount(value: u128) -> (u64, u64) {
    (value as u64, (value >> 64) as u64)
}

fn join_amount(lo: u64, hi: u64) -> u128 {
    (lo as u128) | ((hi as u128) << 64)
}

#[derive(Clone, PartialEq, Message)]
pub struct ProtoTransferTx {
    #[prost(bytes = "vec", tag = "1")]
    pub from: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub to: Vec<u8>,
    #[prost(uint64, tag = "3")]
    pub amount_lo: u64,
    #[prost(uint64, tag = "4")]
    pub amount_hi: u64,
    #[prost(uint64, tag = "5")]
    pub fee_lo: u64,
    #[prost(uint64, tag = "6")]
    pub fee_hi: u64,
    #[prost(uint64, tag = "7")]
    pub nonce: u64,
    #[prost(bytes = "vec", tag = "8")]
    pub signature: Vec<u8>,
}

impl From<TransferTx> for ProtoTransferTx {
    fn from(value: TransferTx) -> Self {
        let (amount_lo, amount_hi) = split_amount(value.amount);
        let (fee_lo, fee_hi) = split_amount(value.fee);
        Self {
            from: value.from.to_vec(),
            to: value.to.to_vec(),
            amount_lo,
            amount_hi,
            fee_lo,
            fee_hi,
            nonce: value.nonce,
            signature: value.signature,
        }
    }
}

impl TryFrom<ProtoTransferTx> for TransferTx {
    type Error = ProtoConversionError;

    fn try_from(value: ProtoTransferTx) -> Result<Self, Self::Error> {
        Ok(Self {
            from: vec_to_fixed_32(value.from, "from")?,
            to: vec_to_fixed_32(value.to, "to")?,
            amount: join_amount(value.amount_lo, value.amount_hi),
            fee: join_amount(value.fee_lo, value.fee_hi),
            nonce: value.nonce,
            signature: value.signature,
        })
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct ProtoStakeTx {
    #[prost(bytes = "vec", tag = "1")]
    pub delegator: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub operator: Vec<u8>,
    #[prost(uint64, tag = "3")]
    pub amount_lo: u64,
    #[prost(uint64, tag = "4")]
    pub amount_hi: u64,
    #[prost(uint64, tag = "5")]
    pub nonce: u64,
    #[prost(bytes = "vec", tag = "6")]
    pub signature: Vec<u8>,
}

impl From<StakeTx> for ProtoStakeTx {
    fn from(value: StakeTx) -> Self {
        let (amount_lo, amount_hi) = split_amount(value.amount);
        Self {
            delegator: value.delegator.to_vec(),
            operator: value.operator.to_vec(),
            amount_lo,
            amount_hi,
            nonce: value.nonce,
            signature: value.signature,
        }
    }
}

impl TryFrom<ProtoStakeTx> for StakeTx {
    type Error = ProtoConversionError;

    fn try_from(value: ProtoStakeTx) -> Result<Self, Self::Error> {
        Ok(Self {
            delegator: vec_to_fixed_32(value.delegator, "delegator")?,
            operator: vec_to_fixed_32(value.operator, "operator")?,
            amount: join_amount(value.amount_lo, value.amount_hi),
            nonce: value.nonce,
            signature: value.signature,
        })
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct ProtoUnbondTx {
    #[prost(bytes = "vec", tag = "1")]
    pub delegator: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub operator: Vec<u8>,
    #[prost(uint64, tag = "3")]
    pub amount_lo: u64,
    #[prost(uint64, tag = "4")]
    pub amount_hi: u64,
    #[prost(uint64, tag = "5")]
    pub nonce: u64,
    #[prost(bytes = "vec", tag = "6")]
    pub signature: Vec<u8>,
}

impl From<UnbondTx> for ProtoUnbondTx {
    fn from(value: UnbondTx) -> Self {
        let (amount_lo, amount_hi) = split_amount(value.amount);
        Self {
            delegator: value.delegator.to_vec(),
            operator: value.operator.to_vec(),
            amount_lo,
            amount_hi,
            nonce: value.nonce,
            signature: value.signature,
        }
    }
}

impl TryFrom<ProtoUnbondTx> for UnbondTx {
    type Error = ProtoConversionError;

    fn try_from(value: ProtoUnbondTx) -> Result<Self, Self::Error> {
        Ok(Self {
            delegator: vec_to_fixed_32(value.delegator, "delegator")?,
            operator: vec_to_fixed_32(value.operator, "operator")?,
            amount: join_amount(value.amount_lo, value.amount_hi),
            nonce: value.nonce,
            signature: value.signature,
        })
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct ProtoVoteTx {
    #[prost(bytes = "vec", tag = "1")]
    pub proposal_id: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub voter: Vec<u8>,
    #[prost(enumeration = "ProtoVoteChoice", tag = "3")]
    pub choice: i32,
    #[prost(uint64, tag = "4")]
    pub voting_power_lo: u64,
    #[prost(uint64, tag = "5")]
    pub voting_power_hi: u64,
    #[prost(uint64, tag = "6")]
    pub nonce: u64,
    #[prost(bytes = "vec", tag = "7")]
    pub signature: Vec<u8>,
}

impl From<VoteTx> for ProtoVoteTx {
    fn from(value: VoteTx) -> Self {
        let (voting_power_lo, voting_power_hi) = split_amount(value.voting_power);
        Self {
            proposal_id: value.proposal_id.to_vec(),
            voter: value.voter.to_vec(),
            choice: ProtoVoteChoice::from(value.choice) as i32,
            voting_power_lo,
            voting_power_hi,
            nonce: value.nonce,
            signature: value.signature,
        }
    }
}

impl TryFrom<ProtoVoteTx> for VoteTx {
    type Error = ProtoConversionError;

    fn try_from(value: ProtoVoteTx) -> Result<Self, Self::Error> {
        Ok(Self {
            proposal_id: vec_to_fixed_32(value.proposal_id, "proposal_id")?,
            voter: vec_to_fixed_32(value.voter, "voter")?,
            choice: VoteChoice::try_from(value.choice)?,
            voting_power: join_amount(value.voting_power_lo, value.voting_power_hi),
            nonce: value.nonce,
            signature: value.signature,
        })
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct ProtoProposeProtocolParamsUpdateTx {
    #[prost(bytes = "vec", tag = "1")]
    pub proposal_id: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub proposer: Vec<u8>,
    #[prost(uint64, tag = "3")]
    pub effective_epoch: u64,
    #[prost(bytes = "vec", tag = "4")]
    pub params: Vec<u8>,
    #[prost(uint64, tag = "5")]
    pub nonce: u64,
    #[prost(bytes = "vec", tag = "6")]
    pub signature: Vec<u8>,
}

impl From<ProposeProtocolParamsUpdateTx> for ProtoProposeProtocolParamsUpdateTx {
    fn from(value: ProposeProtocolParamsUpdateTx) -> Self {
        Self {
            proposal_id: value.proposal_id.to_vec(),
            proposer: value.proposer.to_vec(),
            effective_epoch: value.effective_epoch,
            params: borsh::to_vec(&value.params).expect("protocol params should serialize"),
            nonce: value.nonce,
            signature: value.signature,
        }
    }
}

impl TryFrom<ProtoProposeProtocolParamsUpdateTx> for ProposeProtocolParamsUpdateTx {
    type Error = ProtoConversionError;

    fn try_from(value: ProtoProposeProtocolParamsUpdateTx) -> Result<Self, Self::Error> {
        let params = ProtocolParams::try_from_slice(&value.params).map_err(|_| {
            ProtoConversionError::InvalidLength {
                field: "params",
                expected: 1,
                actual: 0,
            }
        })?;
        Ok(Self {
            proposal_id: vec_to_fixed_32(value.proposal_id, "proposal_id")?,
            proposer: vec_to_fixed_32(value.proposer, "proposer")?,
            effective_epoch: value.effective_epoch,
            params,
            nonce: value.nonce,
            signature: value.signature,
        })
    }
}

impl From<Challenge> for ProtoChallenge {
    fn from(value: Challenge) -> Self {
        let (bond_lo, bond_hi) = split_amount(value.bond);
        Self {
            challenge_id: value.challenge_id.to_vec(),
            kind: ProtoChallengeKind::from(value.kind) as i32,
            epoch_id: value.epoch_id,
            target_node: value.target_node.map(|v| v.to_vec()),
            challenger: value.challenger.to_vec(),
            bond_lo,
            bond_hi,
            opened_at_height: value.opened_at_height,
            deadline_height: value.deadline_height,
            state: ProtoChallengeState::from(value.state) as i32,
            evidence: Some(value.evidence.into()),
        }
    }
}

impl TryFrom<ProtoChallenge> for Challenge {
    type Error = ProtoConversionError;

    fn try_from(value: ProtoChallenge) -> Result<Self, Self::Error> {
        Ok(Self {
            challenge_id: vec_to_fixed_32(value.challenge_id, "challenge_id")?,
            kind: ChallengeKind::try_from(value.kind)?,
            epoch_id: value.epoch_id,
            target_node: opt_vec_to_fixed_32(value.target_node, "target_node")?,
            challenger: vec_to_fixed_32(value.challenger, "challenger")?,
            bond: join_amount(value.bond_lo, value.bond_hi),
            opened_at_height: value.opened_at_height,
            deadline_height: value.deadline_height,
            state: ChallengeState::try_from(value.state)?,
            evidence: value
                .evidence
                .ok_or(ProtoConversionError::InvalidLength {
                    field: "evidence",
                    expected: 1,
                    actual: 0,
                })?
                .try_into()?,
        })
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct ProtoSubmitBatchTx {
    #[prost(message, optional, tag = "1")]
    pub batch_commit: Option<ProtoBatchCommit>,
    #[prost(bytes = "vec", tag = "2")]
    pub signature: Vec<u8>,
}

impl From<SubmitBatchTx> for ProtoSubmitBatchTx {
    fn from(value: SubmitBatchTx) -> Self {
        Self {
            batch_commit: Some(value.batch_commit.into()),
            signature: value.signature,
        }
    }
}

impl TryFrom<ProtoSubmitBatchTx> for SubmitBatchTx {
    type Error = ProtoConversionError;

    fn try_from(value: ProtoSubmitBatchTx) -> Result<Self, Self::Error> {
        Ok(Self {
            batch_commit: value
                .batch_commit
                .ok_or(ProtoConversionError::InvalidLength {
                    field: "batch_commit",
                    expected: 1,
                    actual: 0,
                })?
                .try_into()?,
            signature: value.signature,
        })
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct ProtoCommitEpochTx {
    #[prost(message, optional, tag = "1")]
    pub epoch_commit: Option<ProtoEpochCommit>,
    #[prost(bytes = "vec", tag = "2")]
    pub signature: Vec<u8>,
}

impl From<CommitEpochTx> for ProtoCommitEpochTx {
    fn from(value: CommitEpochTx) -> Self {
        Self {
            epoch_commit: Some(value.epoch_commit.into()),
            signature: value.signature,
        }
    }
}

impl TryFrom<ProtoCommitEpochTx> for CommitEpochTx {
    type Error = ProtoConversionError;

    fn try_from(value: ProtoCommitEpochTx) -> Result<Self, Self::Error> {
        Ok(Self {
            epoch_commit: value
                .epoch_commit
                .ok_or(ProtoConversionError::InvalidLength {
                    field: "epoch_commit",
                    expected: 1,
                    actual: 0,
                })?
                .try_into()?,
            signature: value.signature,
        })
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct ProtoOpenChallengeTx {
    #[prost(message, optional, tag = "1")]
    pub challenge: Option<ProtoChallenge>,
    #[prost(bytes = "vec", tag = "2")]
    pub signature: Vec<u8>,
}

impl From<OpenChallengeTx> for ProtoOpenChallengeTx {
    fn from(value: OpenChallengeTx) -> Self {
        Self {
            challenge: Some(value.challenge.into()),
            signature: value.signature,
        }
    }
}

impl TryFrom<ProtoOpenChallengeTx> for OpenChallengeTx {
    type Error = ProtoConversionError;

    fn try_from(value: ProtoOpenChallengeTx) -> Result<Self, Self::Error> {
        Ok(Self {
            challenge: value
                .challenge
                .ok_or(ProtoConversionError::InvalidLength {
                    field: "challenge",
                    expected: 1,
                    actual: 0,
                })?
                .try_into()?,
            signature: value.signature,
        })
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct ProtoChallengeResponseTx {
    #[prost(bytes = "vec", tag = "1")]
    pub challenge_id: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub responder: Vec<u8>,
    #[prost(string, optional, tag = "3")]
    pub response_payload_cid: Option<String>,
    #[prost(bytes = "vec", optional, tag = "4")]
    pub response_hash: Option<Vec<u8>>,
    #[prost(bytes = "vec", tag = "5")]
    pub signature: Vec<u8>,
}

impl From<ChallengeResponseTx> for ProtoChallengeResponseTx {
    fn from(value: ChallengeResponseTx) -> Self {
        Self {
            challenge_id: value.challenge_id.to_vec(),
            responder: value.responder.to_vec(),
            response_payload_cid: value.response_payload_cid,
            response_hash: value.response_hash.map(|hash| hash.to_vec()),
            signature: value.signature,
        }
    }
}

impl TryFrom<ProtoChallengeResponseTx> for ChallengeResponseTx {
    type Error = ProtoConversionError;

    fn try_from(value: ProtoChallengeResponseTx) -> Result<Self, Self::Error> {
        Ok(Self {
            challenge_id: vec_to_fixed_32(value.challenge_id, "challenge_id")?,
            responder: vec_to_fixed_32(value.responder, "responder")?,
            response_payload_cid: value.response_payload_cid,
            response_hash: opt_vec_to_fixed_32(value.response_hash, "response_hash")?,
            signature: value.signature,
        })
    }
}
