use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;

use crate::node_pipeline::SteamCurrentPlayersSample;
use crate::primitives::{ActivitySourceKind, AppId, UnixMillis};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SteamCollectorError {
    Http(String),
    InvalidJson(String),
    MissingResult,
    MissingPlayerCount,
    Clock(String),
}

impl fmt::Display for SteamCollectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(err) => write!(f, "steam http error: {err}"),
            Self::InvalidJson(err) => write!(f, "invalid steam response json: {err}"),
            Self::MissingResult => write!(f, "steam response indicates failure"),
            Self::MissingPlayerCount => write!(f, "steam response missing player_count"),
            Self::Clock(err) => write!(f, "clock error: {err}"),
        }
    }
}

impl std::error::Error for SteamCollectorError {}

pub trait HttpTextClient {
    fn get_text(&self, url: &str) -> Result<String, SteamCollectorError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ReqwestHttpTextClient;

/// Shared blocking HTTP client with a request timeout and a user agent.
/// The timeout prevents a stalled upstream endpoint from hanging a collect
/// tick forever; the user agent keeps servers from rejecting the request.
fn http_client() -> &'static reqwest::blocking::Client {
    use std::sync::OnceLock;
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("pole-node/0.1 (PoLE activity collector)")
            .build()
            .expect("build reqwest blocking client")
    })
}

impl HttpTextClient for ReqwestHttpTextClient {
    fn get_text(&self, url: &str) -> Result<String, SteamCollectorError> {
        let response = http_client()
            .get(url)
            .send()
            .map_err(|err| SteamCollectorError::Http(err.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(SteamCollectorError::Http(format!(
                "unexpected status {status}"
            )));
        }
        response
            .text()
            .map_err(|err| SteamCollectorError::Http(err.to_string()))
    }
}

#[derive(Debug, Deserialize)]
struct SteamCurrentPlayersEnvelope {
    response: SteamCurrentPlayersBody,
}

#[derive(Debug, Deserialize)]
struct SteamCurrentPlayersBody {
    result: Option<u32>,
    player_count: Option<u64>,
}

pub fn current_players_url(app_id: AppId) -> String {
    format!(
        "https://api.steampowered.com/ISteamUserStats/GetNumberOfCurrentPlayers/v1/?appid={app_id}"
    )
}

pub fn parse_current_players_response(
    app_id: AppId,
    observed_at_millis: UnixMillis,
    raw_body: &str,
) -> Result<SteamCurrentPlayersSample, SteamCollectorError> {
    let envelope: SteamCurrentPlayersEnvelope = serde_json::from_str(raw_body)
        .map_err(|err| SteamCollectorError::InvalidJson(err.to_string()))?;
    let response = envelope.response;

    if response.result == Some(0) {
        return Err(SteamCollectorError::MissingResult);
    }

    let observed_players = response
        .player_count
        .ok_or(SteamCollectorError::MissingPlayerCount)?;

    Ok(SteamCurrentPlayersSample::new(
        app_id,
        observed_players,
        observed_at_millis,
        raw_body,
        ActivitySourceKind::Steam,
        1_000_000,
    ))
}

pub fn fetch_current_players_with_client(
    client: &dyn HttpTextClient,
    app_id: AppId,
    observed_at_millis: UnixMillis,
) -> Result<SteamCurrentPlayersSample, SteamCollectorError> {
    let url = current_players_url(app_id);
    let raw_body = client.get_text(&url)?;
    parse_current_players_response(app_id, observed_at_millis, &raw_body)
}

pub fn fetch_current_players_live(
    client: &dyn HttpTextClient,
    app_id: AppId,
) -> Result<SteamCurrentPlayersSample, SteamCollectorError> {
    let observed_at_millis = current_unix_millis()?;
    fetch_current_players_with_client(client, app_id, observed_at_millis)
}

pub fn current_unix_millis() -> Result<UnixMillis, SteamCollectorError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| SteamCollectorError::Clock(err.to_string()))?;
    Ok(duration.as_millis() as UnixMillis)
}
