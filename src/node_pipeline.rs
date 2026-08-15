use std::fmt;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::primitives::{
    ActivitySourceKind, AppId, ContentId, EpochId, Hash32, Height, NodeId, SignatureBytes, SlotId,
    UnixMillis,
};
use crate::records::{AggregateRecord, BatchCommit, ObservationRecord, RewardRecord};
use crate::MerkleCommitment;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivitySample {
    pub app_id: AppId,
    pub observed_players: u64,
    pub observed_at_millis: UnixMillis,
    pub source_kind: ActivitySourceKind,
    pub source_confidence_ppm: u32,
    pub raw_body: String,
}

pub type SteamCurrentPlayersSample = ActivitySample;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssembledBatch {
    pub batch_commit: BatchCommit,
    pub payload_hash: Hash32,
    pub payload_cid: ContentId,
    pub payload_bytes: Vec<u8>,
    pub observations: Vec<ObservationRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodePipelineError {
    EmptySignature,
    EmptyRawBody,
    EmptyBatch,
    MismatchedEpoch { expected: EpochId, actual: EpochId },
    MismatchedCollector { expected: NodeId, actual: NodeId },
}

impl fmt::Display for NodePipelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySignature => write!(f, "empty collector signature"),
            Self::EmptyRawBody => write!(f, "empty raw response body"),
            Self::EmptyBatch => write!(f, "cannot finalize an empty batch"),
            Self::MismatchedEpoch { expected, actual } => {
                write!(f, "mismatched epoch: expected {expected}, got {actual}")
            }
            Self::MismatchedCollector { expected, actual } => {
                write!(
                    f,
                    "mismatched collector: expected {}, got {}",
                    hex_lower(expected),
                    hex_lower(actual)
                )
            }
        }
    }
}

impl std::error::Error for NodePipelineError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchBuilder {
    epoch_id: EpochId,
    collector_id: NodeId,
    observations: Vec<ObservationRecord>,
}

impl ActivitySample {
    pub fn new(
        app_id: AppId,
        observed_players: u64,
        observed_at_millis: UnixMillis,
        raw_body: &str,
        source_kind: ActivitySourceKind,
        source_confidence_ppm: u32,
    ) -> Self {
        Self {
            app_id,
            observed_players,
            observed_at_millis,
            source_kind,
            source_confidence_ppm,
            raw_body: raw_body.to_string(),
        }
    }

    pub fn steam_current_players(
        app_id: AppId,
        observed_players: u64,
        observed_at_millis: UnixMillis,
        raw_body: &str,
    ) -> Self {
        Self::new(
            app_id,
            observed_players,
            observed_at_millis,
            raw_body,
            ActivitySourceKind::Steam,
            1_000_000,
        )
    }

    pub fn into_observation(
        self,
        epoch_id: EpochId,
        slot_id: SlotId,
        collector_id: NodeId,
        collector_signature: SignatureBytes,
    ) -> Result<ObservationRecord, NodePipelineError> {
        if collector_signature.is_empty() {
            return Err(NodePipelineError::EmptySignature);
        }
        if self.raw_body.is_empty() {
            return Err(NodePipelineError::EmptyRawBody);
        }

        let raw_body_hash = stable_hash32(self.raw_body.as_bytes());
        let raw_body_cid = cid_from_hash(raw_body_hash, source_namespace(self.source_kind));

        Ok(ObservationRecord {
            epoch_id,
            slot_id,
            app_id: self.app_id,
            source_kind: self.source_kind,
            source_confidence_ppm: self.source_confidence_ppm,
            observed_players: self.observed_players,
            observed_at_millis: self.observed_at_millis,
            collector_id,
            raw_body_cid,
            raw_body_hash,
            collector_signature,
        })
    }
}

fn source_namespace(source_kind: ActivitySourceKind) -> &'static str {
    match source_kind {
        ActivitySourceKind::Steam => "steam-observation",
        ActivitySourceKind::Epic => "epic-observation",
        ActivitySourceKind::Ea => "ea-observation",
        ActivitySourceKind::Gog => "gog-observation",
        ActivitySourceKind::Community => "community-observation",
    }
}

