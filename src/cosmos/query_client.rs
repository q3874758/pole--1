use std::collections::BTreeMap;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::cosmos::error::{CosmosError, Result};

/// Account info returned by the auth module REST endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub account_number: String,
    pub sequence: String,
}

/// Lightweight view of an account used by the bridge. The full auth
/// response has many more fields; we only project what we need.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthAccountResponse {
    pub account: AuthAccountInner,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "@type")]
pub enum AuthAccountInner {
    #[serde(rename = "/cosmos.auth.v1beta1.BaseAccount")]
    BaseAccount(BaseAccount),
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseAccount {
    pub account_number: String,
    pub sequence: String,
    pub address: String,
}

impl AuthAccountInner {
    pub fn into_info(self) -> Result<AccountInfo> {
        match self {
            AuthAccountInner::BaseAccount(b) => Ok(AccountInfo {
                account_number: b.account_number,
                sequence: b.sequence,
            }),
            AuthAccountInner::Other => Err(CosmosError::Unimplemented(
                "non-base account type; module account support not yet implemented",
            )),
        }
    }
}

/// One `pole.chain.pole.v1.Query/WitnessCredits` row: the bech32 account of
/// a witness and the number of its attestations the chain adopted for the
/// epoch.
#[derive(Debug, Clone, Deserialize)]
pub struct WitnessCreditEntry {
    pub address: String,
    #[serde(deserialize_with = "de_u64_flexible")]
    pub credits: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WitnessCreditsResponse {
    #[serde(default)]
    pub credits: Vec<WitnessCreditEntry>,
}

/// The gogoproto gateway marshals `uint64` as a JSON string, while a plain
/// `serde_json` round-trip (and some gateway versions) produces a number.
/// Accept both so the parser does not depend on the codegen version.
fn de_u64_flexible<'de, D>(deserializer: D) -> std::result::Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Number(u64),
        Text(String),
    }

    match Raw::deserialize(deserializer)? {
        Raw::Number(value) => Ok(value),
        Raw::Text(text) => text.parse::<u64>().map_err(serde::de::Error::custom),
    }
}

/// Same dual encoding as [`de_u64_flexible`], for `uint32` proto fields.
fn de_u32_flexible<'de, D>(deserializer: D) -> std::result::Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Number(u32),
        Text(String),
    }

    match Raw::deserialize(deserializer)? {
        Raw::Number(value) => Ok(value),
        Raw::Text(text) => text.parse::<u32>().map_err(serde::de::Error::custom),
    }
}

/// Same dual encoding as [`de_u64_flexible`], for `int64` proto fields.
fn de_i64_flexible<'de, D>(deserializer: D) -> std::result::Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Number(i64),
        Text(String),
    }

    match Raw::deserialize(deserializer)? {
        Raw::Number(value) => Ok(value),
        Raw::Text(text) => text.parse::<i64>().map_err(serde::de::Error::custom),
    }
}

/// One `pole.chain.pole.v1.Query/SessionSettlement` row — the verdict the
/// chain reached for a play session: how many independent witnesses it
/// adopted, how much heartbeat coverage it proved, and the weight units it
/// credited to the player.
#[derive(Debug, Clone, Deserialize)]
pub struct SessionSettlementView {
    pub session_id_hex: String,
    #[serde(deserialize_with = "de_u64_flexible")]
    pub epoch_id: u64,
    #[serde(deserialize_with = "de_u32_flexible")]
    pub app_id: u32,
    #[serde(default)]
    pub node_address: String,
    pub valid: bool,
    #[serde(deserialize_with = "de_u64_flexible")]
    pub play_seconds: u64,
    #[serde(deserialize_with = "de_u32_flexible")]
    pub game_weight_ppm: u32,
    #[serde(deserialize_with = "de_u64_flexible")]
    pub player_weight_units: u64,
    #[serde(deserialize_with = "de_u64_flexible")]
    pub witness_count: u64,
    #[serde(deserialize_with = "de_u64_flexible")]
    pub distinct_observation_count: u64,
    #[serde(deserialize_with = "de_u64_flexible")]
    pub heartbeat_count: u64,
    #[serde(deserialize_with = "de_u32_flexible")]
    pub heartbeat_coverage_bps: u32,
    #[serde(deserialize_with = "de_i64_flexible")]
    pub settled_at_height: i64,
    #[serde(default)]
    pub invalid_reason: String,
}

/// `QuerySessionSettlementResponse` — the settlement is optional in proto3
/// (a message field), so a miss is a decode-level "not found".
#[derive(Debug, Clone, Deserialize)]
pub struct SessionSettlementResponse {
    pub settlement: SessionSettlementView,
}

