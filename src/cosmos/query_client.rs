use base64::Engine;
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
#[serde(rename_all = "snake_case")]
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

    pub fn with_client(base_url: impl Into<String>, client: Client) -> Self {
        Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
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

    /// Helper: build a URL that the bridge can call for arbitrary module
    /// queries. Returns the path; callers add params + decode the body.
    pub fn module_query_path(&self, module: &str, query: &str) -> String {
        format!("{}/{}/v1/{}", self.base_url, module, query)
    }

    /// Helper: decode a base64 string to raw bytes, used for response
    /// payloads that the SDK returns as base64.
    pub fn decode_b64(s: &str) -> Result<Vec<u8>> {
        Ok(base64::engine::general_purpose::STANDARD.decode(s)?)
    }
}