/// Status of a single observation's collector signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureStatus {
    /// No signature attached.
    Empty,
    /// The legacy 32-byte development placeholder hash (not a real
    /// Ed25519 signature) — recognised so verifiers can report it.
    DevPlaceholder,
    /// A real Ed25519 signature that verified against the collector key.
    Valid,
    /// A real-looking signature that failed verification.
    Invalid,
    /// A real signature but no collector public key was available to
    /// verify it against.
    Unverifiable,
}

impl ObservationRecord {
    /// Canonical bytes signed by the collector. The signature field itself
    /// is excluded so the payload is stable before and after signing.
    pub fn signing_payload(&self) -> Vec<u8> {
        let mut copy = self.clone();
        copy.collector_signature = Vec::new();
        borsh::to_vec(&copy).expect("observation borsh encoding")
    }

    /// Verify the attached collector signature.
    ///
    /// `pubkey` is the collector's Ed25519 public key when known (e.g. from
    /// the local node table for our own batches); `None` yields
    /// [`SignatureStatus::Unverifiable`] for real signatures. Legacy
    /// 32-byte dev placeholders are reported as
    /// [`SignatureStatus::DevPlaceholder`] and never count as failures.
    pub fn verify_collector_signature(&self, pubkey: Option<&[u8; 32]>) -> SignatureStatus {
        if self.collector_signature.is_empty() {
            return SignatureStatus::Empty;
        }
        if self.collector_signature.len() == 32 {
            return SignatureStatus::DevPlaceholder;
        }
        let Some(pubkey) = pubkey else {
            return SignatureStatus::Unverifiable;
        };
        if crate::stable_hash32(pubkey) != self.collector_id {
            return SignatureStatus::Invalid;
        }
        if crate::wallet::verify_signature(pubkey, &self.signing_payload(), &self.collector_signature) {
            SignatureStatus::Valid
        } else {
            SignatureStatus::Invalid
        }
    }
}

impl BatchBuilder {
    pub fn new(epoch_id: EpochId, collector_id: NodeId) -> Self {
        Self {
            epoch_id,
            collector_id,
            observations: Vec::new(),
        }
    }

    pub fn push(&mut self, observation: ObservationRecord) -> Result<(), NodePipelineError> {
        if observation.epoch_id != self.epoch_id {
            return Err(NodePipelineError::MismatchedEpoch {
                expected: self.epoch_id,
                actual: observation.epoch_id,
            });
        }
        if observation.collector_id != self.collector_id {
            return Err(NodePipelineError::MismatchedCollector {
                expected: self.collector_id,
                actual: observation.collector_id,
            });
        }

        self.observations.push(observation);
        Ok(())
    }

    pub fn finalize(
        self,
        submitted_at_height: Height,
    ) -> Result<AssembledBatch, NodePipelineError> {
        if self.observations.is_empty() {
            return Err(NodePipelineError::EmptyBatch);
        }

        let mut observations = self.observations;
        observations.sort_by_key(|item| (item.slot_id, item.app_id, item.observed_at_millis));

        let slot_start = observations.first().map(|item| item.slot_id).unwrap_or(0);
        let slot_end = observations.last().map(|item| item.slot_id).unwrap_or(0);

        let payload_bytes =
            borsh::to_vec(&observations).expect("observation payload serialization must succeed");
        let payload_hash = stable_hash32(&payload_bytes);
        let payload_cid = cid_from_hash(payload_hash, "batch-payload");

        let leaf_hashes = observations
            .iter()
            .map(|item| {
                let encoded =
                    borsh::to_vec(item).expect("observation leaf serialization must succeed");
                merkle_leaf_sha256(&encoded)
            })
            .collect::<Vec<_>>();
        let batch_root = merkle_root(&leaf_hashes);

        let batch_commit = BatchCommit {
            epoch_id: self.epoch_id,
            collector_id: self.collector_id,
            slot_start,
            slot_end,
            batch: MerkleCommitment {
                root: batch_root,
                leaf_count: observations.len() as u32,
            },
            payload_cid: payload_cid.clone(),
            obs_count: observations.len() as u32,
            submitted_at_height,
        };

        Ok(AssembledBatch {
            batch_commit,
            payload_hash,
            payload_cid,
            payload_bytes,
            observations,
        })
    }
}

