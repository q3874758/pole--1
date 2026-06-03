//! Re-exports of `cosmos-sdk-proto` types used by the bridge layer.
//!
//! We depend on the upstream crate (which ships pre-generated Rust
//! sources) instead of compiling `.proto` files at build time. This
//! keeps the build hermetic — no `protoc` needed.
//!
//! Namespace: [`cosmos_sdk_proto::cosmos::tx::v1beta1`] hosts the
//! `Tx`, `TxBody`, `AuthInfo`, `SignDoc`, `TxRaw`, `SignerInfo`,
//! `ModeInfo`, `Fee` types we sign and broadcast.

pub use cosmos_sdk_proto::cosmos::base::v1beta1::Coin;
pub use cosmos_sdk_proto::cosmos::tx::signing::v1beta1::SignMode;
pub use cosmos_sdk_proto::cosmos::tx::v1beta1::{
    mode_info, AuthInfo, Fee, ModeInfo, SignDoc, SignerInfo, Tx, TxBody, TxRaw,
};
pub use cosmos_sdk_proto::prost::Message as MessageEncode;
pub use cosmos_sdk_proto::prost::Message;
pub use cosmos_sdk_proto::Any;

/// Encode any `Message` to protobuf bytes.
pub fn encode<M: MessageEncode>(msg: &M) -> Result<Vec<u8>, String> {
    let mut buf = Vec::with_capacity(msg.encoded_len());
    msg.encode(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

/// Canonical SignDoc hash for SIGN_MODE_DIRECT.
///
/// The SDK definition (v0.50+, used by both Cosmos SDK 0.47+ and the
/// `SignModeDirect` code path):
///   bytes_to_sign = sha256( body_bytes || auth_info_bytes || chain_id || account_number_be )
///
/// This is the exact byte sequence that gets signed by the signer.
pub fn sign_doc_hash(body_bytes: &[u8], auth_info_bytes: &[u8], chain_id: &str, account_number: u64) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(body_bytes);
    hasher.update(auth_info_bytes);
    hasher.update(chain_id.as_bytes());
    hasher.update(&account_number.to_be_bytes());
    hasher.finalize().into()
}
