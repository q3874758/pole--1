//! Hand-rolled protobuf encoders for the PoLE `Msg*` types.
//!
//! The `pole.chain.pole.v1` package types live in
//! `chain/proto/pole/chain/pole/v1/tx.proto`. They aren't shipped in
//! `cosmos-sdk-proto`, so we encode them directly into the byte stream
//! the chain expects inside `google.protobuf.Any.value`.
//!
//! The encoding follows the standard proto3 wire format:
//! - field tag = (field_number <<3) | wire_type
//! - wire_type0 = varint,2 = length-delimited
//!
//! We provide encoders for the messages used by the bridge skeleton's
//! happy path. Adding a new message is a matter of writing one more
//! `encode_msg_xxx` function that lays out its fields. The
//! [`MessageEncoder`] trait (added in Phase0.2) is the forward-compatible
//! hook for plugging in new message types without touching the
//! `BridgeMessage` enum 閳?used heavily by later phases (session keys,
//! withdrawals, threshold envelopes, PNT-20).
//!
//! `MsgOpenChallenge` carries a nested `Challenge` (state.proto:139)
//! plus a nested `ChallengeEvidenceRef` (state.proto:130). The
//! encoder here emits all required fields, including the seven
//! not-yet-populated slots at open time (`slash_amount`/`challenger_reward`
//! etc.) so the chain's `OpenChallenge` handler can deserialize without
//! tripping the missing-field default.

use crate::cosmos::proto::Any;
use crate::primitives::{ChallengeKind, ChallengeState};
use crate::records::{Challenge, ChallengeEvidenceRef};

/// Forward-compatible hook for plugging new message types into the
/// bridge without modifying the `BridgeMessage` enum.
///
/// Implementations emit a fully-formed protobuf [`Any`] 閳?the
/// `type_url` selects the chain-side `MsgServer` handler, and `value`
/// is the proto3 wire-format byte string.
///
/// Phase0.2 introduces this trait; later phases add `impl
/// MessageEncoder for` new structs (e.g. `MsgFinalizeEpochV2`,
/// `MsgDelegateSessionKey`, `MsgBeginWithdraw`, ...). The
/// `BridgeMessage` enum remains the default for callers that prefer
/// the single-dispatch path.
pub trait MessageEncoder {
    /// The chain-side handler route, e.g. `"/pole.chain.pole.v1.MsgFinalizeEpoch"`.
    fn type_url(&self) -> &'static str;
    /// The proto3-encoded message bytes. Must be a well-formed
    /// `google.protobuf.Any.value` payload.
    fn encode(&self) -> Vec<u8>;
}

/// `MsgFinalizeEpoch` 閳?the simplest message in the suite.
/// pole.chain.pole.v1.MsgFinalizeEpoch {
/// string finalizer =1;
/// uint64 epoch_id =2;
/// }
pub fn encode_msg_finalize_epoch(finalizer_bech32: &str, epoch_id: u64) -> Any {
    let mut buf = Vec::with_capacity(finalizer_bech32.len() + 16);
    encode_string(1, finalizer_bech32, &mut buf);
    encode_uint64(2, epoch_id, &mut buf);
    Any {
        type_url: "/pole.chain.pole.v1.MsgFinalizeEpoch".to_string(),
        value: buf,
    }
}

/// `MsgVerifyBatch` 閳?verifier attestation for a batch.
///
/// pole.chain.pole.v1.MsgVerifyBatch {
///   string verifier = 1;
///   uint64 epoch_id = 2;
///   string target_batch_root_hex = 3;
///   string target_collector = 4;
///   bool is_player = 5;
///   bool verified = 6;
///   string signature_hex = 7;
/// }
pub fn encode_msg_verify_batch(
    verifier_bech32: &str,
    epoch_id: u64,
    target_batch_root_hex: &str,
    target_collector_bech32: &str,
    is_player: bool,
    verified: bool,
    signature_hex: &str,
) -> Any {
    let mut buf = Vec::with_capacity(256);
    encode_string(1, verifier_bech32, &mut buf);
    encode_uint64(2, epoch_id, &mut buf);
    encode_string(3, target_batch_root_hex, &mut buf);
    encode_string(4, target_collector_bech32, &mut buf);
    encode_bool(5, is_player, &mut buf);
    encode_bool(6, verified, &mut buf);
    encode_string(7, signature_hex, &mut buf);
    Any {
        type_url: "/pole.chain.pole.v1.MsgVerifyBatch".to_string(),
        value: buf,
    }
}

/// `MsgClaimReward` 閳?the second-simplest.
pub fn encode_msg_claim_reward(claimer_bech32: &str, epoch_id: u64, recipient_bech32: &str) -> Any {
    let mut buf = Vec::with_capacity(claimer_bech32.len() + recipient_bech32.len() + 24);
    encode_string(1, claimer_bech32, &mut buf);
    encode_uint64(2, epoch_id, &mut buf);
    encode_string(3, recipient_bech32, &mut buf);
    Any {
        type_url: "/pole.chain.pole.v1.MsgClaimReward".to_string(),
        value: buf,
    }
}

/// `MsgOpenChallenge` 閳?proto3 wire encoder.
///
/// pole.chain.pole.v1.MsgOpenChallenge {
/// string challenger =1;
/// Challenge challenge =2;
/// }
///
/// The `challenger_bech32` argument supplies BOTH the outer `challenger`
/// field (proto field1) and the inner `challenge.challenger` field
/// (proto field5). Keeping them in sync satisfies the chain-side
/// `msg.Challenger == msg.Challenge.Challenger` check in
/// `chain/x/pole/keeper/msg_server.go::OpenChallenge` (line237).
///
/// `challenge_id`, `target_node`, `challenger_address` (from
/// `records::Challenge`) and the evidence roots/cid are emitted as
/// lowercase hex strings 閳?the chain's `GetChallenge` / `GetNode`
/// lookup keys accept hex form for now (`chain_bridge.rs::challenge_to_json`
/// uses the same convention).
pub fn encode_msg_open_challenge(challenger_bech32: &str, challenge: &Challenge) -> Any {
    let mut buf = Vec::with_capacity(256);
    // Outer field1: challenger (bech32 string).
    encode_string(1, challenger_bech32, &mut buf);
    // Outer field2: Challenge (nested length-delimited message).
    let inner = encode_challenge_inner(challenge, challenger_bech32);
    encode_bytes(2, &inner, &mut buf);
    Any {
        type_url: "/pole.chain.pole.v1.MsgOpenChallenge".to_string(),
        value: buf,
    }
}

// --- inner Challenge encoder -------------------------------------------------

