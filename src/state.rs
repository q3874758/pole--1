use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::primitives::{
    Address, Amount, Capability, EpochId, Hash32, Height, NodeId, NodeStatus, ServeClass,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct AccountState {
    pub address: Address,
    pub balance: Amount,
    pub staked: Amount,
    pub locked: Amount,
    pub nonce: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ReputationSnapshot {
    pub score_ppm: u32,
    pub successful_challenges: u32,
    pub failed_challenges: u32,
    pub challengeable_faults: u32,
    pub collection_successes: u64,
    pub collection_failures: u64,
    pub storage_proofs_passed: u64,
    pub storage_proofs_failed: u64,
    pub last_updated_epoch: EpochId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct NodeRegistry {
    pub node_id: NodeId,
    pub pubkey: Vec<u8>,
    pub reward_address: Address,
    pub bond: Amount,
    pub status: NodeStatus,
    pub enabled_capabilities: Vec<Capability>,
    pub reputation: ReputationSnapshot,
    pub joined_at_height: Height,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct StorageOffer {
    pub node_id: NodeId,
    pub quota_gb: u32,
    pub min_retention_epochs: u32,
    pub serve_class: ServeClass,
    pub valid_from_epoch: EpochId,
    pub max_response_millis: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct CommitteeAssignment {
    pub epoch_id: EpochId,
    pub seed: Hash32,
    pub collectors: Vec<NodeId>,
    pub verifiers: Vec<NodeId>,
    pub proposers: Vec<NodeId>,
}
