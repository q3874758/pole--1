use thiserror::Error;

#[derive(Debug, Error)]
pub enum CosmosError {
    #[error("invalid hex address: {0}")]
    InvalidHex(String),

    #[error("invalid bech32 address: {0}")]
    InvalidBech32(String),

    #[error("bech32 prefix mismatch: expected {expected}, got {actual}")]
    Bech32PrefixMismatch { expected: String, actual: String },

    #[error("invalid public key length: expected 32, got {0}")]
    InvalidPubKeyLength(usize),

    #[error("invalid signature length: expected 64, got {0}")]
    InvalidSignatureLength(usize),

    #[error("HTTP request failed: {0}")]
    Http(String),

    #[error("RPC error (code={code}): {message}")]
    Rpc { code: i32, message: String },

    #[error("REST error (status={status}): {body}")]
    Rest { status: u16, body: String },

    #[error("failed to decode response: {0}")]
    Decode(String),

    #[error("failed to encode transaction: {0}")]
    Encode(String),

    #[error("broadcast failed after {attempts} attempts: {last}")]
    BroadcastExhausted { attempts: u32, last: String },

    #[error("missing field: {0}")]
    MissingField(&'static str),

    #[error("not implemented: {0}")]
    Unimplemented(&'static str),
}

impl From<reqwest::Error> for CosmosError {
    fn from(err: reqwest::Error) -> Self {
        CosmosError::Http(err.to_string())
    }
}

impl From<bech32::Error> for CosmosError {
    fn from(err: bech32::Error) -> Self {
        CosmosError::InvalidBech32(err.to_string())
    }
}

impl From<base64::DecodeError> for CosmosError {
    fn from(err: base64::DecodeError) -> Self {
        CosmosError::Decode(format!("base64: {err}"))
    }
}

impl From<hex::FromHexError> for CosmosError {
    fn from(err: hex::FromHexError) -> Self {
        CosmosError::InvalidHex(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, CosmosError>;