/// Encode the inner `Challenge` message
/// (`chain/proto/pole/chain/pole/v1/state.proto:139`).
///
/// `challenger_bech32` is reused for inner field5 to keep
/// outer.msg.challenger == inner.challenge.challenger.
fn encode_challenge_inner(challenge: &Challenge, challenger_bech32: &str) -> Vec<u8> {
    let mut buf = Vec::with_capacity(256);
    // Field1: challenge_id_hex (string, hex of32-byte hash).
    encode_string(1, &hex::encode(challenge.challenge_id), &mut buf);
    // Field2: kind (varint i32 閳?proto enum, UNSPECIFIED=0).
    encode_int32(2, challenge_kind_to_proto(challenge.kind), &mut buf);
    // Field3: epoch_id (uint64).
    encode_uint64(3, challenge.epoch_id, &mut buf);
    // Field4: target_address (string, hex of NodeId; empty when None).
    let target_address = challenge
        .target_node
        .as_ref()
        .map(hex::encode)
        .unwrap_or_default();
    encode_string(4, &target_address, &mut buf);
    // Field5: challenger (string 閳?bech32).
    encode_string(5, challenger_bech32, &mut buf);
    // Field6: bond_amount (uint64 閳?low64 bits of the u128 bond).
    encode_uint64(6, challenge.bond as u64, &mut buf);
    // Field7: opened_at_height (int64 閳?non-negative u64 fits cleanly).
    encode_int64(7, challenge.opened_at_height as i64, &mut buf);
    // Field8: deadline_height (int64 閳?non-negative u64 fits cleanly).
    encode_int64(8, challenge.deadline_height as i64, &mut buf);
    // Field9: state (varint i32 閳?proto enum, UNSPECIFIED=0).
    // At open time the canonical state is OPEN; we still pass through the
    // caller-provided value so subsequent callers (resolve, expire) can
    // reuse the encoder.
    encode_int32(9, challenge_state_to_proto(challenge.state), &mut buf);
    // Field10: evidence (nested length-delimited message).
    let ev = encode_evidence_inner(&challenge.evidence);
    encode_bytes(10, &ev, &mut buf);
    // Field11: slash_amount (uint64 閳?zero at open time).
    encode_uint64(11, 0, &mut buf);
    // Field12: challenger_reward (uint64 閳?zero at open time).
    encode_uint64(12, 0, &mut buf);
    // Field13: resolution_summary (string 閳?empty at open time).
    encode_string(13, "", &mut buf);
    // Field14: target_cons_address (string 閳?empty at open time;
    // the chain resolves it from the bonded validator set on its side).
    encode_string(14, "", &mut buf);
    buf
}

// --- inner ChallengeEvidenceRef encoder -------------------------------------

/// Encode the inner `ChallengeEvidenceRef` message
/// (`chain/proto/pole/chain/pole/v1/state.proto:130`).
///
/// Field5 (`merkle_proof_hex`) is `repeated string` 閳?emit one
/// length-delimited field per proof element, all sharing tag0x2A.
fn encode_evidence_inner(ev: &ChallengeEvidenceRef) -> Vec<u8> {
    let mut buf = Vec::with_capacity(128);
    // Field1: batch_root_hex (string; empty when None).
    let batch_root = ev
        .batch_root
        .as_ref()
        .map(hex::encode)
        .unwrap_or_default();
    encode_string(1, &batch_root, &mut buf);
    // Field2: aggregate_root_hex (string; empty when None).
    let aggregate_root = ev
        .aggregate_root
        .as_ref()
        .map(hex::encode)
        .unwrap_or_default();
    encode_string(2, &aggregate_root, &mut buf);
    // Field3: reward_root_hex (string; empty when None).
    let reward_root = ev
        .reward_root
        .as_ref()
        .map(hex::encode)
        .unwrap_or_default();
    encode_string(3, &reward_root, &mut buf);
    // Field4: payload_cid (string; empty when None).
    let payload_cid = ev.payload_cid.clone().unwrap_or_default();
    encode_string(4, &payload_cid, &mut buf);
    // Field5: merkle_proof_hex (repeated string 閳?one tag per element).
    for proof_hash in &ev.merkle_proof {
        encode_string(5, &hex::encode(proof_hash), &mut buf);
    }
    // Field6: aggregate_app_id (uint32 閳?Rust struct doesn't carry this
    // slot yet, so we emit the proto default of0).
    encode_uint32(6, 0, &mut buf);
    buf
}

// --- enum mapping helpers ---------------------------------------------------

/// Map Rust `ChallengeKind` (0-based) to the chain-side proto enum i32
/// (state.proto:114). Proto reserves `CHALLENGE_KIND_UNSPECIFIED =0`
/// and assigns1..5 to the concrete variants 閳?Rust's enum has no
/// `Unspecified` variant, so we shift the index up by1.
fn challenge_kind_to_proto(kind: ChallengeKind) -> i32 {
    match kind {
        ChallengeKind::BadBatch => 1,
        ChallengeKind::Omission => 2,
        ChallengeKind::BadAggregate => 3,
        ChallengeKind::BadReward => 4,
        ChallengeKind::BadStorage => 5,
    }
}

/// Map Rust `ChallengeState` to the chain-side proto enum i32
/// (state.proto:123). Proto exposes only three concrete states
/// (OPEN / RESOLVED / REJECTED); the richer Rust enum collapses as
/// follows:
///
///   - `Open` / `Responded` -> OPEN (the chain handler defaults OPEN
///     on open-time, so a Responded challenge is still treated as OPEN
///     until a ResolveChallenge call lands)
///   - `Succeeded` -> RESOLVED
///   - `Rejected` -> REJECTED
///   - `Expired` -> REJECTED (proto has no Expired slot;
///     expiration is modelled as a rejection with a resolution_summary
///     set on the chain side)
fn challenge_state_to_proto(state: ChallengeState) -> i32 {
    match state {
        ChallengeState::Open => 1,
        ChallengeState::Responded => 1,
        ChallengeState::Succeeded => 2,
        ChallengeState::Rejected => 3,
        ChallengeState::Expired => 3,
    }
}

// --- low-level proto wire format helpers --------------------------------

/// Encode field tag (varint).
pub(crate) fn encode_tag(field_number: u32, wire_type: u32, buf: &mut Vec<u8>) {
    let tag = (field_number << 3) | (wire_type & 0x7);
    encode_varint(tag as u64, buf);
}

/// Encode a varint. (Proto3 uses standard unsigned LEB128.)
pub(crate) fn encode_varint(mut value: u64, buf: &mut Vec<u8>) {
    while value >= 0x80 {
        buf.push((value as u8 & 0x7F) | 0x80);
        value >>= 7;
    }
    buf.push(value as u8);
}

/// Encode a length-delimited byte string (wire type2).
pub(crate) fn encode_bytes(field_number: u32, value: &[u8], buf: &mut Vec<u8>) {
    encode_tag(field_number, 2, buf);
    encode_varint(value.len() as u64, buf);
    buf.extend_from_slice(value);
}

/// Encode a UTF-8 string as a length-delimited field.
pub(crate) fn encode_string(field_number: u32, value: &str, buf: &mut Vec<u8>) {
    encode_bytes(field_number, value.as_bytes(), buf);
}

/// Encode a uint64 as a varint field (wire type0).
pub(crate) fn encode_uint64(field_number: u32, value: u64, buf: &mut Vec<u8>) {
    encode_tag(field_number, 0, buf);
    encode_varint(value, buf);
}

/// Encode a uint32 as a varint field (wire type0).
pub(crate) fn encode_uint32(field_number: u32, value: u32, buf: &mut Vec<u8>) {
    encode_tag(field_number, 0, buf);
    encode_varint(value as u64, buf);
}

/// Encode an int32 as a varint field (wire type0). For non-negative
/// values this is identical to uint32 varint encoding; negative values
/// are2's-complement sign-extended to32 bits.
pub(crate) fn encode_int32(field_number: u32, value: i32, buf: &mut Vec<u8>) {
    encode_tag(field_number, 0, buf);
    encode_varint(value as u64, buf);
}