/// Parses a `QuerySessionSettlementResponse` body.
pub fn parse_session_settlement(body: &str) -> Result<SessionSettlementView> {
    let parsed: SessionSettlementResponse =
        serde_json::from_str(body).map_err(|err| CosmosError::Decode(err.to_string()))?;
    Ok(parsed.settlement)
}

/// Parses a `QueryWitnessCreditsResponse` body into `account -> credits`.
///
/// The map is the authoritative basis for the witness reward split: the
/// chain rejects any reward submission whose `verify_reward` values are not
/// exactly proportional to these credits, so off-chain reward records must
/// be derived from it rather than from local attestation counts.
pub fn parse_witness_credits(body: &str) -> Result<BTreeMap<String, u64>> {
    let parsed: WitnessCreditsResponse =
        serde_json::from_str(body).map_err(|err| CosmosError::Decode(err.to_string()))?;
    let mut credits = BTreeMap::new();
    for entry in parsed.credits {
        if entry.address.is_empty() {
            continue;
        }
        *credits.entry(entry.address).or_insert(0) += entry.credits;
    }
    Ok(credits)
}

/// Blocking variant of [`RestClient::witness_credits`] for the node's
/// synchronous reward computation, which runs outside any async runtime.
///
/// A short timeout keeps an unreachable chain from stalling the tick; every
/// error means "no chain data available" to the caller.
pub fn witness_credits_blocking(base_url: &str, epoch_id: u64) -> Result<BTreeMap<String, u64>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|err| CosmosError::Http(err.to_string()))?;
    let url = format!(
        "{}/pole.chain.pole.v1.Query/WitnessCredits",
        base_url.trim_end_matches('/')
    );
    let resp = client
        .post(&url)
        .json(&serde_json::json!({ "epoch_id": epoch_id }))
        .send()
        .map_err(|err| CosmosError::Http(err.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(CosmosError::Rest {
            status: status.as_u16(),
            body,
        });
    }
    let body = resp
        .text()
        .map_err(|err| CosmosError::Http(err.to_string()))?;
    parse_witness_credits(&body)
}

/// REST client for the Cosmos application endpoints (auth, bank, the
/// PoLE module). Distinct from `TendermintRpc`, which speaks raw
/// Tendermint.
pub struct RestClient {
    client: Client,
    base_url: String,
}

impl RestClient {
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| CosmosError::Http(e.to_string()))?;
        Ok(Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// POST a JSON body to a gRPC-gateway query route and return the raw
    /// response body. All PoLE query routes are
    /// `POST /pole.chain.pole.v1.Query/<Method>`; `runtime.AssumeColonVerbOpt(true)`
    /// means the literal `pole.chain.pole.v1.Query` segment is used as-is.
    pub async fn post_query(&self, method: &str, body: serde_json::Value) -> Result<String> {
        let url = format!("{}/pole.chain.pole.v1.Query/{}", self.base_url, method);
        let resp = self.client.post(&url).json(&body).send().await?;
        let status = resp.status();
        if !status.is_success() {
            // A 404 here is ambiguous: the gateway returns it both for an
            // unrouted path and for `codes.NotFound` raised by the query
            // server (`grpcError` maps `collections.ErrNotFound`). Keep
            // the body so callers can tell the two apart.
            let body = resp.text().await.unwrap_or_default();
            return Err(CosmosError::Rest {
                status: status.as_u16(),
                body,
            });
        }
        resp.text()
            .await
            .map_err(|err| CosmosError::Http(err.to_string()))
    }

    /// Read the chain's verdict for a play session.
    pub async fn session_settlement(&self, session_id_hex: &str) -> Result<SessionSettlementView> {
        let body = self
            .post_query(
                "SessionSettlement",
                serde_json::json!({ "session_id_hex": session_id_hex }),
            )
            .await?;
        parse_session_settlement(&body)
    }

    /// Read the per-witness attestation credits the chain adopted for an
    /// epoch (witness bech32 account → adopted attestation count).
    pub async fn witness_credits(&self, epoch_id: u64) -> Result<BTreeMap<String, u64>> {
        let body = self
            .post_query(
                "WitnessCredits",
                serde_json::json!({ "epoch_id": epoch_id }),
            )
            .await?;
        parse_witness_credits(&body)
    }

    /// Fetch `account_number` and `sequence` for the given bech32 address.
    pub async fn get_account(&self, address: &str) -> Result<AccountInfo> {
        let url = format!("{}/cosmos/auth/v1beta1/accounts/{}", self.base_url, address);
        let resp = self.client.get(&url).send().await?;
        let status = resp.status();
        if status.as_u16() == 404 {
            return Err(CosmosError::MissingField("account not found on chain"));
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(CosmosError::Rest {
                status: status.as_u16(),
                body,
            });
        }
        let parsed: AuthAccountResponse = resp.json().await?;
        parsed.account.into_info()
    }
}