pub fn stable_hash32(bytes: &[u8]) -> Hash32 {
    const SEEDS: [u64; 4] = [
        0xcbf29ce484222325,
        0x84222325cbf29ce4,
        0x9e3779b185ebca87,
        0x517cc1b727220a95,
    ];
    const PRIME: u64 = 0x0000_0100_0000_01B3;

    let mut out = [0u8; 32];
    for (index, seed) in SEEDS.iter().enumerate() {
        let mut acc = *seed;
        for byte in bytes {
            acc ^= *byte as u64;
            acc = acc.wrapping_mul(PRIME);
            acc ^= ((index as u64) + 1).wrapping_mul(0x9e37_79b9);
            acc = acc.rotate_left(5);
        }
        out[index * 8..(index + 1) * 8].copy_from_slice(&acc.to_le_bytes());
    }
    out
}

pub fn merkle_root(leaves: &[Hash32]) -> Hash32 {
    if leaves.is_empty() {
        return [0u8; 32];
    }

    let mut level = leaves.to_vec();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut index = 0;
        while index < level.len() {
            let left = level[index];
            let right = if index + 1 < level.len() {
                level[index + 1]
            } else {
                left
            };

            // Match the chain-side `chain/x/pole/types/merkle.go` algorithm:
            //   parent = sha256(0x01 || left || right)
            // Domain separator (0x01 for parents, 0x00 for leaves) prevents
            // second-preimage attacks where a leaf hash could collide with a
            // partial parent prefix.
            let mut hasher = Sha256::new();
            hasher.update([0x01u8]);
            hasher.update(left);
            hasher.update(right);
            next.push(hasher.finalize().into());
            index += 2;
        }
        level = next;
    }

    level[0]
}

/// Compute the Merkle leaf hash for a pre-encoded record.
///
/// `chain/x/pole/types/merkle.go::MerkleLeafFromRecord` does
///   leaf = sha256(0x00 || json.Marshal(record))
/// — domain separator `0x00` distinguishes leaves from parents at
/// the wire-format boundary.
///
/// The Rust off-chain records use `borsh::to_vec(record)` for the
/// record bytes; callers must pass that encoding (or any canonical
/// byte form) into this function. The result is what
/// [`merkle_root`] expects as input.
pub fn merkle_leaf_sha256(record_bytes: &[u8]) -> Hash32 {
    let mut hasher = Sha256::new();
    hasher.update([0x00u8]);
    hasher.update(record_bytes);
    hasher.finalize().into()
}

// --- Chain-side record JSON (byte-identical to Go json.Marshal) ----------
//
// The chain recomputes Merkle roots over the records **it stores** using
// `json.Marshal(record)` (chain/x/pole/types/merkle.go). For an
// off-chain Rust root to match the chain's recomputation, the leaf bytes
// must equal Go's `json.Marshal` of the corresponding chain proto struct:
//   - field names/order follow the proto `json:"..."` tags
//     (snake_case, declaration order);
//   - `omitempty` drops zero-valued fields;
//   - values are uint64/uint32 decimals and ASCII-safe strings (bech32
//     addresses / hex), so serde_json's escaping matches Go's (no `<`,
//     `>`, `&`, U+2028/2029 appear in these domains).

fn is_zero_u64(v: &u64) -> bool {
    *v == 0
}

fn is_zero_u128(v: &u128) -> bool {
    *v == 0
}

fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}

fn is_empty_str(v: &str) -> bool {
    v.is_empty()
}

#[derive(Serialize)]
struct ChainRewardRecordJson<'a> {
    #[serde(skip_serializing_if = "is_zero_u64")]
    epoch_id: u64,
    #[serde(skip_serializing_if = "is_empty_str")]
    recipient: &'a str,
    #[serde(skip_serializing_if = "is_zero_u128")]
    player_reward: u128,
    #[serde(skip_serializing_if = "is_zero_u128")]
    collect_reward: u128,
    #[serde(skip_serializing_if = "is_zero_u128")]
    store_reward: u128,
    #[serde(skip_serializing_if = "is_zero_u128")]
    verify_reward: u128,
    #[serde(skip_serializing_if = "is_zero_u128")]
    propose_reward: u128,
    #[serde(skip_serializing_if = "is_zero_u128")]
    slash_debit: u128,
    #[serde(skip_serializing_if = "is_zero_u128")]
    net_reward: u128,
}

