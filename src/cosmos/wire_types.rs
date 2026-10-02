//! Wire-only types for the `pole.chain.pole.v1` proto3 package.
//!
//! These types are 1:1 mirrors of the proto messages declared in
//! `chain/proto/pole/chain/pole/v1/{tx,state}.proto`. They live
//! separately from the off-chain `records::*` / `params::ProtocolParams`
//! types so the chain-bridge wire encoder doesn't have to know about
//! off-chain modeling details (subslots, GVS tier weighting, etc.).
//!
//! ## Mapping discipline
//!
//! - Field names mirror proto3 field names (snake_case), in proto field
//!   number order.
//! - Hex-encoded byte arrays (32-byte hashes, signatures, public keys)
//!   are exposed as `String` (lowercase hex).
//! - Bech32-encoded addresses are exposed as `String`.
//! - Enums have no `Unspecified` variant (proto3 reserves 0); the
//!   encoder maps each variant up by 1.
//! - `submitted_at_height`, `opened_at_height`, `bonded_tokens`,
//!   `challenge_open_height` and similar fields are emitted by the
//!   encoder even though the chain overwrites them on receipt —
//!   omitting them would risk proto3 missing-field warnings on
//!   defensive handlers.
//!
//! ## Follow-up
//!
//! A future `From<&records::AggregateRecord> for AggregateRecordWire`
//! (and equivalents) adapter layer will let off-chain code construct
//! these wire types without copy-pasting fields.

use serde::{Deserialize, Serialize};

// --- MerkleCommitment -----------------------------------------------------

/// `pole.chain.pole.v1.MerkleCommitment` — used by BatchCommit,
/// EpochCommit, and ChallengeEvidenceRef.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MerkleCommitmentWire {
    /// Hex-encoded root hash (string).
    pub root: String,
    /// Number of leaves committed.
    pub leaf_count: u32,
}

// --- NodeRole ------------------------------------------------------------

/// `pole.chain.pole.v1.NodeRole` — proto reserves 0 for UNSPECIFIED,
/// so the variant-to-i32 mapping shifts up by 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeRoleWire {
    /// `NODE_ROLE_PLAYER = 1`
    Player,
    /// `NODE_ROLE_SERVICE = 2`
    Service,
    /// `NODE_ROLE_COORDINATOR = 3`
    Coordinator,
}

/// Map `NodeRoleWire` to its proto enum i32 value.
pub fn node_role_to_proto(role: NodeRoleWire) -> i32 {
    match role {
        NodeRoleWire::Player => 1,
        NodeRoleWire::Service => 2,
        NodeRoleWire::Coordinator => 3,
    }
}

// --- NodeCapabilitySet ---------------------------------------------------

/// `pole.chain.pole.v1.NodeCapabilitySet` — four booleans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NodeCapabilitySetWire {
    pub collect: bool,
    pub store: bool,
    pub verify: bool,
    pub propose: bool,
}

// --- NodeRecord ----------------------------------------------------------

/// `pole.chain.pole.v1.NodeRecord` — the operator's stake-bearing
/// identity. `bonded_tokens` is overwritten by the chain from the
/// validator set on receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeRecordWire {
    pub operator_address: String,
    pub reward_address: String,
    pub consensus_address: String,
    pub role: NodeRoleWire,
    pub capabilities: NodeCapabilitySetWire,
    pub active: bool,
    pub bonded_tokens: u64,
    pub last_updated_epoch: u64,
    /// Player collectors may take the verify capability without a
    /// separate stake.
    #[serde(default)]
    pub is_player: bool,
}

// --- AggregateRecord -----------------------------------------------------

/// `pole.chain.pole.v1.AggregateRecord` — 4 fields, far thinner than
/// the off-chain `records::AggregateRecord` (which carries GVS tier
/// weighting). The wire form only needs what the chain stores.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggregateRecordWire {
    pub epoch_id: u64,
    pub app_id: u32,
    pub total_weight_units: u64,
    pub player_count: u64,
}

// --- BatchCommit ---------------------------------------------------------

