//! Activity collection: third-party / community player-count ingestion.
//!
//! # Trust model (dependency-free)
//!
//! The client deliberately stays small: no heavyweight proving/MPC
//! dependencies are introduced. Trust relies on two cheap, local
//! mechanisms:
//!
//! - **multi-source cross-validation**: when two or more sources report the
//!   same app id, outliers get their confidence lowered automatically;
//! - **per-source health tracking** (in the node daemon) that marks a source
//!   degraded after consecutive failures.

use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;

use crate::node_pipeline::ActivitySample;
use crate::primitives::{ActivitySourceKind, AppId, UnixMillis};
use crate::steam_collector::{parse_current_players_response, HttpTextClient, SteamCollectorError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityCollectorError {
    Http(String),
    InvalidJson(String),
    MissingPlayerCount,
    MissingConfidence,
    MissingEndpoint,
    MissingInlineJson,
}

impl fmt::Display for ActivityCollectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(err) => write!(f, "activity http error: {err}"),
            Self::InvalidJson(err) => write!(f, "invalid activity response json: {err}"),
            Self::MissingPlayerCount => write!(f, "activity response missing player count"),
            Self::MissingConfidence => write!(f, "activity response missing confidence ppm"),
            Self::MissingEndpoint => write!(f, "activity source missing endpoint url"),
            Self::MissingInlineJson => write!(f, "community activity source missing inline json"),
        }
    }
}

impl std::error::Error for ActivityCollectorError {}

impl From<SteamCollectorError> for ActivityCollectorError {
    fn from(value: SteamCollectorError) -> Self {
        Self::Http(value.to_string())
    }
}

