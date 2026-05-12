use thiserror::Error;

#[derive(Error, Debug)]
pub enum WalletError {
    #[error("invalid mnemonic word: {0}")]
    InvalidWord(String),
    #[error("mnemonic checksum mismatch: expected {expected}, got {got}")]
    ChecksumMismatch { expected: u16, got: u16 },
    #[error("mnemonic must be 24 words, got {0}")]
    InvalidWordCount(usize),
    #[error("invalid hex string: {0}")]
    InvalidHex(String),
    #[error("keystore file not found: {0}")]
    KeystoreNotFound(String),
    #[error("invalid keystore format: {0}")]
    InvalidKeystore(String),
    #[error("decryption failed: wrong password?")]
    DecryptionFailed,
    #[error("encryption failed: {0}")]
    EncryptionFailed(String),
    #[error("KDF error: {0}")]
    KdfError(String),
    #[error("ed25519 signing error: {0}")]
    SigningError(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("crypto error: {0}")]
    Crypto(String),
}

pub type Result<T> = std::result::Result<T, WalletError>;