/// `pole.chain.pole.v1.BatchCommit`. `submitted_at_height` is
/// overwritten by the chain from `ctx.BlockHeight()`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchCommitWire {
    pub epoch_id: u64,
    pub collector_address: String,
    pub slot_start: u64,
    pub slot_end: u64,
    pub batch: MerkleCommitmentWire,
    pub payload_cid: String,
    pub observation_count: u32,
    pub submitted_at_height: i64,
}

// --- PlaySession / PlayHeartbeat / WitnessAttestation --------------------

/// `pole.chain.pole.v1.PlaySession` — a node's signed claim that it played
/// a mapped game for `play_seconds` during one slot. `submitted_at_height`
/// is overwritten by the chain from `ctx.BlockHeight()`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaySessionWire {
    pub session_id_hex: String,
    pub node_address: String,
    pub app_id: u32,
    pub epoch_id: u64,
    pub slot_id: u64,
    pub play_seconds: u64,
    pub collector_address: String,
    pub observation_cid: String,
    pub observed_players: u64,
    pub submitted_at_height: i64,
    pub session_signature: String,
}

/// `pole.chain.pole.v1.PlayHeartbeat` — one signed liveness proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayHeartbeatWire {
    pub session_id_hex: String,
    pub node_address: String,
    pub bucket_index: u64,
    pub signed_at_millis: i64,
    pub heartbeat_signature: String,
}

/// `pole.chain.pole.v1.WitnessAttestation` — an independent node's
/// corroboration, carrying the witness's own observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitnessAttestationWire {
    pub session_id_hex: String,
    pub witness_address: String,
    pub observed_play_seconds: u64,
    pub witness_observation_cid: String,
    pub observed_players: u64,
    pub attested_at_height: i64,
    pub witness_signature: String,
}

/// `pole.chain.pole.v1.RewardRecord` — the wire form of a reward record, as
/// submitted by `MsgSubmitRewardRecords`. The chain recomputes the rewards
/// root from these and requires it to match the committed root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewardRecordWire {
    pub epoch_id: u64,
    pub recipient: String,
    pub player_reward: u64,
    pub collect_reward: u64,
    pub store_reward: u64,
    pub verify_reward: u64,
    pub propose_reward: u64,
    pub slash_debit: u64,
    pub net_reward: u64,
}

// --- ReplicaReceipt ------------------------------------------------------

/// `pole.chain.pole.v1.ReplicaReceipt` — 6 fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicaReceiptWire {
    pub epoch_id: u64,
    pub payload_cid: String,
    pub storer_address: String,
    pub retention_until_epoch: u64,
    pub receipt_signature: String,
    pub receipt_hash_hex: String,
}

// --- EpochCommit ---------------------------------------------------------

/// `pole.chain.pole.v1.EpochCommit` — 12 fields, including 5 nested
/// `MerkleCommitment` slots. `challenge_open_height` is auto-set by
/// the chain if zero.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpochCommitWire {
    pub epoch_id: u64,
    pub accepted_batches: MerkleCommitmentWire,
    pub observations: MerkleCommitmentWire,
    pub aggregates: MerkleCommitmentWire,
    pub rewards: MerkleCommitmentWire,
    pub availability: MerkleCommitmentWire,
    pub randomness_seed_hex: String,
    pub proposer_address: String,
    pub challenge_open_height: i64,
    pub challenge_deadline_height: i64,
    pub finalized: bool,
    pub total_network_weight_units: u64,
}

// --- GameWeightEntry -----------------------------------------------------

/// `pole.chain.pole.v1.GameWeightEntry` — 4 fields. `tier` is a
/// free-form string (typically "tier1" / "tier2" / "tier3"); the
/// chain doesn't constrain it on the wire side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameWeightEntryWire {
    pub app_id: u32,
    pub game_weight_ppm: u32,
    pub tier: String,
    pub effective_from_epoch_id: u64,
}

// --- Params --------------------------------------------------------------