pub trait ActivityCollector {
    fn source_kind(&self) -> ActivitySourceKind;
    fn collect(
        &self,
        app_id: AppId,
        observed_at_millis: UnixMillis,
        raw_body: &str,
    ) -> Result<ActivitySample, ActivityCollectorError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThirdPartyJsonCollector {
    source_kind: ActivitySourceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CommunityJsonCollector;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EpicLiveCollector;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EaLiveCollector;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GogLiveCollector;

#[derive(Debug, Deserialize)]
struct ThirdPartyActivityEnvelope {
    player_count: Option<u64>,
    confidence_ppm: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct CommunityActivityEnvelope {
    estimated_players: Option<u64>,
    confidence_ppm: Option<u32>,
}

pub fn parse_third_party_activity_response(
    app_id: AppId,
    observed_at_millis: UnixMillis,
    raw_body: &str,
    source_kind: ActivitySourceKind,
) -> Result<ActivitySample, ActivityCollectorError> {
    let envelope: ThirdPartyActivityEnvelope = serde_json::from_str(raw_body)
        .map_err(|err| ActivityCollectorError::InvalidJson(err.to_string()))?;
    let observed_players = envelope
        .player_count
        .ok_or(ActivityCollectorError::MissingPlayerCount)?;
    let source_confidence_ppm = envelope
        .confidence_ppm
        .ok_or(ActivityCollectorError::MissingConfidence)?;
    Ok(ActivitySample::new(
        app_id,
        observed_players,
        observed_at_millis,
        raw_body,
        source_kind,
        source_confidence_ppm,
    ))
}

pub fn parse_community_activity_response(
    app_id: AppId,
    observed_at_millis: UnixMillis,
    raw_body: &str,
) -> Result<ActivitySample, ActivityCollectorError> {
    let envelope: CommunityActivityEnvelope = serde_json::from_str(raw_body)
        .map_err(|err| ActivityCollectorError::InvalidJson(err.to_string()))?;
    let observed_players = envelope
        .estimated_players
        .ok_or(ActivityCollectorError::MissingPlayerCount)?;
    let source_confidence_ppm = envelope
        .confidence_ppm
        .ok_or(ActivityCollectorError::MissingConfidence)?;
    Ok(ActivitySample::new(
        app_id,
        observed_players,
        observed_at_millis,
        raw_body,
        ActivitySourceKind::Community,
        source_confidence_ppm,
    ))
}

impl ThirdPartyJsonCollector {
    pub fn new(source_kind: ActivitySourceKind) -> Self {
        Self { source_kind }
    }
}

impl ActivityCollector for ThirdPartyJsonCollector {
    fn source_kind(&self) -> ActivitySourceKind {
        self.source_kind
    }

    fn collect(
        &self,
        app_id: AppId,
        observed_at_millis: UnixMillis,
        raw_body: &str,
    ) -> Result<ActivitySample, ActivityCollectorError> {
        parse_third_party_activity_response(app_id, observed_at_millis, raw_body, self.source_kind)
    }
}

impl ActivityCollector for CommunityJsonCollector {
    fn source_kind(&self) -> ActivitySourceKind {
        ActivitySourceKind::Community
    }

    fn collect(
        &self,
        app_id: AppId,
        observed_at_millis: UnixMillis,
        raw_body: &str,
    ) -> Result<ActivitySample, ActivityCollectorError> {
        parse_community_activity_response(app_id, observed_at_millis, raw_body)
    }
}

pub trait LiveActivityCollector {
    fn source_kind(&self) -> ActivitySourceKind;
    fn fetch(
        &self,
        http: &dyn HttpTextClient,
        app_id: AppId,
        observed_at_millis: UnixMillis,
        endpoint_url: &str,
    ) -> Result<ActivitySample, ActivityCollectorError>;
}

impl LiveActivityCollector for EpicLiveCollector {
    fn source_kind(&self) -> ActivitySourceKind {
        ActivitySourceKind::Epic
    }

    fn fetch(
        &self,
        http: &dyn HttpTextClient,
        app_id: AppId,
        observed_at_millis: UnixMillis,
        endpoint_url: &str,
    ) -> Result<ActivitySample, ActivityCollectorError> {
        let raw_body = http.get_text(endpoint_url)?;
        parse_third_party_activity_response(
            app_id,
            observed_at_millis,
            &raw_body,
            self.source_kind(),
        )
    }
}

impl LiveActivityCollector for EaLiveCollector {
    fn source_kind(&self) -> ActivitySourceKind {
        ActivitySourceKind::Ea
    }

    fn fetch(
        &self,
        http: &dyn HttpTextClient,
        app_id: AppId,
        observed_at_millis: UnixMillis,
        endpoint_url: &str,
    ) -> Result<ActivitySample, ActivityCollectorError> {
        let raw_body = http.get_text(endpoint_url)?;
        parse_third_party_activity_response(
            app_id,
            observed_at_millis,
            &raw_body,
            self.source_kind(),
        )
    }
}

impl LiveActivityCollector for GogLiveCollector {
    fn source_kind(&self) -> ActivitySourceKind {
        ActivitySourceKind::Gog
    }

    fn fetch(
        &self,
        http: &dyn HttpTextClient,
        app_id: AppId,
        observed_at_millis: UnixMillis,
        endpoint_url: &str,
    ) -> Result<ActivitySample, ActivityCollectorError> {
        let raw_body = http.get_text(endpoint_url)?;
        parse_third_party_activity_response(
            app_id,
            observed_at_millis,
            &raw_body,
            self.source_kind(),
        )
    }
}

pub fn collect_configured_activity_source(
    http: &dyn HttpTextClient,
    source_kind: ActivitySourceKind,
    app_id: AppId,
    observed_at_millis: UnixMillis,
    endpoint_url: Option<&str>,
    inline_json: Option<&str>,
) -> Result<ActivitySample, ActivityCollectorError> {
    match source_kind {
        ActivitySourceKind::Steam => {
            let endpoint = endpoint_url.ok_or(ActivityCollectorError::MissingEndpoint)?;
            let raw_body = http.get_text(endpoint)?;
            parse_current_players_response(app_id, observed_at_millis, &raw_body)
                .map_err(Into::into)
        }
        ActivitySourceKind::Epic => EpicLiveCollector.fetch(
            http,
            app_id,
            observed_at_millis,
            endpoint_url.ok_or(ActivityCollectorError::MissingEndpoint)?,
        ),
        ActivitySourceKind::Ea => EaLiveCollector.fetch(
            http,
            app_id,
            observed_at_millis,
            endpoint_url.ok_or(ActivityCollectorError::MissingEndpoint)?,
        ),
        ActivitySourceKind::Gog => GogLiveCollector.fetch(
            http,
            app_id,
            observed_at_millis,
            endpoint_url.ok_or(ActivityCollectorError::MissingEndpoint)?,
        ),
        ActivitySourceKind::Community => parse_community_activity_response(
            app_id,
            observed_at_millis,
            inline_json.ok_or(ActivityCollectorError::MissingInlineJson)?,
        ),
    }
}

/// Relative disagreement threshold (ppm): a source whose player count
/// differs from the strongest source for the same app id by more than this
/// is treated as an outlier.
pub const CROSS_VALIDATION_DISAGREEMENT_THRESHOLD_PPM: u64 = 250_000; // 25%

/// Confidence downscale applied to an outlier source (ppm).
pub const CROSS_VALIDATION_CONFIDENCE_DOWNSCALE_PPM: u32 = 500_000; // 50%

/// Cheap, dependency-free cross-validation: for every app id with two or
/// more samples, lower the confidence of sources that disagree strongly
/// with the strongest sample. Returns how many samples were adjusted.
pub fn cross_validate_samples(samples: &mut [ActivitySample]) -> usize {
    let mut by_app: BTreeMap<AppId, Vec<usize>> = BTreeMap::new();
    for (index, sample) in samples.iter().enumerate() {
        by_app.entry(sample.app_id).or_default().push(index);
    }

    let mut adjusted = 0;
    for indexes in by_app.values() {
        if indexes.len() < 2 {
            continue;
        }
        let strongest = indexes
            .iter()
            .map(|&index| samples[index].observed_players)
            .max()
            .unwrap_or(0)
            .max(1);
        for &index in indexes {
            let sample = &mut samples[index];
            let difference_ppm = (sample.observed_players.abs_diff(strongest) as u128
                * 1_000_000
                / strongest as u128) as u64;
            if difference_ppm > CROSS_VALIDATION_DISAGREEMENT_THRESHOLD_PPM {
                // Compute in u64: ppm * ppm can exceed u32 before the
                // 1_000_000 division (e.g. 1_000_000 * 500_000).
                let downscaled = (u64::from(sample.source_confidence_ppm)
                    * u64::from(CROSS_VALIDATION_CONFIDENCE_DOWNSCALE_PPM)
                    / 1_000_000) as u32;
                sample.source_confidence_ppm = downscaled.max(100_000);
                adjusted += 1;
            }
        }
    }
    adjusted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn third_party_collector_parses_tier2_style_input() {
        let collector = ThirdPartyJsonCollector::new(ActivitySourceKind::Epic);
        let sample = collector
            .collect(42, 100, r#"{"player_count":1234,"confidence_ppm":450000}"#)
            .unwrap();
        assert_eq!(sample.source_kind, ActivitySourceKind::Epic);
        assert_eq!(sample.source_confidence_ppm, 450_000);
        assert_eq!(sample.observed_players, 1_234);
    }

    #[test]
    fn cross_validate_lowers_outlier_confidence_for_same_app() {
        let mut samples = vec![
            ActivitySample::new(730, 500_000, 1, "{\"player_count\":500000}", ActivitySourceKind::Steam, 1_000_000),
            ActivitySample::new(730, 100_000, 1, "{\"player_count\":100000}", ActivitySourceKind::Epic, 1_000_000),
        ];
        let adjusted = cross_validate_samples(&mut samples);
        assert_eq!(adjusted, 1);
        // The strong source keeps its confidence, the outlier is halved.
        assert_eq!(samples[0].source_confidence_ppm, 1_000_000);
        assert_eq!(samples[1].source_confidence_ppm, 500_000);
    }

    #[test]
    fn cross_validate_keeps_confidence_when_sources_agree() {
        let mut samples = vec![
            ActivitySample::new(730, 500_000, 1, "a", ActivitySourceKind::Steam, 900_000),
            ActivitySample::new(730, 485_000, 1, "b", ActivitySourceKind::Epic, 800_000),
        ];
        let adjusted = cross_validate_samples(&mut samples);
        assert_eq!(adjusted, 0);
        assert_eq!(samples[0].source_confidence_ppm, 900_000);
        assert_eq!(samples[1].source_confidence_ppm, 800_000);
    }

    #[test]
    fn cross_validate_ignores_single_source_apps() {
        let mut samples = vec![ActivitySample::new(
            570,
            300_000,
            1,
            "c",
            ActivitySourceKind::Steam,
            123_456,
        )];
        let adjusted = cross_validate_samples(&mut samples);
        assert_eq!(adjusted, 0);
        assert_eq!(samples[0].source_confidence_ppm, 123_456);
    }

    #[test]
    fn community_collector_parses_tier3_style_input() {
        let collector = CommunityJsonCollector;
        let sample = collector
            .collect(
                99,
                200,
                r#"{"estimated_players":77,"confidence_ppm":120000}"#,
            )
            .unwrap();
        assert_eq!(sample.source_kind, ActivitySourceKind::Community);
        assert_eq!(sample.source_confidence_ppm, 120_000);
        assert_eq!(sample.observed_players, 77);
    }
}