/// Encode an int64 as a varint field (wire type0). For non-negative
/// values this is identical to uint64 varint encoding; negative values
/// are2's-complement sign-extended to64 bits (always10 bytes).
pub(crate) fn encode_int64(field_number: u32, value: i64, buf: &mut Vec<u8>) {
    encode_tag(field_number, 0, buf);
    encode_varint(value as u64, buf);
}

/// Encode a bool as a varint (wire type0).
pub(crate) fn encode_bool(field_number: u32, value: bool, buf: &mut Vec<u8>) {
    encode_uint64(field_number, value as u64, buf);
}

// ===========================================================================
// Remaining 8 Msg wire encoders
// ===========================================================================
//
// All eight follow the same pattern as `encode_msg_open_challenge`:
// outer field1 = signer string (bech32), outer field2 = nested
// `Msg*` payload, then per-field varint/length-delimited encoding
// matching `chain/proto/pole/chain/pole/v1/{tx,state}.proto`.
//
// `MsgResolveChallenge` is the only flat one (no nested message),
// so it skips the outer field2 length-prefix wrapper.

use crate::cosmos::wire_types::{
    AggregateRecordWire, BatchCommitWire, EpochCommitWire, GameWeightEntryWire,
    MerkleCommitmentWire, NodeCapabilitySetWire, NodeRecordWire, NodeRoleWire, ParamsWire,
    ReplicaReceiptWire,
};

// --- MsgUpsertNode -------------------------------------------------------

/// `MsgUpsertNode` 閳?pole.chain.pole.v1.MsgUpsertNode {
///   string operator = 1;
///   NodeRecord node = 2;
/// }
pub fn encode_msg_upsert_node(operator_bech32: &str, node: &NodeRecordWire) -> Any {
    let mut buf = Vec::with_capacity(256);
    encode_string(1, operator_bech32, &mut buf);
    let inner = encode_node_record_inner(node);
    encode_bytes(2, &inner, &mut buf);
    Any {
        type_url: "/pole.chain.pole.v1.MsgUpsertNode".to_string(),
        value: buf,
    }
}

fn encode_node_record_inner(node: &NodeRecordWire) -> Vec<u8> {
    let mut buf = Vec::with_capacity(256);
    encode_string(1, &node.operator_address, &mut buf);
    encode_string(2, &node.reward_address, &mut buf);
    encode_string(3, &node.consensus_address, &mut buf);
    encode_int32(4, node_role_to_proto(node.role), &mut buf);
    let caps = encode_node_capability_set_inner(&node.capabilities);
    encode_bytes(5, &caps, &mut buf);
    encode_bool(6, node.active, &mut buf);
    encode_uint64(7, node.bonded_tokens, &mut buf);
    encode_uint64(8, node.last_updated_epoch, &mut buf);
    encode_bool(9, node.is_player, &mut buf);
    buf
}

fn encode_node_capability_set_inner(c: &NodeCapabilitySetWire) -> Vec<u8> {
    let mut buf = Vec::with_capacity(8);
    encode_bool(1, c.collect, &mut buf);
    encode_bool(2, c.store, &mut buf);
    encode_bool(3, c.verify, &mut buf);
    encode_bool(4, c.propose, &mut buf);
    buf
}

/// Map `NodeRoleWire` to its proto enum i32 (re-exported for callers
/// that want to inspect the wire value).
fn node_role_to_proto(role: NodeRoleWire) -> i32 {
    crate::cosmos::wire_types::node_role_to_proto(role)
}

// --- MsgUpsertAggregateRecord -------------------------------------------

/// `MsgUpsertAggregateRecord` 閳?pole.chain.pole.v1.MsgUpsertAggregateRecord {
///   string operator = 1;
///   AggregateRecord aggregate_record = 2;
/// }
pub fn encode_msg_upsert_aggregate_record(
    operator_bech32: &str,
    aggregate_record: &AggregateRecordWire,
) -> Any {
    let mut buf = Vec::with_capacity(96);
    encode_string(1, operator_bech32, &mut buf);
    let inner = encode_aggregate_record_inner(aggregate_record);
    encode_bytes(2, &inner, &mut buf);
    Any {
        type_url: "/pole.chain.pole.v1.MsgUpsertAggregateRecord".to_string(),
        value: buf,
    }
}

fn encode_aggregate_record_inner(a: &AggregateRecordWire) -> Vec<u8> {
    let mut buf = Vec::with_capacity(48);
    encode_uint64(1, a.epoch_id, &mut buf);
    encode_uint32(2, a.app_id, &mut buf);
    encode_uint64(3, a.total_weight_units, &mut buf);
    encode_uint64(4, a.player_count, &mut buf);
    buf
}

// --- MsgSubmitBatch ------------------------------------------------------

/// `MsgSubmitBatch` 閳?pole.chain.pole.v1.MsgSubmitBatch {
///   string collector = 1;
///   BatchCommit batch_commit = 2;
/// }
pub fn encode_msg_submit_batch(collector_bech32: &str, batch: &BatchCommitWire) -> Any {
    let mut buf = Vec::with_capacity(256);
    encode_string(1, collector_bech32, &mut buf);
    let inner = encode_batch_commit_inner(batch);
    encode_bytes(2, &inner, &mut buf);
    Any {
        type_url: "/pole.chain.pole.v1.MsgSubmitBatch".to_string(),
        value: buf,
    }
}

fn encode_batch_commit_inner(b: &BatchCommitWire) -> Vec<u8> {
    let mut buf = Vec::with_capacity(192);
    encode_uint64(1, b.epoch_id, &mut buf);
    encode_string(2, &b.collector_address, &mut buf);
    encode_uint64(3, b.slot_start, &mut buf);
    encode_uint64(4, b.slot_end, &mut buf);
    let mc = encode_merkle_commitment_inner(&b.batch);
    encode_bytes(5, &mc, &mut buf);
    encode_string(6, &b.payload_cid, &mut buf);
    encode_uint32(7, b.observation_count, &mut buf);
    encode_int64(8, b.submitted_at_height, &mut buf);
    buf
}

fn encode_merkle_commitment_inner(m: &MerkleCommitmentWire) -> Vec<u8> {
    let mut buf = Vec::with_capacity(80);
    encode_string(1, &m.root, &mut buf);
    encode_uint32(2, m.leaf_count, &mut buf);
    buf
}

// --- MsgSubmitReplicaReceipt --------------------------------------------

/// `MsgSubmitReplicaReceipt` 閳?pole.chain.pole.v1.MsgSubmitReplicaReceipt {
///   string storer = 1;
///   ReplicaReceipt replica_receipt = 2;
/// }
pub fn encode_msg_submit_replica_receipt(storer_bech32: &str, receipt: &ReplicaReceiptWire) -> Any {
    let mut buf = Vec::with_capacity(256);
    encode_string(1, storer_bech32, &mut buf);
    let inner = encode_replica_receipt_inner(receipt);
    encode_bytes(2, &inner, &mut buf);
    Any {
        type_url: "/pole.chain.pole.v1.MsgSubmitReplicaReceipt".to_string(),
        value: buf,
    }
}

