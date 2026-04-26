use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::node_config::{NodeConfig, NodeConfigError};
use crate::node_daemon::{payload_path, retention_book_path};
use crate::storage_book::{validate_stored_payload_record, LocalRetentionBook, StorageBookError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionAuditRecord {
    pub payload_cid: String,
    pub epoch_id: u64,
    pub retention_until_epoch: u64,
    pub expected_size_bytes: u64,
    pub file_present: bool,
    pub size_matches: bool,
    pub payload_hash_matches: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionAuditArtifact {
    pub current_epoch: u64,
    pub retained_payload_count: usize,
    pub retrievable_payload_count: usize,
    pub missing_payload_count: usize,
    pub corrupted_payload_count: usize,
    pub all_retrievable: bool,
    pub records: Vec<RetentionAuditRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageChallengeRecord {
    pub payload_cid: String,
    pub epoch_id: u64,
    pub retention_until_epoch: u64,
    pub payload_retrievable: bool,
    pub receipt_payload_matches: bool,
    pub receipt_epoch_matches: bool,
    pub receipt_storer_matches: bool,
    pub receipt_retention_matches: bool,
    pub receipt_signature_matches: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageChallengeArtifact {
    pub current_epoch: u64,
    pub checked_payload_count: usize,
    pub passed_payload_count: usize,
    pub failed_payload_count: usize,
    pub all_passed: bool,
    pub records: Vec<StorageChallengeRecord>,
}

#[derive(Debug)]
pub enum NodeStorageAuditError {
    Config(NodeConfigError),
    Storage(StorageBookError),
    Io(io::Error),
    Json(serde_json::Error),
}

impl fmt::Display for NodeStorageAuditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(err) => write!(f, "config error: {err}"),
            Self::Storage(err) => write!(f, "storage error: {err}"),
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::Json(err) => write!(f, "json error: {err}"),
        }
    }
}

impl std::error::Error for NodeStorageAuditError {}

impl From<NodeConfigError> for NodeStorageAuditError {
    fn from(value: NodeConfigError) -> Self {
        Self::Config(value)
    }
}

impl From<StorageBookError> for NodeStorageAuditError {
    fn from(value: StorageBookError) -> Self {
        Self::Storage(value)
    }
}

impl From<io::Error> for NodeStorageAuditError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for NodeStorageAuditError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl RetentionAuditArtifact {
    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), NodeStorageAuditError> {
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

impl StorageChallengeArtifact {
    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), NodeStorageAuditError> {
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

pub fn audit_local_retention(
    config: &NodeConfig,
    current_epoch: u64,
) -> Result<RetentionAuditArtifact, NodeStorageAuditError> {
    let retention_book = LocalRetentionBook::load_or_default_json(
        retention_book_path(config),
        config.storage.quota_gb,
    )?;
    let mut records = retention_book
        .payloads
        .values()
        .filter(|record| current_epoch <= record.retention_until_epoch)
        .map(|record| audit_record(config, record))
        .collect::<Result<Vec<_>, _>>()?;
    records.sort_by(|left, right| left.payload_cid.cmp(&right.payload_cid));

    let retained_payload_count = records.len();
    let retrievable_payload_count = records
        .iter()
        .filter(|record| record.file_present && record.size_matches && record.payload_hash_matches)
        .count();
    let missing_payload_count = records.iter().filter(|record| !record.file_present).count();
    let corrupted_payload_count = records
        .iter()
        .filter(|record| {
            record.file_present && (!record.size_matches || !record.payload_hash_matches)
        })
        .count();
    let all_retrievable = missing_payload_count == 0 && corrupted_payload_count == 0;

    let artifact = RetentionAuditArtifact {
        current_epoch,
        retained_payload_count,
        retrievable_payload_count,
        missing_payload_count,
        corrupted_payload_count,
        all_retrievable,
        records,
    };
    artifact.save_json(retention_audit_artifact_path(config, current_epoch))?;
    Ok(artifact)
}

pub fn retention_audit_artifact_path(config: &NodeConfig, current_epoch: u64) -> PathBuf {
    Path::new(&config.runtime.data_dir)
        .join("retention-audits")
        .join(format!("epoch-{current_epoch:06}.json"))
}

pub fn run_local_storage_challenge(
    config: &NodeConfig,
    current_epoch: u64,
    retention_audit: &RetentionAuditArtifact,
) -> Result<StorageChallengeArtifact, NodeStorageAuditError> {
    let retention_book = LocalRetentionBook::load_or_default_json(
        retention_book_path(config),
        config.storage.quota_gb,
    )?;
    let local_node_id = config.node_id()?;
    let mut records = retention_book
        .payloads
        .values()
        .filter(|record| current_epoch <= record.retention_until_epoch)
        .map(|record| challenge_record(config, local_node_id, record, retention_audit))
        .collect::<Result<Vec<_>, _>>()?;
    records.sort_by(|left, right| left.payload_cid.cmp(&right.payload_cid));

    let checked_payload_count = records.len();
    let passed_payload_count = records
        .iter()
        .filter(|record| {
            record.payload_retrievable
                && record.receipt_payload_matches
                && record.receipt_epoch_matches
                && record.receipt_storer_matches
                && record.receipt_retention_matches
                && record.receipt_signature_matches
        })
        .count();
    let failed_payload_count = checked_payload_count.saturating_sub(passed_payload_count);
    let all_passed = failed_payload_count == 0;

    let artifact = StorageChallengeArtifact {
        current_epoch,
        checked_payload_count,
        passed_payload_count,
        failed_payload_count,
        all_passed,
        records,
    };
    artifact.save_json(storage_challenge_artifact_path(config, current_epoch))?;
    Ok(artifact)
}

pub fn storage_challenge_artifact_path(config: &NodeConfig, current_epoch: u64) -> PathBuf {
    Path::new(&config.runtime.data_dir)
        .join("storage-challenges")
        .join(format!("epoch-{current_epoch:06}.json"))
}

fn audit_record(
    config: &NodeConfig,
    record: &crate::storage_book::StoredPayloadRecord,
) -> Result<RetentionAuditRecord, NodeStorageAuditError> {
    let path = payload_path(config, &record.payload_cid);
    let file_present = path.exists();

    let (size_matches, payload_hash_matches) = if file_present {
        let payload_bytes = fs::read(&path)?;
        (
            payload_bytes.len() as u64 == record.size_bytes,
            crate::stable_hash32(&payload_bytes) == record.payload_hash,
        )
    } else {
        (false, false)
    };

    Ok(RetentionAuditRecord {
        payload_cid: record.payload_cid.clone(),
        epoch_id: record.epoch_id,
        retention_until_epoch: record.retention_until_epoch,
        expected_size_bytes: record.size_bytes,
        file_present,
        size_matches,
        payload_hash_matches,
    })
}

fn challenge_record(
    _config: &NodeConfig,
    local_node_id: [u8; 32],
    record: &crate::storage_book::StoredPayloadRecord,
    retention_audit: &RetentionAuditArtifact,
) -> Result<StorageChallengeRecord, NodeStorageAuditError> {
    let payload_retrievable = retention_audit
        .records
        .iter()
        .find(|audit_record| audit_record.payload_cid == record.payload_cid)
        .map(|audit_record| {
            audit_record.file_present
                && audit_record.size_matches
                && audit_record.payload_hash_matches
        })
        .unwrap_or(false);
    let receipt_validation = validate_stored_payload_record(record)?;

    Ok(StorageChallengeRecord {
        payload_cid: record.payload_cid.clone(),
        epoch_id: record.epoch_id,
        retention_until_epoch: record.retention_until_epoch,
        payload_retrievable,
        receipt_payload_matches: receipt_validation.payload_matches,
        receipt_epoch_matches: receipt_validation.epoch_matches,
        receipt_storer_matches: record.receipt.storer_id == local_node_id,
        receipt_retention_matches: receipt_validation.retention_matches,
        receipt_signature_matches: receipt_validation.signature_matches,
    })
}
