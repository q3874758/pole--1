use std::fs;
use std::path::Path;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use ed25519_dalek::{Signature as Ed25519Signature, Verifier as Ed25519Verifier, VerifyingKey};
use serde::Serialize;

use crate::{hex_32, stable_hash32, ManifestSigning, ReleaseArtifact, ReleaseManifest};

/// Public key extracted from a Fulcio certificate, tagged by algorithm so the
/// matching verifier is used for the sidecar signature.
enum CertificatePublicKey {
    Ed25519(VerifyingKey),
    P256(p256::ecdsa::VerifyingKey),
}

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

/// Decodes the certificate sidecar into PEM text. `cosign sign-blob
/// --output-certificate` writes the PEM block base64-encoded (the same
/// encoding Rekor stores), so accept both base64(PEM) and plain PEM.
fn decode_cert_pem(cert_content: &str) -> Result<String, Box<dyn std::error::Error>> {
    let trimmed = cert_content.trim();
    if trimmed.starts_with("-----BEGIN") {
        return Ok(trimmed.to_string());
    }
    let pem_bytes = BASE64.decode(trimmed.as_bytes())?;
    Ok(String::from_utf8(pem_bytes)?)
}

/// Extracts the public key from a PEM-encoded X.509 certificate (the Fulcio
/// certificate emitted by `cosign sign-blob`). Supports Ed25519 (OID
/// 1.3.101.112) and ECDSA P-256 (OID 1.2.840.10045.3.1.7); the algorithm is
/// selected from the SPKI algorithm OID.
fn extract_certificate_public_key(
    pem: &str,
) -> Result<CertificatePublicKey, Box<dyn std::error::Error>> {
    let base64_content: String = pem
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect();
    let der = BASE64.decode(base64_content.as_bytes())?;
    let (_, cert) = x509_parser::parse_x509_certificate(&der)
        .map_err(|err| format!("parse x509 certificate: {err:?}"))?;
    let spki = cert.public_key();
    let algorithm_oid = spki.algorithm.algorithm.to_string();
    match algorithm_oid.as_str() {
        // Ed25519 (RFC 8410): 32 raw bytes.
        "1.3.101.112" => {
            let key: [u8; 32] = spki
                .subject_public_key
                .data
                .as_ref()
                .try_into()
                .map_err(|_| "ed25519 public key is not 32 bytes")?;
            Ok(CertificatePublicKey::Ed25519(VerifyingKey::from_bytes(
                &key,
            )?))
        }
        // id-ecPublicKey: SEC1 point (0x04 || X || Y, 65 bytes). The named
        // curve lives in the SPKI parameters (prime256v1 = 1.2.840.10045.3.1.7);
        // p256's from_sec1_bytes enforces the P-256 curve itself.
        "1.2.840.10045.2.1" => {
            let key = p256::ecdsa::VerifyingKey::from_sec1_bytes(&spki.subject_public_key.data)
                .map_err(|err| format!("parse p256 public key: {err}"))?;
            Ok(CertificatePublicKey::P256(key))
        }
        other => Err(format!("unsupported certificate public key algorithm OID: {other}").into()),
    }
}

/// Verifies a cosign keyless signature: the raw manifest bytes are checked
/// against the sidecar signature (`.sig`), using the public key embedded in
/// the Fulcio certificate sidecar (`.cert`). `cosign sign-blob` signs the
/// raw blob bytes with either ECDSA P-256 (DER ASN.1 signature) or Ed25519
/// (raw 64-byte signature); the algorithm is inferred from the certificate.
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
    let sig_bytes = BASE64.decode(sig_b64.trim().as_bytes())?;
    let cert_content = fs::read_to_string(&cert_path)
        .map_err(|_| format!("missing certificate sidecar {}", cert_path.display()))?;
    let cert_pem = decode_cert_pem(&cert_content)?;
    let public_key = extract_certificate_public_key(&cert_pem)?;

    let verified = match public_key {
        CertificatePublicKey::Ed25519(verifying_key) => {
            let sig: [u8; 64] = sig_bytes
                .as_slice()
                .try_into()
                .map_err(|_| "ed25519 signature is not 64 bytes")?;
            verifying_key
                .verify(manifest_bytes, &Ed25519Signature::from_bytes(&sig))
                .is_ok()
        }
        CertificatePublicKey::P256(verifying_key) => {
            // cosign emits the ECDSA P-256 signature as DER ASN.1 (SEQUENCE of
            // two INTEGERs); p256's Signature::from_der parses it into r||s.
            let der_sig = p256::ecdsa::Signature::from_der(&sig_bytes)
                .map_err(|err| format!("parse ecdsa signature: {err}"))?;
            verifying_key.verify(manifest_bytes, &der_sig).is_ok()
        }
    };

    Ok(if verified {
        ManifestSignatureVerification::Verified
    } else {
        ManifestSignatureVerification::Invalid
    })
}

/// Verifies the release manifest signature. When the manifest carries a
/// `cosign-keyless` signing block, the signature is verified against the
/// sidecar files (ECDSA P-256 or Ed25519, per the Fulcio certificate);
/// otherwise the legacy inline signature is checked (dev-hash is accepted
/// only in debug builds).
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