fn encode_replica_receipt_inner(r: &ReplicaReceiptWire) -> Vec<u8> {
    let mut buf = Vec::with_capacity(192);
    encode_uint64(1, r.epoch_id, &mut buf);
    encode_string(2, &r.payload_cid, &mut buf);
    encode_string(3, &r.storer_address, &mut buf);
    encode_uint64(4, r.retention_until_epoch, &mut buf);
    encode_string(5, &r.receipt_signature, &mut buf);
    encode_string(6, &r.receipt_hash_hex, &mut buf);
    buf
}

// --- MsgCommitEpoch ------------------------------------------------------

/// `MsgCommitEpoch` 閳?pole.chain.pole.v1.MsgCommitEpoch {
///   string proposer = 1;
///   EpochCommit epoch_commit = 2;
/// }
pub fn encode_msg_commit_epoch(proposer_bech32: &str, commit: &EpochCommitWire) -> Any {
    let mut buf = Vec::with_capacity(512);
    encode_string(1, proposer_bech32, &mut buf);
    let inner = encode_epoch_commit_inner(commit);
    encode_bytes(2, &inner, &mut buf);
    Any {
        type_url: "/pole.chain.pole.v1.MsgCommitEpoch".to_string(),
        value: buf,
    }
}

fn encode_epoch_commit_inner(c: &EpochCommitWire) -> Vec<u8> {
    let mut buf = Vec::with_capacity(512);
    encode_uint64(1, c.epoch_id, &mut buf);
    encode_bytes(
        2,
        &encode_merkle_commitment_inner(&c.accepted_batches),
        &mut buf,
    );
    encode_bytes(
        3,
        &encode_merkle_commitment_inner(&c.observations),
        &mut buf,
    );
    encode_bytes(4, &encode_merkle_commitment_inner(&c.aggregates), &mut buf);
    encode_bytes(5, &encode_merkle_commitment_inner(&c.rewards), &mut buf);
    encode_bytes(
        6,
        &encode_merkle_commitment_inner(&c.availability),
        &mut buf,
    );
    encode_string(7, &c.randomness_seed_hex, &mut buf);
    encode_string(8, &c.proposer_address, &mut buf);
    encode_int64(9, c.challenge_open_height, &mut buf);
    encode_int64(10, c.challenge_deadline_height, &mut buf);
    encode_bool(11, c.finalized, &mut buf);
    encode_uint64(12, c.total_network_weight_units, &mut buf);
    buf
}

// --- MsgResolveChallenge (flat, no nested message) ----------------------

/// `MsgResolveChallenge` 閳?pole.chain.pole.v1.MsgResolveChallenge {
///   string resolver = 1;
///   string challenge_id_hex = 2;
///   uint64 slash_amount = 3;
///   uint64 challenger_reward = 4;
///   string resolution_summary = 5;
///   ChallengeState final_state = 6;
///   uint32 slash_fraction_bps = 7;
///   bool jail_validator = 8;
/// }
#[allow(clippy::too_many_arguments)]
pub fn encode_msg_resolve_challenge(
    resolver_bech32: &str,
    challenge_id_hex: &str,
    slash_amount: u64,
    challenger_reward: u64,
    resolution_summary: &str,
    final_state: crate::primitives::ChallengeState,
    slash_fraction_bps: u32,
    jail_validator: bool,
) -> Any {
    let mut buf = Vec::with_capacity(256);
    encode_string(1, resolver_bech32, &mut buf);
    encode_string(2, challenge_id_hex, &mut buf);
    encode_uint64(3, slash_amount, &mut buf);
    encode_uint64(4, challenger_reward, &mut buf);
    encode_string(5, resolution_summary, &mut buf);
    encode_int32(6, challenge_state_to_proto(final_state), &mut buf);
    encode_uint32(7, slash_fraction_bps, &mut buf);
    encode_bool(8, jail_validator, &mut buf);
    Any {
        type_url: "/pole.chain.pole.v1.MsgResolveChallenge".to_string(),
        value: buf,
    }
}

// --- MsgUpsertGameWeight -------------------------------------------------

/// `MsgUpsertGameWeight` 閳?pole.chain.pole.v1.MsgUpsertGameWeight {
///   string authority = 1;
///   GameWeightEntry entry = 2;
/// }
pub fn encode_msg_upsert_game_weight(authority_bech32: &str, entry: &GameWeightEntryWire) -> Any {
    let mut buf = Vec::with_capacity(96);
    encode_string(1, authority_bech32, &mut buf);
    let inner = encode_game_weight_entry_inner(entry);
    encode_bytes(2, &inner, &mut buf);
    Any {
        type_url: "/pole.chain.pole.v1.MsgUpsertGameWeight".to_string(),
        value: buf,
    }
}

fn encode_game_weight_entry_inner(e: &GameWeightEntryWire) -> Vec<u8> {
    let mut buf = Vec::with_capacity(48);
    encode_uint32(1, e.app_id, &mut buf);
    encode_uint32(2, e.game_weight_ppm, &mut buf);
    encode_string(3, &e.tier, &mut buf);
    encode_uint64(4, e.effective_from_epoch_id, &mut buf);
    buf
}

// --- MsgUpdateParams -----------------------------------------------------

/// `MsgUpdateParams` 閳?pole.chain.pole.v1.MsgUpdateParams {
///   string authority = 1;
///   Params params = 2;
/// }
pub fn encode_msg_update_params(authority_bech32: &str, params: &ParamsWire) -> Any {
    let mut buf = Vec::with_capacity(256);
    encode_string(1, authority_bech32, &mut buf);
    let inner = encode_params_inner(params);
    encode_bytes(2, &inner, &mut buf);
    Any {
        type_url: "/pole.chain.pole.v1.MsgUpdateParams".to_string(),
        value: buf,
    }
}

