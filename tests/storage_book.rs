use pole_protocol_draft::{LocalRetentionBook, StorageBookError};

fn fixed32(byte: u8) -> [u8; 32] {
    [byte; 32]
}

#[test]
fn retention_book_records_payload_and_receipt() {
    let mut book = LocalRetentionBook::with_quota_gb(1);
    let payload = b"batch-payload-example";

    let record = book
        .record_batch_payload(fixed32(1), 5, 3, payload)
        .unwrap();

    assert_eq!(record.epoch_id, 5);
    assert_eq!(record.retention_until_epoch, 8);
    assert_eq!(record.receipt.epoch_id, 5);
    assert_eq!(record.receipt.storer_id, fixed32(1));
    assert_eq!(record.receipt.retention_until_epoch, 8);
    assert!(record.payload_cid.starts_with("cid://batch-payload/"));
    assert_eq!(book.used_bytes, payload.len() as u64);
}

#[test]
fn retention_book_prunes_expired_payloads() {
    let mut book = LocalRetentionBook::with_quota_gb(1);
    book.record_batch_payload(fixed32(2), 1, 1, b"old").unwrap();
    book.record_batch_payload(fixed32(2), 3, 4, b"new").unwrap();

    let removed = book.prune_expired(4);
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].retention_until_epoch, 2);
    assert_eq!(book.payloads.len(), 1);
}

#[test]
fn retention_book_enforces_quota() {
    let mut book = LocalRetentionBook::with_quota_gb(0);
    let err = book
        .record_batch_payload(fixed32(3), 1, 2, b"payload")
        .unwrap_err();

    assert!(matches!(err, StorageBookError::QuotaExceeded { .. }));
}

#[test]
fn retention_book_persists_to_json() {
    let path =
        std::env::temp_dir().join(format!("pole-retention-book-{}.json", std::process::id()));
    if path.exists() {
        std::fs::remove_file(&path).unwrap();
    }

    let mut book = LocalRetentionBook::with_quota_gb(1);
    let record = book
        .record_batch_payload(fixed32(4), 7, 2, b"persisted-payload")
        .unwrap();
    book.save_json(&path).unwrap();

    let loaded = LocalRetentionBook::load_or_default_json(&path, 1).unwrap();
    let restored = loaded.payloads.get(&record.payload_cid).unwrap();
    assert_eq!(restored.payload_hash, record.payload_hash);
    assert_eq!(restored.retention_until_epoch, record.retention_until_epoch);

    std::fs::remove_file(path).unwrap();
}
