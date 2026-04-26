use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

pub type EpochId = u64;
pub type SlotId = u64;
pub type Height = u64;
pub type UnixMillis = u64;
pub type Amount = u128;
pub type AppId = u32;
pub type ContentId = String;
pub type Hash32 = [u8; 32];
pub type NodeId = [u8; 32];
pub type Address = [u8; 32];
pub type SignatureBytes = Vec<u8>;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum Capability {
    Collect,
    Store,
    Verify,
    Propose,
    Archive,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum NodeStatus {
    Pending,
    Active,
    Jailed,
    Tombstoned,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum ServeClass {
    BestEffort,
    ChallengeWindow,
    Archive,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum ChallengeKind {
    BadBatch,
    Omission,
    BadAggregate,
    BadReward,
    BadStorage,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum ChallengeState {
    Open,
    Responded,
    Succeeded,
    Rejected,
    Expired,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum VoteChoice {
    Yes,
    No,
    Abstain,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub enum ActivitySourceKind {
    Steam,
    Epic,
    Ea,
    Gog,
    Community,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct MerkleCommitment {
    pub root: Hash32,
    pub leaf_count: u32,
}
