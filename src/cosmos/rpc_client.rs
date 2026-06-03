use std::time::Duration;

use base64::Engine;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::cosmos::error::{CosmosError, Result};

/// Options controlling how the RPC client retries broadcasts.
#[derive(Debug, Clone)]
pub struct BroadcastOptions {
    pub max_retries: u32,
    pub retry_delay: Duration,
    pub request_timeout: Duration,
}

impl Default for BroadcastOptions {
    fn default() -> Self {
        Self {
            max_retries: 3,
            retry_delay: Duration::from_secs(2),
            request_timeout: Duration::from_secs(15),
        }
    }
}

/// Result of a `broadcast_tx_sync` call. Mirrors the Tendermint RPC schema
/// but only retains the fields we actually use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadcastTxResponse {
    #[serde(rename = "txhash")]
    pub tx_hash: String,
    #[serde(default)]
    pub code: u32,
    #[serde(default)]
    pub data: String,
    #[serde(default)]
    pub log: String,
    #[serde(default)]
    pub codespace: String,
}

impl BroadcastTxResponse {
    pub fn is_ok(&self) -> bool {
        self.code == 0
    }
}

/// Subset of `/status` response we need to read the latest block height.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub result: StatusResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResult {
    pub sync_info: SyncInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncInfo {
    pub latest_block_height: String,
    #[serde(default)]
    pub latest_block_time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbciQueryResponse {
    pub result: AbciQueryResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbciQueryResult {
    pub response: AbciQueryResponseInner,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbciQueryResponseInner {
    #[serde(default)]
    pub code: u32,
    #[serde(default)]
    pub log: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub height: String,
}

/// Thin wrapper around the Tendermint RPC endpoints we use from the
/// bridge layer. The full surface area is intentionally small — extend
/// as new RPCs are needed.
pub struct TendermintRpc {
    client: Client,
    base_url: String,
}

impl TendermintRpc {
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| CosmosError::Http(e.to_string()))?;
        Ok(Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
        })
    }

    pub fn with_client(base_url: impl Into<String>, client: Client) -> Self {
        Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Broadcast a pre-signed transaction (already protobuf-encoded and
    /// base64-wrapped by the caller) and retry on transient failures.
    pub async fn broadcast_tx_sync(
        &self,
        tx_bytes: &[u8],
        opts: &BroadcastOptions,
    ) -> Result<BroadcastTxResponse> {
        let mut last_err = String::new();
        for attempt in 1..=opts.max_retries.max(1) {
            match self.broadcast_tx_sync_once(tx_bytes, opts.request_timeout).await {
                Ok(resp) if resp.is_ok() => return Ok(resp),
                Ok(resp) => {
                    last_err = format!("code={} log={}", resp.code, resp.log);
                    if !Self::is_retriable_code(resp.code) {
                        return Ok(resp);
                    }
                }
                Err(e) => last_err = e.to_string(),
            }
            if attempt < opts.max_retries {
                tokio::time::sleep(opts.retry_delay).await;
            }
        }
        Err(CosmosError::BroadcastExhausted {
            attempts: opts.max_retries,
            last: last_err,
        })
    }

    async fn broadcast_tx_sync_once(
        &self,
        tx_bytes: &[u8],
        timeout: Duration,
    ) -> Result<BroadcastTxResponse> {
        let tx_b64 = base64::engine::general_purpose::STANDARD.encode(tx_bytes);
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "pole-bridge",
            "method": "broadcast_tx_sync",
            "params": { "tx": tx_b64 }
        });
        let resp = self
            .client
            .post(&self.base_url)
            .timeout(timeout)
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        let raw: serde_json::Value = resp.json().await?;
        if !status.is_success() {
            return Err(CosmosError::Rest {
                status: status.as_u16(),
                body: raw.to_string(),
            });
        }
        if let Some(err) = raw.get("error") {
            return Err(CosmosError::Rpc {
                code: err.get("code").and_then(|c| c.as_i64()).unwrap_or(-1) as i32,
                message: err.to_string(),
            });
        }
        let parsed: BroadcastTxResponseWrapper = serde_json::from_value(raw)
            .map_err(|e| CosmosError::Decode(e.to_string()))?;
        Ok(parsed.result)
    }

    /// Query the latest committed block height.
    pub async fn latest_height(&self) -> Result<u64> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "pole-bridge",
            "method": "status",
            "params": {}
        });
        let raw: serde_json::Value = self
            .client
            .post(&self.base_url)
            .json(&body)
            .send()
            .await?
            .json()
            .await?;
        let parsed: StatusResponse = serde_json::from_value(raw)
            .map_err(|e| CosmosError::Decode(e.to_string()))?;
        parsed
            .result
            .sync_info
            .latest_block_height
            .parse()
            .map_err(|e: std::num::ParseIntError| CosmosError::Decode(e.to_string()))
    }

    /// Generic `abci_query` against the application store. The `data` and
    /// `path` arguments are application-specific; the response value is
    /// base64-encoded bytes that the caller decodes.
    pub async fn abci_query(
        &self,
        path: &str,
        data: &[u8],
        height: Option<u64>,
    ) -> Result<AbciQueryResponseInner> {
        let data_b64 = base64::engine::general_purpose::STANDARD.encode(data);
        let mut params = serde_json::Map::new();
        params.insert("path".into(), serde_json::Value::String(path.into()));
        params.insert("data".into(), serde_json::Value::String(data_b64));
        if let Some(h) = height {
            params.insert("height".into(), serde_json::Value::String(h.to_string()));
        }
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "pole-bridge",
            "method": "abci_query",
            "params": serde_json::Value::Object(params),
        });
        let raw: serde_json::Value = self
            .client
            .post(&self.base_url)
            .json(&body)
            .send()
            .await?
            .json()
            .await?;
        let parsed: AbciQueryResponse = serde_json::from_value(raw)
            .map_err(|e| CosmosError::Decode(e.to_string()))?;
        Ok(parsed.result.response)
    }

    /// Codes that the Cosmos SDK reserves for transient conditions worth
    /// retrying. Anything outside this list is returned to the caller.
    fn is_retriable_code(code: u32) -> bool {
        // 11: sequence mismatch (mempool will catch up)
        // 19: tx already in mempool
        // 20: mempool full
        // 25: no validators (rare, transient)
        matches!(code, 11 | 19 | 20 | 25)
    }
}

#[derive(Debug, Deserialize)]
struct BroadcastTxResponseWrapper {
    result: BroadcastTxResponse,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broadcast_options_default_is_sane() {
        let opts = BroadcastOptions::default();
        assert!(opts.max_retries >= 1);
        assert!(opts.retry_delay.as_secs() > 0);
    }

    #[test]
    fn retriable_codes_match_sdk() {
        assert!(TendermintRpc::is_retriable_code(11));
        assert!(TendermintRpc::is_retriable_code(19));
        assert!(!TendermintRpc::is_retriable_code(5));
    }
}