/// `pole.chain.pole.v1.Params` — 23 primitive fields, flattened
/// (the off-chain `params::ProtocolParams` packs these into nested
/// `FeeParams` / `RewardParams` / `GovernanceParams` /
/// `SlashingParams` structs; the wire form is flat to match proto).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParamsWire {
    pub reward_block_duration_seconds: u64,
    pub base_hourly_reward: u64,
    pub target_network_weight_units: u64,
    pub reward_adjustment_cap_bps: u32,
    pub challenge_window_blocks: u64,
    pub min_retention_epochs: u64,
    pub player_reward_allocation_bps: u32,
    pub service_reward_allocation_bps: u32,
    pub collect_reward_bps: u32,
    pub store_reward_bps: u32,
    pub verify_reward_bps: u32,
    pub propose_reward_bps: u32,
    pub tier1_weight_ppm: u32,
    pub tier2_weight_min_ppm: u32,
    pub tier2_weight_max_ppm: u32,
    pub tier3_weight_min_ppm: u32,
    pub tier3_weight_max_ppm: u32,
    pub fee_burn_bps: u32,
    pub reward_burn_threshold: u64,
    pub reward_burn_bps: u32,
    pub governance_burn_bps: u32,
    /// Minimum number of independent verification credentials required
    /// before an epoch can be finalized (proto field 22). Omitting this
    /// on the wire zeroes the chain-side finalize verification gate.
    #[serde(default)]
    pub min_verification_count: u64,
    /// Minimum share (bps, 10000 = 100%) of verification credentials
    /// that must come from player verifiers (proto field 23).
    #[serde(default)]
    pub min_player_verifier_share_bps: u32,
    /// Minimum distinct witness attestations a play session needs before the
    /// chain accepts the claimed play time (proto field 24).
    #[serde(default)]
    pub min_witness_count: u64,
    /// Maximum relative deviation (ppm) tolerated between a play session's
    /// observed player count and a witness's own observation (proto field 25).
    #[serde(default)]
    pub min_witness_observation_tolerance_ppm: u32,
    /// Minimum distinct witness observation payloads backing a session
    /// (proto field 26). Defeats N witnesses copying one observation.
    #[serde(default)]
    pub min_distinct_observations: u64,
    /// Share (bps) of a session's player reward burned when a challenge
    /// resolves the session invalid (proto field 27).
    #[serde(default)]
    pub session_slash_bps: u32,
    /// Minimum signed play heartbeats a session needs (proto field 28).
    #[serde(default)]
    pub min_heartbeat_count: u64,
    /// Length of one heartbeat bucket in seconds (proto field 29).
    #[serde(default)]
    pub heartbeat_bucket_seconds: u64,
    /// Minimum share (bps) of the declared play window heartbeats must
    /// cover (proto field 30).
    #[serde(default)]
    pub min_heartbeat_coverage_bps: u32,
    /// Share (bps) of a challenge bond burned when a challenge fails
    /// (proto field 31).
    #[serde(default)]
    pub challenge_bond_burn_bps: u32,
}

impl Default for ParamsWire {
    /// Empty / all-zero defaults. Callers should overwrite before
    /// sending; the encoder will emit all 23 fields either way.
    fn default() -> Self {
        Self {
            reward_block_duration_seconds: 0,
            base_hourly_reward: 0,
            target_network_weight_units: 0,
            reward_adjustment_cap_bps: 0,
            challenge_window_blocks: 0,
            min_retention_epochs: 0,
            player_reward_allocation_bps: 0,
            service_reward_allocation_bps: 0,
            collect_reward_bps: 0,
            store_reward_bps: 0,
            verify_reward_bps: 0,
            propose_reward_bps: 0,
            tier1_weight_ppm: 0,
            tier2_weight_min_ppm: 0,
            tier2_weight_max_ppm: 0,
            tier3_weight_min_ppm: 0,
            tier3_weight_max_ppm: 0,
            fee_burn_bps: 0,
            reward_burn_threshold: 0,
            reward_burn_bps: 0,
            governance_burn_bps: 0,
            min_verification_count: 0,
            min_player_verifier_share_bps: 0,
            min_witness_count: 0,
            min_witness_observation_tolerance_ppm: 0,
            min_distinct_observations: 0,
            session_slash_bps: 0,
            min_heartbeat_count: 0,
            heartbeat_bucket_seconds: 0,
            min_heartbeat_coverage_bps: 0,
            challenge_bond_burn_bps: 0,
        }
    }
}
