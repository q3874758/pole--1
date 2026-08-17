//! Signature verification against fixtures captured from the real
//! published v0.1.0 GitHub Release (`stable.json` + cosign keyless
//! sidecars). These fixtures pin the exact on-disk formats cosign
//! produces — the `.sig` is a base64 DER ECDSA P-256 signature and the
//! `.cert` is a base64-encoded PEM Fulcio certificate — so the
//! verification path is exercised end-to-end without network.
use std::path::PathBuf;

use pole_protocol_draft::{
    load_release_manifest, verify_release_manifest_signature, ManifestSignatureVerification,
};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// The real v0.1.0 `stable.json` was signed by cosign keyless with an
/// ECDSA P-256 key (Fulcio certificate). This must verify as
/// `manifest_signed`; before the ECDSA support landed, the same assets
/// produced `invalid_signature` because the verifier only tried Ed25519.
#[test]
fn verifies_real_cosign_ecdsa_release_manifest() {
    let manifest_path = fixture("stable.json");
    let manifest = load_release_manifest(&manifest_path).unwrap();
    assert_eq!(manifest.version, "0.1.0");
    let signing = manifest.signing.as_ref().expect("signing block");
    assert_eq!(signing.scheme, "cosign-keyless");

    let verification = verify_release_manifest_signature(&manifest, &manifest_path).unwrap();
    assert_eq!(
        verification,
        ManifestSignatureVerification::Verified,
        "real cosign ECDSA P-256 signature must verify"
    );
}

/// Tampering with the manifest bytes must flip the verdict to invalid
/// (the sidecar signature no longer matches the content).
#[test]
fn tampered_manifest_fails_cosign_ecdsa_verification() {
    let dir = tempfile::tempdir().unwrap();
    let manifest_path = dir.path().join("stable.json");
    let mut manifest_bytes = std::fs::read(fixture("stable.json")).unwrap();
    // Mutate one hex digit of the first sha256 value, keeping the JSON
    // well-formed so the manifest still parses.
    let needle = b"\"sha256\": \"".to_vec();
    let start = manifest_bytes
        .windows(needle.len())
        .position(|w| w == needle)
        .expect("sha256 field present");
    let hex_start = start + needle.len();
    let digit = manifest_bytes[hex_start];
    manifest_bytes[hex_start] = if digit == b'0' { b'1' } else { b'0' };
    std::fs::write(&manifest_path, &manifest_bytes).unwrap();
    std::fs::copy(
        fixture("stable.json.sig"),
        dir.path().join("stable.json.sig"),
    )
    .unwrap();
    std::fs::copy(
        fixture("stable.json.cert"),
        dir.path().join("stable.json.cert"),
    )
    .unwrap();

    let manifest = load_release_manifest(&manifest_path).unwrap();
    let verification = verify_release_manifest_signature(&manifest, &manifest_path).unwrap();
    assert_eq!(verification, ManifestSignatureVerification::Invalid);
}
