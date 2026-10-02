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

/// Canonical sign bytes for SIGN_MODE_DIRECT.
///
/// The SDK definition (`x/tx/signing/direct/direct.go`):
///   bytes_to_sign = proto.Marshal(SignDoc{body_bytes, auth_info_bytes,
///                                         chain_id, account_number})
///
/// The four context fields are *not* concatenated and hashed: they are
/// serialized as a real protobuf message (tags 1..4). The raw marshaled
/// bytes are what the signer signs — Ed25519 then applies SHA-512
/// internally, so there is no extra pre-hash on this side.
///
/// `SignDoc` carries no maps, so prost's field-ordered encoding is
/// already the deterministic encoding the SDK's `proto.MarshalOptions{
/// Deterministic: true}` produces.
pub fn sign_doc_bytes(
    body_bytes: &[u8],
    auth_info_bytes: &[u8],
    chain_id: &str,
    account_number: u64,
) -> Result<Vec<u8>, String> {
    encode(&SignDoc {
        body_bytes: body_bytes.to_vec(),
        auth_info_bytes: auth_info_bytes.to_vec(),
        chain_id: chain_id.to_string(),
        account_number,
    })
}