#[derive(Serialize)]
struct ChainAggregateRecordJson {
    #[serde(skip_serializing_if = "is_zero_u64")]
    epoch_id: u64,
    #[serde(skip_serializing_if = "is_zero_u32")]
    app_id: u32,
    #[serde(skip_serializing_if = "is_zero_u64")]
    total_weight_units: u64,
    #[serde(skip_serializing_if = "is_zero_u64")]
    player_count: u64,
}

/// Serialize a reward record the way the chain's
/// `json.Marshal(types.RewardRecord{...})` does. `recipient` must be the
/// on-chain bech32 recipient (the chain cannot see the Rust `node_id`);
/// callers derive it with `cosmos::address::node_id_to_bech32`.
pub fn reward_record_to_chain_json(
    record: &RewardRecord,
    recipient: &str,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&ChainRewardRecordJson {
        epoch_id: record.epoch_id,
        recipient,
        player_reward: record.player_reward,
        collect_reward: record.collect_reward,
        store_reward: record.store_reward,
        verify_reward: record.verify_reward,
        propose_reward: record.propose_reward,
        slash_debit: record.slash_debit,
        net_reward: record.net_reward,
    })
}

/// Serialize an aggregate record the way the chain's
/// `json.Marshal(types.AggregateRecord{...})` does. Maps the Rust
/// aggregate fields onto the chain's 4-field record:
/// `total_weight_units = gvs_microunits`, `player_count = median_players`.
pub fn aggregate_record_to_chain_json(
    record: &AggregateRecord,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&ChainAggregateRecordJson {
        epoch_id: record.epoch_id,
        app_id: record.app_id,
        total_weight_units: record.gvs_microunits,
        player_count: record.median_players,
    })
}

