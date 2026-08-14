//! Verifies that the committed `vendor/core2` patch (a minimal MIT stub
//! that replaces the upstream no_std core2 via `[patch.crates-io]`) still
//! matches its recorded checksum. Path dependencies are not checksum-verified
//! by cargo, so this test provides the tamper-evidence instead.

use std::fs;

use sha2::{Digest, Sha256};

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[test]
fn vendor_core2_matches_recorded_checksum() {
    let manifest = fs::read_to_string("vendor/core2/.cargo-checksum.json")
        .expect("vendor/core2/.cargo-checksum.json should be present");
    let value: serde_json::Value =
        serde_json::from_str(&manifest).expect("checksum manifest should be valid JSON");
    let files = value["files"]
        .as_object()
        .expect("checksum manifest should have a files map");

    assert!(!files.is_empty(), "checksum manifest should not be empty");
    for (path, expected) in files {
        let content = fs::read(format!("vendor/core2/{path}"))
            .unwrap_or_else(|e| panic!("vendor/core2/{path} should be readable: {e}"));
        let actual = sha256_hex(&content);
        assert_eq!(
            actual,
            expected.as_str().unwrap(),
            "checksum mismatch for vendor/core2/{path}"
        );
    }
}
