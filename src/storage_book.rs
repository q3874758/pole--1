use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::node_pipeline::{cid_from_hash, stable_hash32};
use crate::primitives::{ContentId, EpochId, Hash32, NodeId, SignatureBytes};
use crate::records::ReplicaReceipt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct StoredPayloadRecord {
    pub epoch_id: EpochId,
    pub payload_cid: ContentId,
    pub payload_hash: Hash32,
    pub size_bytes: u64,
    pub retention_until_epoch: EpochId,
    pub receipt: ReplicaReceipt,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct LocalRetentionBook {
    pub quota_bytes: u64,
    pub used_bytes: u64,
    pub payloads: BTreeMap<ContentId, StoredPayloadRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredPayloadReceiptValidation {
    pub payload_matches: bool,
    pub epoch_matches: bool,
    pub retention_matches: bool,
    pub signature_matches: bool,
}

#[derive(Debug)]
pub enum StorageBookError {
    Io(io::Error),
    Json(serde_json::Error),
    Borsh(String),
    QuotaExceeded {
        quota_bytes: u64,
        used_bytes: u64,
        requested_bytes: u64,
    },
}

impl fmt::Display for StorageBookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::Json(err) => write!(f, "json error: {err}"),
            Self::Borsh(err) => write!(f, "borsh error: {err}"),
            Self::QuotaExceeded {
                quota_bytes,
                used_bytes,
                requested_bytes,
            } => write!(
                f,
                "quota exceeded: quota={quota_bytes} used={used_bytes} requested={requested_bytes}"
            ),
        }
    }
}

impl std::error::Error for StorageBookError {}

impl From<io::Error> for StorageBookError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for StorageBookError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl LocalRetentionBook {
    pub fn with_quota_gb(quota_gb: u32) -> Self {
        Self {
            quota_bytes: quota_gb as u64 * 1024 * 1024 * 1024,
            used_bytes: 0,
            payloads: BTreeMap::new(),
        }
    }

    pub fn load_or_default_json(
        path: impl AsRef<Path>,
        quota_gb: u32,
    ) -> Result<Self, StorageBookError> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self::with_quota_gb(quota_gb));
        }

        let content = fs::read_to_string(path)?;
        let mut book: Self = serde_json::from_str(&content)?;
        if book.quota_bytes == 0 {
            book.quota_bytes = quota_gb as u64 * 1024 * 1024 * 1024;
        }
        book.recalculate_used_bytes();
        Ok(book)
    }

    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), StorageBookError> {
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

    pub fn record_batch_payload(
        &mut self,
        storer_id: NodeId,
        epoch_id: EpochId,
        retention_epochs: u32,
        payload_bytes: &[u8],
    ) -> Result<StoredPayloadRecord, StorageBookError> {
        let payload_hash = stable_hash32(payload_bytes);
        let payload_cid = cid_from_hash(payload_hash, "batch-payload");
        self.record_payload(
            storer_id,
            epoch_id,
            retention_epochs,
            payload_cid,
            payload_hash,
            payload_bytes.len() as u64,
        )
    }

    pub fn record_payload(
        &mut self,
        storer_id: NodeId,
        epoch_id: EpochId,
        retention_epochs: u32,
        payload_cid: ContentId,
        payload_hash: Hash32,
        size_bytes: u64,
    ) -> Result<StoredPayloadRecord, StorageBookError> {
        if let Some(existing) = self.payloads.get(&payload_cid) {
            return Ok(existing.clone());
        }

        if self.used_bytes.saturating_add(size_bytes) > self.quota_bytes {
            return Err(StorageBookError::QuotaExceeded {
                quota_bytes: self.quota_bytes,
                used_bytes: self.used_bytes,
                requested_bytes: size_bytes,
            });
        }

        let retention_until_epoch = epoch_id + retention_epochs as u64;
        let receipt_signature = development_receipt_signature(
            storer_id,
            epoch_id,
            retention_until_epoch,
            &payload_cid,
            payload_hash,
            size_bytes,
        )?;
        let receipt = ReplicaReceipt {
            epoch_id,
            payload_cid: payload_cid.clone(),
            storer_id,
            retention_until_epoch,
            receipt_signature,
        };

        let record = StoredPayloadRecord {
            epoch_id,
            payload_cid: payload_cid.clone(),
            payload_hash,
            size_bytes,
            retention_until_epoch,
            receipt,
        };
        self.payloads.insert(payload_cid, record.clone());
        self.used_bytes += size_bytes;
        Ok(record)
    }

    pub fn prune_expired(&mut self, current_epoch: EpochId) -> Vec<StoredPayloadRecord> {
        let expired_keys = self
            .payloads
            .iter()
            .filter(|(_, record)| current_epoch > record.retention_until_epoch)
            .map(|(cid, _)| cid.clone())
            .collect::<Vec<_>>();

        let mut removed = Vec::with_capacity(expired_keys.len());
        for key in expired_keys {
            if let Some(record) = self.payloads.remove(&key) {
                self.used_bytes = self.used_bytes.saturating_sub(record.size_bytes);
                removed.push(record);
            }
        }
        removed
    }

    fn recalculate_used_bytes(&mut self) {
        self.used_bytes = self.payloads.values().map(|record| record.size_bytes).sum();
    }
}

pub fn validate_stored_payload_record(
    record: &StoredPayloadRecord,
) -> Result<StoredPayloadReceiptValidation, StorageBookError> {
    Ok(StoredPayloadReceiptValidation {
        payload_matches: record.receipt.payload_cid == record.payload_cid,
        epoch_matches: record.receipt.epoch_id == record.epoch_id,
        retention_matches: record.receipt.retention_until_epoch == record.retention_until_epoch,
        signature_matches: record.receipt.receipt_signature
            == development_receipt_signature(
                record.receipt.storer_id,
                record.epoch_id,
                record.retention_until_epoch,
                &record.payload_cid,
                record.payload_hash,
                record.size_bytes,
            )?,
    })
}

fn development_receipt_signature(
    storer_id: NodeId,
    epoch_id: EpochId,
    retention_until_epoch: EpochId,
    payload_cid: &str,
    payload_hash: Hash32,
    size_bytes: u64,
) -> Result<SignatureBytes, StorageBookError> {
    let encoded = borsh::to_vec(&(
        storer_id,
        epoch_id,
        retention_until_epoch,
        payload_cid,
        payload_hash,
        size_bytes,
    ))
    .map_err(|err| StorageBookError::Borsh(err.to_string()))?;
    Ok(stable_hash32(&encoded).to_vec())
}
