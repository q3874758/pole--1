use serde::{Deserialize, Serialize};

use crate::cosmos::error::{CosmosError, Result};
use crate::wallet::KeyPair;

/// Canonical [`cosmos_sdk_proto::cosmos::tx::v1beta1::SignDoc`] payload
/// for the bridge. The fields here are the four inputs to the SDK's
/// `SignDoc` proto; we keep a typed Rust view so callers don't have to
/// import the upstream type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignDocInputs {
    /// Pre-encoded `TxBody` protobuf bytes
    pub body_bytes: Vec<u8>,
    /// Pre-encoded `AuthInfo` protobuf bytes
    pub auth_info_bytes: Vec<u8>,
    pub chain_id: String,
    pub account_number: u64,
}

impl SignDocInputs {
    /// Produce the canonical `SignDoc` protobuf bytes that get signed
    /// under `SIGN_MODE_DIRECT`. The SDK definition is:
    ///     proto.Marshal(SignDoc{body_bytes, auth_info_bytes, chain_id, account_number})
    ///
    /// Implemented by [`crate::cosmos::proto::sign_doc_bytes`]; the
    /// indirection through a method is just so callers don't have to
    /// pass the four pieces of context separately.
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        crate::cosmos::proto::sign_doc_bytes(
            &self.body_bytes,
            &self.auth_info_bytes,
            &self.chain_id,
            self.account_number,
        )
        .map_err(|e| CosmosError::Encode(format!("SignDoc: {e}")))
    }
}

/// Pair of (signature) attached to a TxRaw. Stored alongside the
/// signed body+auth_info so the caller can rebuild `TxRaw` for
/// broadcast.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedTx {
    pub body_bytes: Vec<u8>,
    pub auth_info_bytes: Vec<u8>,
    pub signatures: Vec<Vec<u8>>,
}

impl SignedTx {
    /// Encode the signed transaction as a real proto `TxRaw` and
    /// base64-wrap it for the Tendermint `broadcast_tx_sync` endpoint.
    pub fn to_base64(&self) -> Result<String> {
        let raw = crate::cosmos::proto::TxRaw {
            body_bytes: self.body_bytes.clone(),
            auth_info_bytes: self.auth_info_bytes.clone(),
            signatures: self.signatures.clone(),
        };
        let bytes = crate::cosmos::proto::encode(&raw)
            .map_err(|e| CosmosError::Encode(format!("TxRaw: {e}")))?;
        Ok(base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            bytes,
        ))
    }
}

/// Sign the canonical SignDoc inputs with the given keypair. The
/// signature is 64 bytes (Ed25519) and is what the SDK verifies under
/// SignModeDirect.
pub fn sign_sign_doc(keypair: &KeyPair, doc: &SignDocInputs) -> Result<Vec<u8>> {
    let bytes = doc.signing_bytes()?;
    let sig = keypair.sign(&bytes);
    if sig.len() != 64 {
        return Err(CosmosError::InvalidSignatureLength(sig.len()));
    }
    Ok(sig)
}

/// Convenience: produce a broadcast-ready `SignedTx` from a keypair
/// plus pre-encoded `body_bytes` and `auth_info_bytes`.
pub fn sign_with_keypair(
    keypair: &KeyPair,
    body_bytes: Vec<u8>,
    auth_info_bytes: Vec<u8>,
    chain_id: &str,
    account_number: u64,
) -> Result<SignedTx> {
    let doc = SignDocInputs {
        body_bytes,
        auth_info_bytes,
        chain_id: chain_id.to_string(),
        account_number,
    };
    let signature = sign_sign_doc(keypair, &doc)?;
    Ok(SignedTx {
        body_bytes: doc.body_bytes,
        auth_info_bytes: doc.auth_info_bytes,
        signatures: vec![signature],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_doc_signing_bytes_is_deterministic() {
        let doc = SignDocInputs {
            body_bytes: vec![1, 2, 3],
            auth_info_bytes: vec![4, 5, 6],
            chain_id: "pole-test".into(),
            account_number: 42,
        };
        let a = doc.signing_bytes().unwrap();
        let b = doc.signing_bytes().unwrap();
        assert_eq!(a, b);
    }

    /// The sign bytes must be a real protobuf `SignDoc` message — the
    /// SDK's `SignModeDirect` handler marshals exactly this struct and
    /// Ed25519 hashes it internally. Asserting the literal wire bytes
    /// pins the encoding against a regression to a hand-rolled
    /// concatenation + sha256 (which the chain rejects as
    /// `ErrUnauthorized`).
    #[test]
    fn sign_doc_signing_bytes_are_a_proto_sign_doc() {
        use crate::cosmos::proto::Message;
        let doc = SignDocInputs {
            body_bytes: vec![1, 2, 3],
            auth_info_bytes: vec![4, 5, 6],
            chain_id: "pole-test".into(),
            account_number: 42,
        };
        let bytes = doc.signing_bytes().unwrap();

        // tag 1 (body_bytes, LEN), tag 2 (auth_info_bytes, LEN),
        // tag 3 (chain_id, LEN), tag 4 (account_number, VARINT) — in
        // ascending field order.
        let mut expected = Vec::new();
        expected.extend_from_slice(&[0x0A, 0x03, 1, 2, 3]);
        expected.extend_from_slice(&[0x12, 0x03, 4, 5, 6]);
        expected.extend_from_slice(&[0x1A, 0x09]);
        expected.extend_from_slice(b"pole-test");
        expected.extend_from_slice(&[0x20, 0x2A]);
        assert_eq!(bytes, expected);

        // Round-trip through the upstream proto type.
        let parsed = crate::cosmos::proto::SignDoc::decode(bytes.as_slice()).unwrap();
        assert_eq!(parsed.body_bytes, vec![1, 2, 3]);
        assert_eq!(parsed.auth_info_bytes, vec![4, 5, 6]);
        assert_eq!(parsed.chain_id, "pole-test");
        assert_eq!(parsed.account_number, 42);
    }

    #[test]
    fn sign_with_keypair_produces_64_byte_signature() {
        let kp = KeyPair::from_seed(&[7u8; 32]);
        let signed = sign_with_keypair(&kp, vec![1], vec![2], "pole-test", 1).unwrap();
        assert_eq!(signed.signatures.len(), 1);
        assert_eq!(signed.signatures[0].len(), 64);
    }

    #[test]
    fn signed_tx_to_base64_decodes_to_tx_raw() {
        use crate::cosmos::proto::Message;
        let kp = KeyPair::from_seed(&[9u8; 32]);
        let signed = sign_with_keypair(&kp, vec![1, 2, 3], vec![4, 5, 6], "pole-test", 7).unwrap();
        let b64 = signed.to_base64().unwrap();
        let raw_bytes =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &b64).unwrap();
        // The decoded bytes should be a valid TxRaw — re-parse it.
        let parsed = crate::cosmos::proto::TxRaw::decode(raw_bytes.as_slice()).unwrap();
        assert_eq!(parsed.body_bytes, vec![1, 2, 3]);
        assert_eq!(parsed.auth_info_bytes, vec![4, 5, 6]);
        assert_eq!(parsed.signatures.len(), 1);
        assert_eq!(parsed.signatures[0].len(), 64);
    }
}
