use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseArtifact {
    pub platform: String,
    pub kind: String,
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseManifest {
    pub channel: String,
    pub version: String,
    pub artifacts: Vec<ReleaseArtifact>,
    pub signature: String,
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