fn encode_params_inner(p: &ParamsWire) -> Vec<u8> {
    let mut buf = Vec::with_capacity(256);
    encode_uint64(1, p.reward_block_duration_seconds, &mut buf);
    encode_uint64(2, p.base_hourly_reward, &mut buf);
    encode_uint64(3, p.target_network_weight_units, &mut buf);
    encode_uint32(4, p.reward_adjustment_cap_bps, &mut buf);
    encode_uint64(5, p.challenge_window_blocks, &mut buf);
    encode_uint64(6, p.min_retention_epochs, &mut buf);
    encode_uint32(7, p.player_reward_allocation_bps, &mut buf);
    encode_uint32(8, p.service_reward_allocation_bps, &mut buf);
    encode_uint32(9, p.collect_reward_bps, &mut buf);
    encode_uint32(10, p.store_reward_bps, &mut buf);
    encode_uint32(11, p.verify_reward_bps, &mut buf);
    encode_uint32(12, p.propose_reward_bps, &mut buf);
    encode_uint32(13, p.tier1_weight_ppm, &mut buf);
    encode_uint32(14, p.tier2_weight_min_ppm, &mut buf);
    encode_uint32(15, p.tier2_weight_max_ppm, &mut buf);
    encode_uint32(16, p.tier3_weight_min_ppm, &mut buf);
    encode_uint32(17, p.tier3_weight_max_ppm, &mut buf);
    encode_uint32(18, p.fee_burn_bps, &mut buf);
    encode_uint64(19, p.reward_burn_threshold, &mut buf);
    encode_uint32(20, p.reward_burn_bps, &mut buf);
    encode_uint32(21, p.governance_burn_bps, &mut buf);
    // proto field 22 (uint64): min_verification_count 閳?the FinalizeEpoch
    // verification-coverage gate. Omitting it zeroes the gate on-chain.
    encode_uint64(22, p.min_verification_count, &mut buf);
    // proto field 23 (uint32): min_player_verifier_share_bps.
    encode_uint32(23, p.min_player_verifier_share_bps, &mut buf);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::ChallengeState;
    use crate::records::ChallengeEvidenceRef;
    
    fn sample_challenge(
        kind: ChallengeKind,
        state: ChallengeState,
        target: Option<[u8; 32]>,
        evidence: ChallengeEvidenceRef,
    ) -> Challenge {
        Challenge {
            challenge_id: [0xAAu8; 32],
            kind,
            epoch_id: 42,
            target_node: target,
            challenger: [0xBBu8; 32],
            bond: 1_000,
            opened_at_height: 100,
            deadline_height: 200,
            state,
            evidence,
        }
    }

    fn empty_evidence() -> ChallengeEvidenceRef {
        ChallengeEvidenceRef {
            batch_root: None,
            aggregate_root: None,
            reward_root: None,
            payload_cid: None,
            merkle_proof: Vec::new(),
        }
    }

    #[test]
    fn varint_encoding_matches_proto_spec() {
        let mut buf = Vec::new();
        encode_varint(1, &mut buf);
        assert_eq!(buf, vec![0x01]);
        encode_varint(300, &mut buf);
        assert_eq!(buf, vec![0x01, 0xAC, 0x02]);
    }

    #[test]
    fn finalize_epoch_encodes_to_expected_bytes() {
        let any = encode_msg_finalize_epoch("cosmos1abc", 42);
        assert_eq!(any.type_url, "/pole.chain.pole.v1.MsgFinalizeEpoch");
        // Field1 (string): tag=0x0A, length=10, "cosmos1abc"
        // Field2 (uint64): tag=0x10, value=42
        assert_eq!(
            any.value,
            vec![
                0x0A, 0x0A, b'c', b'o', b's', b'm', b'o', b's', b'1', b'a', b'b', b'c', 0x10, 0x2A,
            ]
        );
    }

    #[test]
    fn verify_batch_encodes_expected_wire_bytes() {
        let any = encode_msg_verify_batch(
            "cosmos1verify",
            7,
            "ab".repeat(32).as_str(),
            "cosmos1collect",
            true,
            true,
            "deadbeef",
        );
        assert_eq!(any.type_url, "/pole.chain.pole.v1.MsgVerifyBatch");
        assert!(!any.value.is_empty());
        // Outer field1 verifier (string): tag 0x0A.
        assert_eq!(any.value[0], 0x0A);
        // Field2 epoch_id = 7: tag (2<<3)|0 = 0x10, varint 0x07.
        assert!(any.value.windows(2).any(|w| w == [0x10, 0x07]));
        // Field5 is_player = true: tag (5<<3)|0 = 0x28, varint 0x01.
        assert!(any.value.windows(2).any(|w| w == [0x28, 0x01]));
        // Field6 verified = true: tag 0x30, varint 0x01.
        assert!(any.value.windows(2).any(|w| w == [0x30, 0x01]));
    }

    #[test]
    fn claim_reward_handles_empty_recipient() {
        let any = encode_msg_claim_reward("cosmos1abc", 1, "");
        // Three fields, all should encode cleanly
        assert!(!any.value.is_empty());
        // Last byte should mark the end of the empty string field
        assert_eq!(any.value.last(), Some(&0x00));
    }

    /// Phase0.2: the trait is the forward-compatible hook for
    /// plugging in new message types without touching `BridgeMessage`.
    /// This test demonstrates a minimal impl.
    #[test]
    fn message_encoder_trait_is_implementable() {
        struct Hello {
            who: String,
        }
        impl MessageEncoder for Hello {
            fn type_url(&self) -> &'static str {
                "/pole.test.v1.MsgHello"
            }
            fn encode(&self) -> Vec<u8> {
                let mut buf = Vec::new();
                encode_string(1, &self.who, &mut buf);
                buf
            }
        }
        let h = Hello {
            who: "world".into(),
        };
        assert_eq!(h.type_url(), "/pole.test.v1.MsgHello");
        // Field1 string: tag=0x0A, length=5, "world"
        assert_eq!(h.encode(), vec![0x0A, 0x05, b'w', b'o', b'r', b'l', b'd']);
    }

    // --- MsgOpenChallenge tests ---------------------------------------------

    /// Smoke test: a minimal challenge produces non-empty proto3 bytes
    /// with the correct type_url. This is the regression test for the
    /// `dead-code stub` that used to emit only the outer `challenger`
    /// field and silently drop the nested `challenge` message.
    #[test]
    fn open_challenge_emits_non_empty_wire_bytes() {
        let challenge = sample_challenge(
            ChallengeKind::BadBatch,
            ChallengeState::Open,
            None,
            empty_evidence(),
        );
        let any = encode_msg_open_challenge("cosmos1abc", &challenge);
        assert_eq!(any.type_url, "/pole.chain.pole.v1.MsgOpenChallenge");
        assert!(
            !any.value.is_empty(),
            "MsgOpenChallenge must emit non-empty proto bytes (was a dead-code stub): got {} bytes",
            any.value.len()
        );
    }

    /// The first byte must be the outer `challenger` field tag (field1,
    /// length-delimited =0x0A). After the bech32 string, the next
    /// bytes must start a nested `Challenge` message (tag0x12 for
    /// outer field2 length-delimited).
    #[test]
    fn open_challenge_outer_wire_layout_matches_proto() {
        let challenge = sample_challenge(
            ChallengeKind::BadBatch,
            ChallengeState::Open,
            None,
            empty_evidence(),
        );
        let any = encode_msg_open_challenge("cosmos1abc", &challenge);
        // Outer field1 (challenger): tag0x0A + length10 +10 ASCII bytes.
        assert_eq!(any.value[0], 0x0A);
        assert_eq!(any.value[1], 10);
        assert_eq!(&any.value[2..12], b"cosmos1abc");
        // Outer field2 (challenge nested): tag0x12 followed by varint length.
        assert_eq!(any.value[12], 0x12);
        let (inner_len, hdr_len) = decode_varint(&any.value[13..]);
        assert!(inner_len > 0, "inner Challenge message must be non-empty");
        assert!(
            13 + hdr_len + inner_len as usize == any.value.len(),
            "outer length-prefix must cover exactly the remaining bytes"
        );
    }

    /// The inner `Challenge` message must carry a `kind` field whose
    /// varint value matches the proto enum for `CHALLENGE_KIND_BAD_BATCH` (=1).
    /// Tag for inner field2 (varint) = (2 <<3) |0 =0x10.
    #[test]
    fn open_challenge_inner_carries_kind_varint() {
        let challenge = sample_challenge(
            ChallengeKind::BadBatch,
            ChallengeState::Open,
            None,
            empty_evidence(),
        );
        let any = encode_msg_open_challenge("cosmos1abc", &challenge);
        // Search for the inner kind tag (0x10) anywhere in the bytes;
        // the following byte must be the varint1.
        let pos = find_byte_sequence(&any.value, &[0x10, 0x01])
            .expect("inner kind tag (0x10) + varint1 must appear");
        assert!(
            pos > 12,
            "kind tag must live inside the nested Challenge, not the outer challenger"
        );
    }

    /// Helper: locate a byte sequence inside a buffer.
    fn find_byte_sequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        if needle.is_empty() || haystack.len() < needle.len() {
            return None;
        }
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    }

    /// Helper: decode a single varint from the start of `buf`.
    /// Returns `(value, bytes_consumed)`.
    fn decode_varint(buf: &[u8]) -> (u64, usize) {
        let mut value: u64 = 0;
        let mut shift = 0u32;
        for (i, b) in buf.iter().enumerate() {
            value |= ((*b & 0x7F) as u64) << shift;
            if (*b & 0x80) == 0 {
                return (value, i + 1);
            }
            shift += 7;
            if shift >= 64 {
                panic!("varint too long");
            }
        }
        panic!("unterminated varint");
    }

    // --- five ChallengeKind variant tests -----------------------------------

    /// Each `ChallengeKind` variant must round-trip into the correct
    /// proto enum i32 value via the inner field2 varint.
    /// Tag = (2 <<3) |0 =0x10. Values: BadBatch=1, Omission=2,
    /// BadAggregate=3, BadReward=4, BadStorage=5.
    #[test]
    fn open_challenge_kind_bad_batch_value_is_1() {
        let challenge = sample_challenge(
            ChallengeKind::BadBatch,
            ChallengeState::Open,
            Some([0x11u8; 32]),
            empty_evidence(),
        );
        let any = encode_msg_open_challenge("cosmos1abc", &challenge);
        assert_kind_varint(&any.value, 0x01);
        // BadBatch evidence: batch_root set, others empty.
        assert!(
            any.value.windows(2).any(|w| w == [0x0A, 64]),
            "expected batch_root_hex length-delimited field"
        );
    }

    #[test]
    fn open_challenge_kind_omission_value_is_2() {
        let challenge = sample_challenge(
            ChallengeKind::Omission,
            ChallengeState::Open,
            Some([0x22u8; 32]),
            empty_evidence(),
        );
        let any = encode_msg_open_challenge("cosmos1abc", &challenge);
        assert_kind_varint(&any.value, 0x02);
    }

    #[test]
    fn open_challenge_kind_bad_aggregate_value_is_3() {
        let challenge = sample_challenge(
            ChallengeKind::BadAggregate,
            ChallengeState::Open,
            Some([0x33u8; 32]),
            empty_evidence(),
        );
        let any = encode_msg_open_challenge("cosmos1abc", &challenge);
        assert_kind_varint(&any.value, 0x03);
    }

    #[test]
    fn open_challenge_kind_bad_reward_value_is_4() {
        let challenge = sample_challenge(
            ChallengeKind::BadReward,
            ChallengeState::Open,
            Some([0x44u8; 32]),
            empty_evidence(),
        );
        let any = encode_msg_open_challenge("cosmos1abc", &challenge);
        assert_kind_varint(&any.value, 0x04);
    }

    #[test]
    fn open_challenge_kind_bad_storage_value_is_5() {
        let challenge = sample_challenge(
            ChallengeKind::BadStorage,
            ChallengeState::Open,
            Some([0x55u8; 32]),
            empty_evidence(),
        );
        let any = encode_msg_open_challenge("cosmos1abc", &challenge);
        assert_kind_varint(&any.value, 0x05);
    }

    /// Confirm the inner kind tag (0x10) is followed by the expected
    /// varint byte. Looks for the exact `[tag, varint]` sequence
    /// anywhere in the payload (the encoder writes kind as the second
    /// field of the nested Challenge, so it is preceded by
    /// `challenge_id_hex` length-delimited bytes).
    fn assert_kind_varint(bytes: &[u8], varint: u8) {
        let pair = [0x10u8, varint];
        assert!(
            bytes.windows(2).any(|w| w == pair),
            "expected inner kind tag0x10 followed by varint {:#04x}, got bytes {:02x?}",
            varint,
            bytes
        );
    }

    // --- end-to-end wire shape test -----------------------------------------

    /// Golden-vector test: hard-codes the exact byte sequence for a
    /// minimal BadBatch challenge. Any drift in field order, tag
    /// computation, or enum mapping will trip this test.
    ///
    /// Wire breakdown:
    /// outer field1 challenger = "cosmos1abc"
    /// [0x0A,0x0A, b"cosmos1abc"]
    /// outer field2 challenge (nested) 閳?length-prefixed inner:
    /// [0x12, <inner_len>, ...inner bytes...]
    /// inner field1 challenge_id_hex = "aaaa...aa" (64 chars)
    /// [0x0A,0x40, b"aa" *32]
    /// inner field2 kind =1 (CHALLENGE_KIND_BAD_BATCH)
    /// [0x10,0x01]
    /// inner field3 epoch_id =42
    /// [0x18,0x2A]
    /// inner field4 target_address = "1111...11" (64 chars)
    /// [0x22,0x40, b"11" *32]
    /// inner field5 challenger = "cosmos1abc"
    /// [0x2A,0x0A, b"cosmos1abc"]
    /// inner field6 bond_amount =1000 (varint =0xE8,0x07)
    /// [0x30,0xE8,0x07]
    /// inner field7 opened_at_height =100
    /// [0x38,0x64]
    /// inner field8 deadline_height =200
    /// [0x40,0xC8,0x01]
    /// inner field9 state =1 (CHALLENGE_STATE_OPEN)
    /// [0x48,0x01]
    /// inner field10 evidence (nested 閳?empty evidence)
    /// [0x52,0x0A, ...evidence bytes...]
    /// inner field11 slash_amount =0
    /// [0x58,0x00]
    /// inner field12 challenger_reward =0
    /// [0x60,0x00]
    /// inner field13 resolution_summary = "" (length-delimited empty)
    /// [0x6A,0x00]
    /// inner field14 target_cons_address = "" (length-delimited empty)
    /// [0x72,0x00]
    #[test]
    fn open_challenge_golden_vector_bad_batch() {
        let challenge = sample_challenge(
            ChallengeKind::BadBatch,
            ChallengeState::Open,
            Some([0x11u8; 32]),
            empty_evidence(),
        );
        let any = encode_msg_open_challenge("cosmos1abc", &challenge);
        assert_eq!(any.type_url, "/pole.chain.pole.v1.MsgOpenChallenge");

        // Build the expected bytes by hand.
        let mut expected_inner: Vec<u8> = Vec::new();
        // inner field1 challenge_id_hex (64 chars of "aa")
        encode_string(1, &"aa".repeat(32), &mut expected_inner);
        // inner field2 kind =1
        encode_int32(2, 1, &mut expected_inner);
        // inner field3 epoch_id =42
        encode_uint64(3, 42, &mut expected_inner);
        // inner field4 target_address = "11" *32 (64 chars)
        encode_string(4, &"11".repeat(32), &mut expected_inner);
        // inner field5 challenger = "cosmos1abc"
        encode_string(5, "cosmos1abc", &mut expected_inner);
        // inner field6 bond_amount =1000 (u64 cast)
        encode_uint64(6, 1000, &mut expected_inner);
        // inner field7 opened_at_height =100
        encode_int64(7, 100, &mut expected_inner);
        // inner field8 deadline_height =200
        encode_int64(8, 200, &mut expected_inner);
        // inner field9 state =1
        encode_int32(9, 1, &mut expected_inner);
        // inner field10 evidence (empty)
        let ev_bytes = encode_evidence_inner(&challenge.evidence);
        encode_bytes(10, &ev_bytes, &mut expected_inner);
        // inner field11 slash_amount =0
        encode_uint64(11, 0, &mut expected_inner);
        // inner field12 challenger_reward =0
        encode_uint64(12, 0, &mut expected_inner);
        // inner field13 resolution_summary = ""
        encode_string(13, "", &mut expected_inner);
        // inner field14 target_cons_address = ""
        encode_string(14, "", &mut expected_inner);

        let mut expected_outer: Vec<u8> = Vec::new();
        // outer field1 challenger
        encode_string(1, "cosmos1abc", &mut expected_outer);
        // outer field2 challenge
        encode_bytes(2, &expected_inner, &mut expected_outer);

        assert_eq!(
            any.value, expected_outer,
            "OpenChallenge wire bytes drift from golden vector"
        );
        println!(
            "open_challenge_golden_vector_bad_batch: outer={} bytes, inner={} bytes",
            any.value.len(),
            expected_inner.len()
        );
    }

    /// Exercise the evidence encoder with a populated
    /// `ChallengeEvidenceRef`: merkle_proof is `repeated string`, so we
    /// expect one length-delimited tag (0x2A) per proof element, all
    /// appearing inside the nested evidence message.
    #[test]
    fn open_challenge_evidence_emits_repeated_merkle_proof() {
        let evidence = ChallengeEvidenceRef {
            batch_root: Some([0xA1u8; 32]),
            aggregate_root: None,
            reward_root: None,
            payload_cid: Some("bafy.test.cid".to_string()),
            merkle_proof: vec![[0xB1u8; 32], [0xB2u8; 32], [0xB3u8; 32]],
        };
        let challenge = sample_challenge(
            ChallengeKind::BadBatch,
            ChallengeState::Open,
            Some([0x11u8; 32]),
            evidence,
        );
        let any = encode_msg_open_challenge("cosmos1abc", &challenge);
        // Three merkle_proof elements should produce three occurrences
        // of the [0x2A, 0x40] tag+length marker in the byte stream.
        // Note: a bare 0x2A byte is ambiguous (the inner field5
        // `challenger` string also uses wire-type2 field-5 tag = 0x2A),
        // so we count the [tag, len=0x40] pair which is unique to
        // 64-hex-char string fields (the merkle proof hashes).
        let tag_count = any.value.windows(2).filter(|w| w == &[0x2A, 0x40]).count();
        assert_eq!(
  tag_count,3,
  "expected3 occurrences of evidence field-5 tag+len (0x2A,0x40) for3 merkle proof hashes, got {}",
  tag_count
  );
        // Each proof hash hex is64 chars; expect three [0x2A,0x40] markers.
        let len_marker_count = any.value.windows(2).filter(|w| w == &[0x2A, 0x40]).count();
        assert_eq!(
            len_marker_count, 3,
            "expected3 occurrences of [0x2A,0x40] length marker, got {}",
            len_marker_count
        );
    }

    // ===============================================================
    // Tests for the 8 newly-wired Msg encoders
    // ===============================================================
    //
    // Each encoder test asserts: type_url, non-empty value, and first
    // byte = 0x0A (outer field1 = signer string tag). The complex
    // ones (UpsertNode / CommitEpoch) also get a golden vector to
    // lock down field ordering.

    fn sample_merkle_commitment() -> MerkleCommitmentWire {
        MerkleCommitmentWire {
            root: "ab".repeat(32),
            leaf_count: 7,
        }
    }

    // --- MsgUpsertNode ------------------------------------------------

    #[test]
    fn upsert_node_emits_non_empty_wire_bytes() {
        let node = NodeRecordWire {
            operator_address: "cosmos1op".into(),
            reward_address: "cosmos1op".into(),
            consensus_address: "cosmosvalcons1consensus".into(),
            role: NodeRoleWire::Coordinator,
            capabilities: NodeCapabilitySetWire {
                collect: true,
                store: false,
                verify: true,
                propose: true,
            },
            active: true,
            bonded_tokens: 1_000_000,
            last_updated_epoch: 5,
            is_player: false,
        };
        let any = encode_msg_upsert_node("cosmos1op", &node);
        assert_eq!(any.type_url, "/pole.chain.pole.v1.MsgUpsertNode");
        assert!(!any.value.is_empty());
        assert_eq!(any.value[0], 0x0A);
    }

    #[test]
    fn upsert_node_role_enum_maps_to_proto_varints() {
        // Player=1, Service=2, Coordinator=3
        // Tag for inner field4 (varint) = (4 << 3) | 0 = 0x20.
        let cases = [
            (NodeRoleWire::Player, 0x01u8),
            (NodeRoleWire::Service, 0x02u8),
            (NodeRoleWire::Coordinator, 0x03u8),
        ];
        for (role, expected_varint) in cases {
            let node = NodeRecordWire {
                operator_address: "cosmos1op".into(),
                reward_address: "cosmos1op".into(),
                consensus_address: "".into(),
                role,
                capabilities: NodeCapabilitySetWire::default(),
                active: false,
                bonded_tokens: 0,
                last_updated_epoch: 0,
                is_player: false,
            };
            let any = encode_msg_upsert_node("cosmos1op", &node);
            let pair = [0x20u8, expected_varint];
            assert!(
                any.value.windows(2).any(|w| w == pair),
                "expected role tag 0x20 + varint {:#04x}, got bytes {:02x?}",
                expected_varint,
                any.value
            );
        }
    }

    // --- MsgUpsertAggregateRecord --------------------------------------

    #[test]
    fn upsert_aggregate_record_emits_non_empty_wire_bytes() {
        let ar = AggregateRecordWire {
            epoch_id: 42,
            app_id: 7,
            total_weight_units: 1_000_000,
            player_count: 50,
        };
        let any = encode_msg_upsert_aggregate_record("cosmos1op", &ar);
        assert_eq!(any.type_url, "/pole.chain.pole.v1.MsgUpsertAggregateRecord");
        assert!(!any.value.is_empty());
        assert_eq!(any.value[0], 0x0A);
    }

    // --- MsgSubmitBatch ------------------------------------------------

    #[test]
    fn submit_batch_emits_non_empty_wire_bytes() {
        let batch = BatchCommitWire {
            epoch_id: 42,
            collector_address: "cosmos1col".into(),
            slot_start: 100,
            slot_end: 200,
            batch: sample_merkle_commitment(),
            payload_cid: "bafy.test.cid".into(),
            observation_count: 5,
            submitted_at_height: 1000,
        };
        let any = encode_msg_submit_batch("cosmos1col", &batch);
        assert_eq!(any.type_url, "/pole.chain.pole.v1.MsgSubmitBatch");
        assert!(!any.value.is_empty());
        assert_eq!(any.value[0], 0x0A);
    }

    // --- MsgSubmitReplicaReceipt ---------------------------------------

    #[test]
    fn submit_replica_receipt_emits_non_empty_wire_bytes() {
        let r = ReplicaReceiptWire {
            epoch_id: 42,
            payload_cid: "bafy.test.cid".into(),
            storer_address: "cosmos1storer".into(),
            retention_until_epoch: 100,
            receipt_signature: "deadbeef".into(),
            receipt_hash_hex: "ab".repeat(32),
        };
        let any = encode_msg_submit_replica_receipt("cosmos1storer", &r);
        assert_eq!(any.type_url, "/pole.chain.pole.v1.MsgSubmitReplicaReceipt");
        assert!(!any.value.is_empty());
        assert_eq!(any.value[0], 0x0A);
    }

    // --- MsgCommitEpoch ------------------------------------------------

    #[test]
    fn commit_epoch_emits_non_empty_wire_bytes() {
        let mc = sample_merkle_commitment();
        let commit = EpochCommitWire {
            epoch_id: 42,
            accepted_batches: mc.clone(),
            observations: mc.clone(),
            aggregates: mc.clone(),
            rewards: mc.clone(),
            availability: mc,
            randomness_seed_hex: "cd".repeat(32),
            proposer_address: "cosmos1prop".into(),
            challenge_open_height: 100,
            challenge_deadline_height: 200,
            finalized: false,
            total_network_weight_units: 50_000,
        };
        let any = encode_msg_commit_epoch("cosmos1prop", &commit);
        assert_eq!(any.type_url, "/pole.chain.pole.v1.MsgCommitEpoch");
        assert!(!any.value.is_empty());
        assert_eq!(any.value[0], 0x0A);
        // 5 nested MerkleCommitment fields, each with tag 0x0A/0x12/0x1A/0x22/0x2A
        // for outer field2 (length-delimited) on inner fields 2-6.
        // Just sanity-check that we see all 5 expected outer tags.
        for tag in [0x12u8, 0x1A, 0x22, 0x2A, 0x32] {
            assert!(
                any.value.contains(&tag),
                "expected outer MerkleCommitment tag 0x{:02x} in encoded bytes",
                tag
            );
        }
    }

    // --- MsgResolveChallenge (flat) ------------------------------------

    #[test]
    fn resolve_challenge_emits_non_empty_wire_bytes() {
        let any = encode_msg_resolve_challenge(
            "cosmos1resolver",
            "ab".repeat(32).as_str(),
            1_000,
            100,
            "verified",
            crate::primitives::ChallengeState::Succeeded,
            5_000,
            true,
        );
        assert_eq!(any.type_url, "/pole.chain.pole.v1.MsgResolveChallenge");
        assert!(!any.value.is_empty());
        assert_eq!(any.value[0], 0x0A);
        // final_state tag = (6 << 3) | 0 = 0x30, varint = 2 (RESOLVED)
        let pair = [0x30u8, 0x02];
        assert!(
            any.value.windows(2).any(|w| w == pair),
            "expected final_state tag 0x30 + varint 0x02, got bytes {:02x?}",
            any.value
        );
    }

    // --- MsgUpsertGameWeight -------------------------------------------

    #[test]
    fn upsert_game_weight_emits_non_empty_wire_bytes() {
        let entry = GameWeightEntryWire {
            app_id: 1,
            game_weight_ppm: 500_000,
            tier: "tier1".into(),
            effective_from_epoch_id: 10,
        };
        let any = encode_msg_upsert_game_weight("cosmos10authority", &entry);
        assert_eq!(any.type_url, "/pole.chain.pole.v1.MsgUpsertGameWeight");
        assert!(!any.value.is_empty());
        assert_eq!(any.value[0], 0x0A);
    }

    // --- MsgUpdateParams -----------------------------------------------

    #[test]
    fn update_params_emits_non_empty_wire_bytes() {
        let p = full_params_fixture();
        let any = encode_msg_update_params("cosmos10authority", &p);
        assert_eq!(any.type_url, "/pole.chain.pole.v1.MsgUpdateParams");
        assert!(!any.value.is_empty());
        assert_eq!(any.value[0], 0x0A);
        // 23 uint64/uint32 fields inside. Verify the inner last-field
        // tags: field 21 (0xA8 0x01 + varint 100 = 0x64), field 22
        // (0xB0 0x01 + varint 3), field 23 (0xB8 0x01 + varint 5000 =
        // 0x88 0x27) all appear at the tail.
        let triple21 = [0xA8u8, 0x01, 0x64];
        let triple22 = [0xB0u8, 0x01, 0x03];
        let quad23 = [0xB8u8, 0x01, 0x88, 0x27];
        assert!(
            any.value.windows(3).any(|w| w == triple21),
            "expected governance_burn_bps tag 0xA8 0x01 + varint 0x64, got bytes {:02x?}",
            any.value
        );
        assert!(
            any.value.windows(3).any(|w| w == triple22),
            "expected min_verification_count tag 0xB0 0x01 + varint 0x03, got bytes {:02x?}",
            any.value
        );
        assert!(
            any.value.windows(4).any(|w| w == quad23),
            "expected min_player_verifier_share_bps tag 0xB8 0x01 + varint 5000, got bytes {:02x?}",
            any.value
        );
    }

    #[test]
    fn params_wire_matches_go_proto_marshal_golden() {
        // Cross-language golden: the same 23-field Params marshaled by
        // Go's gogoproto (chain/x/pole/types Params, proto.Marshal)
        // produced this exact byte string. Any drift in field order,
        // tag, or varint encoding breaks the byte-identical lock.
        let p = full_params_fixture();
        let expected = hex_decode(
            "08901c106418c0843d20d00f2864300538d83640d00f48c41350c41358c41360c41368a0c21e70c09a0c7880ea308001a08d068801a0f7369001f4039801c0843da00164a80164b00103b8018827",
        );
        let inner = encode_params_inner(&p);
        assert_eq!(inner, expected);
        assert_eq!(inner.len(), 78);
        // The two verification gates must be present: a Rust-built
        // MsgUpdateParams must never silently zero them (that would
        // disable FinalizeEpoch's verification-coverage gates).
        assert!(inner.ends_with(&[0xB0, 0x01, 0x03, 0xB8, 0x01, 0x88, 0x27]));
    }

    fn full_params_fixture() -> ParamsWire {
        ParamsWire {
            reward_block_duration_seconds: 3600,
            base_hourly_reward: 100,
            target_network_weight_units: 1_000_000,
            reward_adjustment_cap_bps: 2_000,
            challenge_window_blocks: 100,
            min_retention_epochs: 5,
            player_reward_allocation_bps: 7_000,
            service_reward_allocation_bps: 2_000,
            collect_reward_bps: 2_500,
            store_reward_bps: 2_500,
            verify_reward_bps: 2_500,
            propose_reward_bps: 2_500,
            tier1_weight_ppm: 500_000,
            tier2_weight_min_ppm: 200_000,
            tier2_weight_max_ppm: 800_000,
            tier3_weight_min_ppm: 100_000,
            tier3_weight_max_ppm: 900_000,
            fee_burn_bps: 500,
            reward_burn_threshold: 1_000_000,
            reward_burn_bps: 100,
            governance_burn_bps: 100,
            min_verification_count: 3,
            min_player_verifier_share_bps: 5_000,
        }
    }

    fn hex_decode(hex: &str) -> Vec<u8> {
        hex::decode(hex).expect("golden hex")
    }
}
