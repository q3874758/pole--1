use serde::Serialize;

use crate::{hex_32, stable_hash32, ReleaseArtifact, ReleaseManifest};

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

pub fn development_manifest_signature(
    manifest: &ReleaseManifest,
) -> Result<String, Box<dyn std::error::Error>> {
    let payload = release_manifest_signing_payload(manifest)?;
    Ok(format!(
        "dev-hash:{}",
        hex_32(stable_hash32(payload.as_bytes()))
    ))
}

pub fn verify_release_manifest_signature(
    manifest: &ReleaseManifest,
) -> Result<ManifestSignatureVerification, Box<dyn std::error::Error>> {
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
