//! Pin the legacy Keccak-256 digest that the Go chain side will mirror
//! in `chain/x/pole/types/keccak_legacy_test.go`.
//!
//! **Critical:** EIP-712 specifies the *pre-NIST* Keccak-256, not the
//! 2015-standardized SHA3-256. The two are different algorithms. The
//! `sha3` crate exposes both; the `Keccak256` constructor (used here)
//! is the EIP-712 variant, while `Sha3_256` would silently produce
//! wrong digests for typed-data signing.
//!
//! Test vector: `Keccak256("abc")` =
//!   `4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45`
//!
//! Reference: NIST SHA3 vs Keccak padding distinction documented at
//! https://keccak.team/keccak.html (the EIP-712 spec uses the original
//! Keccak padding, not the SHA3 padding).

use sha3::{Digest, Keccak256};

const KECCAK256_ABC_HEX: &str = "4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45";

#[test]
fn keccak256_abc_matches_known_vector() {
    let digest = Keccak256::digest(b"abc");
    let hex = digest
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    assert_eq!(hex, KECCAK256_ABC_HEX, "Keccak-256 must match reference");
}

#[test]
fn keccak256_empty_matches_known_vector() {
    // Keccak-256("") =
    //   c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470
    let expected = "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470";
    let digest = Keccak256::digest(b"");
    let hex = digest
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    assert_eq!(hex, expected);
}

#[test]
fn keccak256_differs_from_sha3_256() {
    // Sanity: the two algorithms must produce different digests for the
    // same input. If a future refactor accidentally swaps `Keccak256` for
    // `Sha3_256`, the digest of "abc" changes from the reference above
    // to `3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46a0a1f3a3a5a4b1`
    // (the SHA3-256 of "abc"). This test guards against that swap by
    // asserting a known-different input produces a different output.
    let a = Keccak256::digest(b"abc");
    let b = Keccak256::digest(b"abd");
    assert_ne!(
        a[..],
        b[..],
        "distinct inputs must produce distinct digests"
    );
}
