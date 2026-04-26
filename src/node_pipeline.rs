use std::fmt;

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
                stable_hash32(&encoded)
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

            let mut pair = Vec::with_capacity(65);
            pair.push(0x01);
            pair.extend_from_slice(&left);
            pair.extend_from_slice(&right);
            next.push(stable_hash32(&pair));
            index += 2;
        }
        level = next;
    }

    level[0]
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
