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
    pub pubkey: [u8; 32],
    pub signature: SignatureBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct StakeTx {
    pub delegator: Address,
    pub operator: NodeId,
    pub amount: Amount,
    pub nonce: u64,
    pub pubkey: [u8; 32],
    pub signature: SignatureBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct UnbondTx {
    pub delegator: Address,
    pub operator: NodeId,
    pub amount: Amount,
    pub nonce: u64,
    pub pubkey: [u8; 32],
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
    pub pubkey: [u8; 32],
    pub signature: SignatureBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct SubmitBatchTx {
    pub batch_commit: BatchCommit,
    pub pubkey: [u8; 32],
    pub signature: SignatureBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct CommitEpochTx {
    pub epoch_commit: EpochCommit,
    pub pubkey: [u8; 32],
    pub signature: SignatureBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct OpenChallengeTx {
    pub challenge: Challenge,
    pub pubkey: [u8; 32],
    pub signature: SignatureBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ChallengeResponseTx {
    pub challenge_id: Hash32,
    pub responder: NodeId,
    pub response_payload_cid: Option<ContentId>,
    pub response_hash: Option<Hash32>,
    pub pubkey: [u8; 32],
    pub signature: SignatureBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct VoteTx {
    pub proposal_id: Hash32,
    pub voter: Address,
    pub choice: VoteChoice,
    pub voting_power: Amount,
    pub nonce: u64,
    pub pubkey: [u8; 32],
    pub signature: SignatureBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ProposeProtocolParamsUpdateTx {
    pub proposal_id: Hash32,
    pub proposer: Address,
    pub effective_epoch: EpochId,
    pub params: ProtocolParams,
    pub nonce: u64,
    pub pubkey: [u8; 32],
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

/// Deterministic signing payload for a transaction: Borsh serialization of the
/// transaction with its `signature` field cleared. Signers sign this payload,
/// so every field (including `pubkey` and `nonce`) is committed.
macro_rules! impl_signing_payload {
    ($($t:ty),+ $(,)?) => {
        $(
            impl $t {
                pub fn signing_payload(&self) -> Vec<u8> {
                    let mut clone = self.clone();
                    clone.signature.clear();
                    borsh::to_vec(&clone).expect("borsh serialize transaction")
                }
            }
        )+
    };
}

impl_signing_payload!(
    TransferTx,
    StakeTx,
    UnbondTx,
    ClaimRewardTx,
    SubmitBatchTx,
    CommitEpochTx,
    OpenChallengeTx,
    ChallengeResponseTx,
    VoteTx,
    ProposeProtocolParamsUpdateTx,
);