pub fn cid_from_hash(hash: Hash32, namespace: &str) -> ContentId {
    format!("cid://{namespace}/{}", hex_lower(&hash))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    //! Cross-language Merkle golden-vector tests.
    //!
    //! Every expected hash below was computed independently in Python
    //! (sha256 + domain separators 0x00 / 0x01) and then verified to
    //! match `chain/x/pole/types/merkle.go`. The Rust side re-asserts
    //! them here so a regression on either side surfaces in CI.
    //!
    //! Python script that produced these vectors (kept for re-runs):
    //!
    //! ```text
    //! def leaf(b): return hashlib.sha256(b'\x00' + b).digest()
    //! def parent(l, r): return hashlib.sha256(b'\x01' + l + r).digest()
    //! def root(leaves):
    //!     if not leaves: return b'\x00' * 32
    //!     lvl = leaves[:]
    //!     while len(lvl) > 1:
    //!         nxt = []
    //!         for i in range(0, len(lvl), 2):
    //!             l = lvl[i]; r = lvl[i+1] if i+1 < len(lvl) else l
    //!             nxt.append(parent(l, r))
    //!         lvl = nxt
    //!     return lvl[0]
    //! ```
    use super::*;

    fn hex_of(bytes: &[u8]) -> String {
        hex_lower(bytes)
    }

    fn parse_hex_32(s: &str) -> Hash32 {
        let bytes = hex::decode(s).expect("valid hex");
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes);
        out
    }

    #[test]
    fn leaf_domain_separator_matches_chain_format() {
        // sha256(0x00 || "a") = 022a6979e6dab7aa5ae4c3e5e45f7e977112a7e63593820dbec1ec738a24f93c
        let leaf = merkle_leaf_sha256(b"a");
        assert_eq!(
            hex_of(&leaf),
            "022a6979e6dab7aa5ae4c3e5e45f7e977112a7e63593820dbec1ec738a24f93c"
        );
    }

    #[test]
    fn leaf_b_and_c_match_chain_format() {
        // sha256(0x00 || "b") = 57eb35615d47f34ec714cacdf5fd74608a5e8e102724e80b24b287c0c27b6a31
        // sha256(0x00 || "c") = 597fcb31282d34654c200d3418fca5705c648ebf326ec73d8ddef11841f876d8
        assert_eq!(
            hex_of(&merkle_leaf_sha256(b"b")),
            "57eb35615d47f34ec714cacdf5fd74608a5e8e102724e80b24b287c0c27b6a31"
        );
        assert_eq!(
            hex_of(&merkle_leaf_sha256(b"c")),
            "597fcb31282d34654c200d3418fca5705c648ebf326ec73d8ddef11841f876d8"
        );
    }

    #[test]
    fn root_empty_tree_is_all_zero() {
        let empty: [Hash32; 0] = [];
        let r = merkle_root(&empty);
        assert_eq!(r, [0u8; 32]);
    }

    #[test]
    fn root_single_leaf_equals_leaf_hash() {
        let leaf = merkle_leaf_sha256(b"a");
        let r = merkle_root(&[leaf]);
        assert_eq!(r, leaf);
    }

    #[test]
    fn root_two_leaves_matches_chain() {
        // sha256(0x01 || sha256(0x00||"a") || sha256(0x00||"b"))
        // = b137985ff484fb600db93107c77b0365c80d78f5b429ded0fd97361d077999eb
        let leaves = [merkle_leaf_sha256(b"a"), merkle_leaf_sha256(b"b")];
        let r = merkle_root(&leaves);
        assert_eq!(
            hex_of(&r),
            "b137985ff484fb600db93107c77b0365c80d78f5b429ded0fd97361d077999eb"
        );
    }

    #[test]
    fn root_three_leaves_odd_duplicates_last() {
        // Three leaves: parent pair (a,b) → p_ab; odd leaf c duplicates with
        // itself → p_cc; root = parent(p_ab, p_cc).
        // Expected: e9636069c740c9ff51625b01a0b040396d265a9b920cc6febdfa5ecc9f58ecce
        let leaves = [
            merkle_leaf_sha256(b"a"),
            merkle_leaf_sha256(b"b"),
            merkle_leaf_sha256(b"c"),
        ];
        let r = merkle_root(&leaves);
        assert_eq!(
            hex_of(&r),
            "e9636069c740c9ff51625b01a0b040396d265a9b920cc6febdfa5ecc9f58ecce"
        );
    }

    #[test]
    fn root_four_leaves_balanced() {
        // Four leaves: (a,b) and (c,d), then root = parent(p_ab, p_cd).
        // Expected: 33376a3bd63e9993708a84ddfe6c28ae58b83505dd1fed711bd924ec5a6239f0
        let leaves = [
            merkle_leaf_sha256(b"a"),
            merkle_leaf_sha256(b"b"),
            merkle_leaf_sha256(b"c"),
            merkle_leaf_sha256(b"d"),
        ];
        let r = merkle_root(&leaves);
        assert_eq!(
            hex_of(&r),
            "33376a3bd63e9993708a84ddfe6c28ae58b83505dd1fed711bd924ec5a6239f0"
        );
    }

    #[test]
    fn root_five_leaves_odd_duplicates_last() {
        // Five leaves: pairs (a,b), (c,d), then c5 duplicates with itself.
        // Expected: 605c72ca9351dd39f38678f4c1326df06d8fb1a58272792acaf70e8c191fb823
        let leaves = [
            merkle_leaf_sha256(b"a"),
            merkle_leaf_sha256(b"b"),
            merkle_leaf_sha256(b"c"),
            merkle_leaf_sha256(b"d"),
            merkle_leaf_sha256(b"e"),
        ];
        let r = merkle_root(&leaves);
        assert_eq!(
            hex_of(&r),
            "605c72ca9351dd39f38678f4c1326df06d8fb1a58272792acaf70e8c191fb823"
        );
    }

    #[test]
    fn root_32byte_leaves_match_chain() {
        // 32-byte leaves are the realistic input shape from sha256-hashed
        // record payloads.
        let aa = [0xaau8; 32];
        let bb = [0xbbu8; 32];
        let leaves = [merkle_leaf_sha256(&aa), merkle_leaf_sha256(&bb)];
        let r = merkle_root(&leaves);
        assert_eq!(
            hex_of(&r),
            "03938e2c8f758e6cae443d499b41c899c373eb0c0198bae61796a069f2b05904"
        );
    }

    #[test]
    fn round_trip_with_chain_algorithmic_spec() {
        // Sanity test: re-derive the leaf-then-parent algorithm from
        // scratch using the Rust primitives, then compare to the
        // packaged helper. If `merkle_leaf_sha256` or `merkle_root`
        // ever drift from the algorithm spec, this test trips first.
        use sha2::{Digest, Sha256};

        let raw_records: &[&[u8]] = &[b"a", b"b", b"c"];
        let expected_leaves: Vec<Hash32> = raw_records
            .iter()
            .map(|raw| {
                let mut h = Sha256::new();
                h.update([0x00u8]);
                h.update(raw);
                Hash32::from(h.finalize())
            })
            .collect();
        let computed_leaves: Vec<Hash32> = raw_records
            .iter()
            .map(|raw| merkle_leaf_sha256(raw))
            .collect();
        assert_eq!(computed_leaves, expected_leaves);

        // Build the root manually and compare.
        let l = computed_leaves.clone();
        let p01 = {
            let mut h = Sha256::new();
            h.update([0x01u8]);
            h.update(l[0]);
            h.update(l[1]);
            Hash32::from(h.finalize())
        };
        let p22 = {
            let mut h = Sha256::new();
            h.update([0x01u8]);
            h.update(l[2]);
            h.update(l[2]);
            Hash32::from(h.finalize())
        };
        let expected_root = {
            let mut h = Sha256::new();
            h.update([0x01u8]);
            h.update(p01);
            h.update(p22);
            Hash32::from(h.finalize())
        };
        assert_eq!(merkle_root(&computed_leaves), expected_root);
    }

    #[test]
    fn fixture_table_matches_chain_for_full_sweep() {
        // Table-driven version of the per-fixture tests above. Each
        // (input_records, expected_root_hex) pair must match
        // `chain/x/pole/types/merkle_test.go::TestMerkleRootFixtures`.
        let cases: &[(&[&[u8]], &str)] = &[
            (
                &[b"" as &[u8]; 0],
                "0000000000000000000000000000000000000000000000000000000000000000",
            ),
            (
                &[b"a" as &[u8]],
                "022a6979e6dab7aa5ae4c3e5e45f7e977112a7e63593820dbec1ec738a24f93c",
            ),
            (
                &[b"a" as &[u8], b"b"],
                "b137985ff484fb600db93107c77b0365c80d78f5b429ded0fd97361d077999eb",
            ),
            (
                &[b"a" as &[u8], b"b", b"c"],
                "e9636069c740c9ff51625b01a0b040396d265a9b920cc6febdfa5ecc9f58ecce",
            ),
            (
                &[b"a" as &[u8], b"b", b"c", b"d"],
                "33376a3bd63e9993708a84ddfe6c28ae58b83505dd1fed711bd924ec5a6239f0",
            ),
            (
                &[b"a" as &[u8], b"b", b"c", b"d", b"e"],
                "605c72ca9351dd39f38678f4c1326df06d8fb1a58272792acaf70e8c191fb823",
            ),
        ];
        for (records, expected_hex) in cases {
            let leaves: Vec<Hash32> = records.iter().map(|raw| merkle_leaf_sha256(raw)).collect();
            let r = merkle_root(&leaves);
            let actual = hex_of(&r);
            assert_eq!(
                &actual,
                expected_hex,
                "Merkle root drift for fixture with {} leaf(ves)",
                records.len()
            );
            // Cross-check the expected value parses to 32 bytes.
            assert_eq!(
                parse_hex_32(expected_hex).len(),
                32,
                "fixture hex must decode to 32 bytes"
            );
        }
    }

    fn sample_observation() -> ObservationRecord {
        ObservationRecord {
            epoch_id: 1,
            slot_id: 1,
            app_id: 730,
            source_kind: ActivitySourceKind::Steam,
            source_confidence_ppm: 1_000_000,
            observed_players: 500_000,
            observed_at_millis: 1_700_000_000_000,
            collector_id: [0x42u8; 32],
            raw_body_cid: "cid".to_string(),
            raw_body_hash: [0x22u8; 32],
            collector_signature: Vec::new(),
        }
    }

    #[test]
    fn collector_signature_status_distinguishes_dev_and_real() {
        // Empty signature.
        assert_eq!(
            sample_observation().verify_collector_signature(None),
            SignatureStatus::Empty
        );

        // Legacy 32-byte dev placeholder is recognised, never a failure.
        let mut dev = sample_observation();
        dev.collector_signature = [0xABu8; 32].to_vec();
        assert_eq!(
            dev.verify_collector_signature(None),
            SignatureStatus::DevPlaceholder
        );

        // Real-looking 64-byte signature without a known public key.
        let mut real = sample_observation();
        real.collector_signature = [0xCDu8; 64].to_vec();
        assert_eq!(
            real.verify_collector_signature(None),
            SignatureStatus::Unverifiable
        );

        // With a public key that does not match the collector id.
        assert_eq!(
            real.verify_collector_signature(Some(&[0x11u8; 32])),
            SignatureStatus::Invalid
        );
    }

    #[test]
    fn collector_signature_valid_roundtrip_with_identity() {
        let keypair = crate::wallet::KeyPair::from_seed(&[0x5Au8; 32]);
        let mut observation = sample_observation();
        observation.collector_id = crate::stable_hash32(&keypair.public);
        observation.collector_signature = keypair.sign(&observation.signing_payload());

        assert_eq!(
            observation.verify_collector_signature(Some(&keypair.public)),
            SignatureStatus::Valid
        );
        // Tampered payload must fail.
        let mut tampered = observation.clone();
        tampered.observed_players += 1;
        assert_eq!(
            tampered.verify_collector_signature(Some(&keypair.public)),
            SignatureStatus::Invalid
        );
    }

    #[test]
    fn reward_record_chain_json_matches_go_json_marshal() {
        // Byte-identical to Go's json.Marshal(types.RewardRecord{...}):
        // field order follows the proto declaration, omitempty drops
        // zero fields, values are decimals. Verified against Go output
        // in chain/tmp_golden and locked in
        // chain/x/pole/types/merkle_cross_language_test.go.
        let record = RewardRecord {
            epoch_id: 9,
            node_id: [0u8; 32],
            player_reward: 50,
            collect_reward: 0,
            store_reward: 0,
            verify_reward: 0,
            propose_reward: 0,
            slash_debit: 0,
            net_reward: 50,
        };
        let json = reward_record_to_chain_json(
            &record,
            "cosmos1xyqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq65su5v",
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(json).unwrap(),
            "{\"epoch_id\":9,\"recipient\":\"cosmos1xyqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq65su5v\",\"player_reward\":50,\"net_reward\":50}"
        );

        // Omitempty: all-zero amounts collapse to just epoch_id.
        let empty = RewardRecord {
            epoch_id: 9,
            node_id: [0u8; 32],
            player_reward: 0,
            collect_reward: 0,
            store_reward: 0,
            verify_reward: 0,
            propose_reward: 0,
            slash_debit: 0,
            net_reward: 0,
        };
        let json = reward_record_to_chain_json(&empty, "cosmos1abc").unwrap();
        assert_eq!(
            String::from_utf8(json).unwrap(),
            "{\"epoch_id\":9,\"recipient\":\"cosmos1abc\"}"
        );
    }

    #[test]
    fn aggregate_record_chain_json_matches_go_json_marshal() {
        // Byte-identical to Go's json.Marshal(types.AggregateRecord{...})
        // with total_weight_units = gvs_microunits, player_count =
        // median_players. Verified against Go output and locked in
        // chain/x/pole/types/merkle_cross_language_test.go.
        let record = AggregateRecord {
            epoch_id: 9,
            slot_id: 0,
            app_id: 730,
            gvs_tier: crate::node_gvs::GvsTier::Tier1,
            primary_source_kind: crate::primitives::ActivitySourceKind::Steam,
            source_confidence_ppm: 0,
            accepted_observations: 0,
            median_players: 2,
            base_glv_microunits: 0,
            tier_weight_ppm: 0,
            time_decay_ppm: 0,
            coverage_bonus_ppm: 0,
            gvs_microunits: 88,
            source_batch_root: [0u8; 32],
        };
        let json = aggregate_record_to_chain_json(&record).unwrap();
        assert_eq!(
            String::from_utf8(json).unwrap(),
            "{\"epoch_id\":9,\"app_id\":730,\"total_weight_units\":88,\"player_count\":2}"
        );

        // player_count = 0 is omitted (Go omitempty).
        let zero_players = AggregateRecord {
            median_players: 0,
            gvs_microunits: 5,
            ..record
        };
        let json = aggregate_record_to_chain_json(&zero_players).unwrap();
        assert_eq!(
            String::from_utf8(json).unwrap(),
            "{\"epoch_id\":9,\"app_id\":730,\"total_weight_units\":5}"
        );
    }
}
