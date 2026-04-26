use std::fmt;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::node_config::NodeConfig;
use crate::node_daemon::{CollectTickArtifact, NodeDaemonError};
use crate::node_pipeline::{merkle_root, stable_hash32};
use crate::primitives::Hash32;
use crate::records::ObservationRecord;
use crate::storage_book::{LocalRetentionBook, StorageBookError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchVerificationReport {
    pub epoch_id: u64,
    pub slot_id: u64,
    pub payload_cid: String,
    pub payload_hash_matches: bool,
    pub batch_root_matches: bool,
    pub obs_count_matches: bool,
    pub retention_record_present: bool,
    pub retention_hash_matches: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpochVerificationReport {
    pub epoch_id: u64,
    pub batch_count: usize,
    pub stored_payload_count: usize,
    pub all_valid: bool,
    pub reports: Vec<BatchVerificationReport>,
}

#[derive(Debug)]
pub enum NodeVerificationError {
    Upstream(String),
    Storage(StorageBookError),
    Io(std::io::Error),
    Json(serde_json::Error),
    Borsh(String),
    InvalidHexLength { expected: usize, actual: usize },
    InvalidHexCharacter { index: usize, byte: u8 },
}

impl fmt::Display for NodeVerificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Upstream(err) => write!(f, "upstream error: {err}"),
            Self::Storage(err) => write!(f, "storage error: {err}"),
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::Json(err) => write!(f, "json error: {err}"),
            Self::Borsh(err) => write!(f, "borsh error: {err}"),
            Self::InvalidHexLength { expected, actual } => {
                write!(f, "invalid hex length: expected {expected}, got {actual}")
            }
            Self::InvalidHexCharacter { index, byte } => {
                write!(f, "invalid hex character at {index}: 0x{byte:02x}")
            }
        }
    }
}

impl std::error::Error for NodeVerificationError {}

impl From<NodeDaemonError> for NodeVerificationError {
    fn from(value: NodeDaemonError) -> Self {
        Self::Upstream(value.to_string())
    }
}

impl From<StorageBookError> for NodeVerificationError {
    fn from(value: StorageBookError) -> Self {
        Self::Storage(value)
    }
}

impl From<std::io::Error> for NodeVerificationError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for NodeVerificationError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl EpochVerificationReport {
    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), NodeVerificationError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }
}

pub fn verify_local_epoch(
    config: &NodeConfig,
    epoch_id: u64,
) -> Result<EpochVerificationReport, NodeVerificationError> {
    let retention_book = LocalRetentionBook::load_or_default_json(
        crate::retention_book_path(config),
        config.storage.quota_gb,
    )?;

    let mut reports = Vec::new();
    let batches_dir = Path::new(&config.runtime.data_dir).join("batches");
    if batches_dir.exists() {
        let mut entries = fs::read_dir(&batches_dir)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.path());

        for entry in entries {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let artifact =
                CollectTickArtifact::load_json(&path).map_err(NodeVerificationError::from)?;
            if artifact.epoch_id != epoch_id {
                continue;
            }

            let payload_file = crate::payload_path(config, &artifact.payload_cid);
            let payload_bytes = fs::read(&payload_file)?;
            let observations = borsh::from_slice::<Vec<ObservationRecord>>(&payload_bytes)
                .map_err(|err| NodeVerificationError::Borsh(err.to_string()))?;
            let payload_hash = stable_hash32(&payload_bytes);
            let expected_payload_hash = decode_hex_32(&artifact.payload_hash_hex)?;

            let observation_hashes = observations
                .iter()
                .map(|observation| {
                    stable_hash32(&borsh::to_vec(observation).expect("observation encoding"))
                })
                .collect::<Vec<_>>();
            let batch_root = merkle_root(&observation_hashes);
            let expected_batch_root = decode_hex_32(&artifact.batch_root_hex)?;

            let retention_record = retention_book.payloads.get(&artifact.payload_cid);
            let retention_hash_matches = retention_record
                .map(|record| record.payload_hash == payload_hash)
                .unwrap_or(false);

            reports.push(BatchVerificationReport {
                epoch_id: artifact.epoch_id,
                slot_id: artifact.slot_id,
                payload_cid: artifact.payload_cid,
                payload_hash_matches: payload_hash == expected_payload_hash,
                batch_root_matches: batch_root == expected_batch_root,
                obs_count_matches: observations.len() as u32 == artifact.obs_count,
                retention_record_present: retention_record.is_some(),
                retention_hash_matches,
            });
        }
    }

    let all_valid = !reports.is_empty()
        && reports.iter().all(|report| {
            report.payload_hash_matches
                && report.batch_root_matches
                && report.obs_count_matches
                && report.retention_record_present
                && report.retention_hash_matches
        });

    Ok(EpochVerificationReport {
        epoch_id,
        batch_count: reports.len(),
        stored_payload_count: retention_book
            .payloads
            .values()
            .filter(|record| record.epoch_id == epoch_id)
            .count(),
        all_valid,
        reports,
    })
}

fn decode_hex_32(input: &str) -> Result<Hash32, NodeVerificationError> {
    if input.len() != 64 {
        return Err(NodeVerificationError::InvalidHexLength {
            expected: 64,
            actual: input.len(),
        });
    }

    let mut out = [0u8; 32];
    let bytes = input.as_bytes();
    for index in 0..32 {
        let hi = decode_nibble(bytes[index * 2], index * 2)?;
        let lo = decode_nibble(bytes[index * 2 + 1], index * 2 + 1)?;
        out[index] = (hi << 4) | lo;
    }
    Ok(out)
}

fn decode_nibble(byte: u8, index: usize) -> Result<u8, NodeVerificationError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(NodeVerificationError::InvalidHexCharacter { index, byte }),
    }
}
