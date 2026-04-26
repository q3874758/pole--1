use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::params::ProtocolParams;
use crate::primitives::{Address, Amount, ContentId, Hash32, NodeId, SignatureBytes, VoteChoice};
use crate::records::{BatchCommit, Challenge, EpochCommit};
use crate::EpochId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct TransferTx {
    pub from: Address,
    pub to: Address,
    pub amount: Amount,
    pub fee: Amount,
    pub nonce: u64,
    pub signature: SignatureBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct StakeTx {
    pub delegator: Address,
    pub operator: NodeId,
    pub amount: Amount,
    pub nonce: u64,
    pub signature: SignatureBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct UnbondTx {
    pub delegator: Address,
    pub operator: NodeId,
    pub amount: Amount,
    pub nonce: u64,
    pub signature: SignatureBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ClaimRewardTx {
    pub claimer: Address,
    pub epoch_id: EpochId,
    pub node_id: NodeId,
    pub amount: Amount,
    pub merkle_proof: Vec<Hash32>,
    pub nonce: u64,
    pub signature: SignatureBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct SubmitBatchTx {
    pub batch_commit: BatchCommit,
    pub signature: SignatureBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct CommitEpochTx {
    pub epoch_commit: EpochCommit,
    pub signature: SignatureBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct OpenChallengeTx {
    pub challenge: Challenge,
    pub signature: SignatureBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ChallengeResponseTx {
    pub challenge_id: Hash32,
    pub responder: NodeId,
    pub response_payload_cid: Option<ContentId>,
    pub response_hash: Option<Hash32>,
    pub signature: SignatureBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct VoteTx {
    pub proposal_id: Hash32,
    pub voter: Address,
    pub choice: VoteChoice,
    pub voting_power: Amount,
    pub nonce: u64,
    pub signature: SignatureBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ProposeProtocolParamsUpdateTx {
    pub proposal_id: Hash32,
    pub proposer: Address,
    pub effective_epoch: EpochId,
    pub params: ProtocolParams,
    pub nonce: u64,
    pub signature: SignatureBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum Transaction {
    Transfer(TransferTx),
    Stake(StakeTx),
    Unbond(UnbondTx),
    ClaimReward(ClaimRewardTx),
    SubmitBatch(SubmitBatchTx),
    CommitEpoch(CommitEpochTx),
    OpenChallenge(OpenChallengeTx),
    ChallengeResponse(ChallengeResponseTx),
    Vote(VoteTx),
    ProposeProtocolParamsUpdate(ProposeProtocolParamsUpdateTx),
}
