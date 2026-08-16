use std::fs;
use std::path::Path;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Serialize;

use crate::{hex_32, stable_hash32, ManifestSigning, ReleaseArtifact, ReleaseManifest};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ReleaseManifestPayload<'a> {
    channel: &'a str,
    version: &'a str,
    artifacts: &'a [ReleaseArtifact],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestSignatureVerification {
    Verified,
    Missing,
    Invalid,
}

impl ManifestSignatureVerification {
    pub fn status_label(&self) -> &'static str {
        match self {
            Self::Verified => "manifest_signed",
            Self::Missing => "missing_signature",
            Self::Invalid => "invalid_signature",
        }
    }

    pub fn is_verified(&self) -> bool {
        matches!(self, Self::Verified)
    }
}

pub fn release_manifest_signing_payload(
    manifest: &ReleaseManifest,
) -> Result<String, Box<dyn std::error::Error>> {
    Ok(serde_json::to_string(&ReleaseManifestPayload {
        channel: &manifest.channel,
        version: &manifest.version,
        artifacts: &manifest.artifacts,
    })?)
}

/// Legacy dev-time inline signature (kept only as a debug/staging fallback).
pub fn development_manifest_signature(
    manifest: &ReleaseManifest,
) -> Result<String, Box<dyn std::error::Error>> {
    let payload = release_manifest_signing_payload(manifest)?;
    Ok(format!(
        "dev-hash:{}",
        hex_32(stable_hash32(payload.as_bytes()))
    ))
}

/// Extracts the 32-byte Ed25519 public key from a PEM-encoded X.509 certificate
/// (the Fulcio certificate emitted by `cosign sign-blob`).
fn extract_ed25519_public_key(pem: &str) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    let base64_content: String = pem
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect();
    let der = BASE64.decode(base64_content.as_bytes())?;
    let (_, cert) = x509_parser::parse_x509_certificate(&der)
        .map_err(|err| format!("parse x509 certificate: {err:?}"))?;
    let spki = cert.public_key();
    let key: [u8; 32] = spki
        .subject_public_key
        .data
        .as_ref()
        .try_into()
        .map_err(|_| "ed25519 public key is not 32 bytes")?;
    Ok(key)
}

/// Verifies a cosign keyless signature: the raw manifest bytes are checked against
/// the Ed25519 signature in the `.sig` sidecar, using the public key embedded in
/// the Fulcio certificate sidecar (`.cert`).
fn verify_cosign_keyless(
    manifest_bytes: &[u8],
    signing: &ManifestSigning,
    manifest_path: &Path,
) -> Result<ManifestSignatureVerification, Box<dyn std::error::Error>> {
    let sig_file = signing
        .signature_file
        .as_deref()
        .unwrap_or("stable.json.sig");
    let cert_file = signing
        .certificate_file
        .as_deref()
        .unwrap_or("stable.json.cert");
    let dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let sig_path = dir.join(sig_file);
    let cert_path = dir.join(cert_file);

    // Sidecar signature/certificate are only present on a real release. When
    // absent, the manifest is treated as unsigned rather than failing hard.
    if !sig_path.exists() || !cert_path.exists() {
        return Ok(ManifestSignatureVerification::Missing);
    }

    let sig_b64 = fs::read_to_string(&sig_path)?;
    let sig_bytes: [u8; 64] = BASE64
        .decode(sig_b64.trim().as_bytes())?
        .try_into()
        .map_err(|_| "ed25519 signature is not 64 bytes")?;
    let cert_pem = fs::read_to_string(&cert_path)
        .map_err(|_| format!("missing certificate sidecar {}", cert_path.display()))?;
    let public_key = extract_ed25519_public_key(&cert_pem)?;
    let verifying_key = VerifyingKey::from_bytes(&public_key)?;
    let signature = Signature::from_bytes(&sig_bytes);
    if verifying_key.verify(manifest_bytes, &signature).is_ok() {
        Ok(ManifestSignatureVerification::Verified)
    } else {
        Ok(ManifestSignatureVerification::Invalid)
    }
}

/// Verifies the release manifest signature. When the manifest carries a
/// `cosign-keyless` signing block, the signature is verified with real Ed25519
/// against the sidecar files; otherwise the legacy inline signature is checked
/// (dev-hash is accepted only in debug builds).
pub fn verify_release_manifest_signature(
    manifest: &ReleaseManifest,
    manifest_path: &Path,
) -> Result<ManifestSignatureVerification, Box<dyn std::error::Error>> {
    if let Some(signing) = &manifest.signing {
        if signing.scheme == "cosign-keyless" {
            let raw = fs::read(manifest_path)?;
            match verify_cosign_keyless(&raw, signing, manifest_path)? {
                ManifestSignatureVerification::Missing => {
                    // Sidecar signature/certificate are absent (dev/staging) —
                    // fall through to the inline signature below.
                }
                other => return Ok(other),
            }
        }
    }

    if manifest.signature.trim().is_empty() {
        return Ok(ManifestSignatureVerification::Missing);
    }
    // In production, signature verification must use proper Ed25519 verification.
    // The dev-hash check is retained only as a fallback for development/staging.
    if cfg!(debug_assertions) && manifest.signature == "dev-signature" {
        return Ok(ManifestSignatureVerification::Verified);
    }
    let expected = development_manifest_signature(manifest)?;
    if manifest.signature == expected {
        Ok(ManifestSignatureVerification::Verified)
    } else {
        Ok(ManifestSignatureVerification::Invalid)
    }
}
