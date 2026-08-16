use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::app_paths::InstallLayout;

/// Environment variable that explicitly pins the release-manifest
/// directory (operations/testing override).
pub const RELEASE_MANIFEST_DIR_ENV: &str = "POLE_RELEASE_MANIFEST_DIR";

/// GitHub repository owning the release assets (matches Cargo.toml
/// `repository`).
pub const RELEASE_REPO: &str = "q3874758/pole--1";

/// Resolves the release-manifest directory for `channel`, in order:
///
/// 1. `POLE_RELEASE_MANIFEST_DIR` environment override;
/// 2. the installed layout's `release-manifests` directory (a real
///    install ships the manifests next to the binaries);
/// 3. the in-tree `dist/release-manifests` (development builds — avoids
///    hitting the network during tests);
/// 4. a fresh pull from GitHub Releases (`latest/download/{channel}.json`
///    plus its `.sig` / `.cert` sidecars) cached under the update dir;
/// 5. the in-tree directory as a last-resort fallback.
///
/// The caller still verifies the manifest signature itself
/// (`verify_release_manifest_signature`); a sidecar-less pull is treated
/// as unsigned.
pub fn resolve_release_manifest_dir(
    layout: &InstallLayout,
    channel: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Ok(dir) = std::env::var(RELEASE_MANIFEST_DIR_ENV) {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    let installed = layout.root_dir.join("release-manifests");
    if installed.join(format!("{channel}.json")).exists() {
        return Ok(installed);
    }

    let in_tree = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("dist")
        .join("release-manifests");
    if in_tree.join(format!("{channel}.json")).exists() {
        return Ok(in_tree);
    }

    if let Some(cache) = fetch_release_manifest_from_github(layout, channel)? {
        return Ok(cache);
    }

    Ok(in_tree)
}

fn fetch_release_manifest_from_github(
    layout: &InstallLayout,
    channel: &str,
) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    let cache_dir = layout.update_dir.join("release-manifests");
    fs::create_dir_all(&cache_dir)?;

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    let base =
        format!("https://github.com/{RELEASE_REPO}/releases/latest/download/{channel}.json");

    let response = client.get(&base).send()?;
    if !response.status().is_success() {
        return Ok(None);
    }
    let manifest_bytes = response.bytes()?;
    fs::write(cache_dir.join(format!("{channel}.json")), &manifest_bytes)?;

    // Sidecar signature / certificate (optional — absence means unsigned).
    for suffix in [".sig", ".cert"] {
        let url = format!("{base}{suffix}");
        if let Ok(resp) = client.get(&url).send() {
            if resp.status().is_success() {
                if let Ok(bytes) = resp.bytes() {
                    let _ = fs::write(cache_dir.join(format!("{channel}.json{suffix}")), bytes);
                }
            }
        }
    }
    Ok(Some(cache_dir))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseArtifact {
    pub platform: String,
    pub kind: String,
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestSigning {
    pub scheme: String,
    #[serde(default)]
    pub issuer: Option<String>,
    #[serde(default)]
    pub identity_regexp: Option<String>,
    #[serde(default)]
    pub signature_file: Option<String>,
    #[serde(default)]
    pub certificate_file: Option<String>,
    #[serde(default)]
    pub transparency_log: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseManifest {
    pub channel: String,
    pub version: String,
    pub artifacts: Vec<ReleaseArtifact>,
    #[serde(default)]
    pub signature: String,
    #[serde(default)]
    pub signing: Option<ManifestSigning>,
}

pub fn release_manifest_path(manifest_dir: impl AsRef<Path>, channel: &str) -> PathBuf {
    manifest_dir.as_ref().join(format!("{channel}.json"))
}

pub fn load_release_manifest(
    path: impl AsRef<Path>,
) -> Result<ReleaseManifest, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn load_release_manifest_for_channel(
    manifest_dir: impl AsRef<Path>,
    channel: &str,
) -> Result<ReleaseManifest, Box<dyn std::error::Error>> {
    load_release_manifest(release_manifest_path(manifest_dir, channel))
}

pub fn version_is_newer(candidate: &str, current: &str) -> bool {
    let candidate_parts = parse_version(candidate);
    let current_parts = parse_version(current);
    candidate_parts > current_parts
}

fn parse_version(input: &str) -> Vec<u64> {
    input
        .split('.')
        .map(|segment| segment.parse::<u64>().unwrap_or(0))
        .collect()
}
