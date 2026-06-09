use std::fmt;

use sha2::{Digest, Sha256};

use crate::primitives::{
    ActivitySourceKind, AppId, ContentId, EpochId, Hash32, Height, NodeId, SignatureBytes, SlotId,
    UnixMillis,
};
use crate::records::{BatchCommit, ObservationRecord};
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
}
