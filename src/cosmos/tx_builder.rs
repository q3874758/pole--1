use serde::{Deserialize, Serialize};

use crate::cosmos::address::CosmosAddress;
use crate::cosmos::error::{CosmosError, Result};
use crate::cosmos::tx_signer::{sign_with_keypair, SignedTx};
use crate::primitives::EpochId;
use crate::wallet::KeyPair;

pub use crate::cosmos::proto::Any;
use crate::cosmos::proto::{mode_info, AuthInfo, Coin, Fee, ModeInfo, SignDoc, SignerInfo, TxBody};

/// Cosmos gas configuration. Real values come from `fee_params` in
/// `genesis.json`; the defaults here are conservative for a local node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeConfig {
    pub denom: String,
    pub gas_limit: u64,
    pub gas_price: Amount,
}

impl Default for FeeConfig {
    fn default() -> Self {
        Self {
            denom: "upole".into(),
            gas_limit: 200_000,
            gas_price: 1_000,
        }
    }
}

impl FeeConfig {
    pub fn estimated_fee(&self) -> Amount {
        (self.gas_limit as Amount) * self.gas_price
    }

    fn to_proto(&self) -> Fee {
        Fee {
            amount: vec![Coin {
                denom: self.denom.clone(),
                amount: self.estimated_fee().to_string(),
            }],
            gas_limit: self.gas_limit,
            payer: String::new(),
            granter: String::new(),
        }
    }
}

pub type Amount = u128;

/// Top-level bridge message enum. Each variant corresponds to one
/// `MsgServer` entry in `chain/x/pole/types/tx.pb.go`. The `to_any`
/// method emits a real protobuf `Any` that the chain's
/// `MsgServer.Impls` can decode.
#[derive(Debug, Clone)]
pub enum BridgeMessage {
    FinalizeEpoch {
        finalizer: CosmosAddress,
        epoch_id: EpochId,
    },
    ClaimReward {
        claimer: CosmosAddress,
        epoch_id: EpochId,
        recipient: CosmosAddress,
    },
    OpenChallenge {
        challenger: CosmosAddress,
        epoch_id: EpochId,
    },
    /// Catch-all for messages we haven't hand-rolled yet. The chain
    /// will reject the broadcast, but the type keeps the API stable
    /// for callers that want to compile against the full surface.
    Unsupported { type_url: String, note: String },
}

impl BridgeMessage {
    /// Render the message as a real protobuf `Any` with the proper
    /// `type_url` and proto-encoded `value` bytes.
    pub fn to_any(&self) -> Any {
        match self {
            BridgeMessage::FinalizeEpoch {
                finalizer,
                epoch_id,
            } => crate::cosmos::pole_msgs::encode_msg_finalize_epoch(&finalizer.bech32, *epoch_id),
            BridgeMessage::ClaimReward {
                claimer,
                epoch_id,
                recipient,
            } => crate::cosmos::pole_msgs::encode_msg_claim_reward(
                &claimer.bech32,
                *epoch_id,
                &recipient.bech32,
            ),
            BridgeMessage::OpenChallenge { .. } => {
                // Proto field 2 is a nested `Challenge` message, not a
                // `uint64` — `encode_msg_open_challenge` would write
                // bytes the chain would misinterpret. Emit a typed
                // `Any` with the correct type_url and an empty value
                // so the chain rejects the tx deterministically. The
                // full payload will land when the `Challenge` type is
                // wired in a later phase.
                Any {
                    type_url: "/pole.chain.pole.v1.MsgOpenChallenge".to_string(),
                    value: vec![],
                }
            }
            BridgeMessage::Unsupported { type_url, note } => Any {
                type_url: type_url.clone(),
                value: note.as_bytes().to_vec(),
            },
        }
    }
}

/// Builder that produces a `SignedTx` from a single bridge message.
pub struct TxBuilder<'a> {
    pub chain_id: &'a str,
    pub account_number: u64,
    pub sequence: u64,
    pub fee: FeeConfig,
    pub memo: &'a str,
    pub timeout_height: u64,
}

impl<'a> TxBuilder<'a> {
    pub fn new(chain_id: &'a str) -> Self {
        Self {
            chain_id,
            account_number: 0,
            sequence: 0,
            fee: FeeConfig::default(),
            memo: "",
            timeout_height: 0,
        }
    }

    pub fn with_sequence(mut self, account_number: u64, sequence: u64) -> Self {
        self.account_number = account_number;
        self.sequence = sequence;
        self
    }

    pub fn with_fee(mut self, fee: FeeConfig) -> Self {
        self.fee = fee;
        self
    }

    pub fn with_memo(mut self, memo: &'a str) -> Self {
        self.memo = memo;
        self
    }

    /// Build the real `TxBody` proto.
    pub fn build_body(&self, msg: &BridgeMessage) -> Result<TxBody> {
        Ok(TxBody {
            messages: vec![msg.to_any()],
            memo: self.memo.to_string(),
            timeout_height: self.timeout_height,
            extension_options: Vec::new(),
            non_critical_extension_options: Vec::new(),
        })
    }

    /// Build the real `AuthInfo` proto with a single Ed25519 signer.
    pub fn build_auth_info(&self, signer_pubkey: &[u8; 32]) -> Result<AuthInfo> {
        let pubkey_any = Any {
            type_url: "/cosmos.crypto.ed25519.PubKey".to_string(),
            value: pubkey_pubkey_to_proto_bytes(signer_pubkey),
        };
        Ok(AuthInfo {
            signer_infos: vec![SignerInfo {
                public_key: Some(pubkey_any),
                mode_info: Some(ModeInfo {
                    sum: Some(mode_info::Sum::Single(mode_info::Single { mode: 1 })),
                }),
                sequence: self.sequence,
            }],
            fee: Some(self.fee.to_proto()),
            tip: None,
        })
    }

    /// Sign a message and return the broadcast-ready `SignedTx`.
    ///
    /// `signer` is verified against the public key: the bech32
    /// encoding of `keypair.public`'s first 20 bytes (using the
    /// chain's bech32 prefix) must match `signer.bech32`. This is a
    /// cheap pre-broadcast check that catches key/address mismatches
    /// before the chain rejects the tx with code 5 (invalid signer).
    pub fn build(
        &self,
        msg: &BridgeMessage,
        signer: &CosmosAddress,
        keypair: &KeyPair,
    ) -> Result<SignedTx> {
        // Derive the bech32 the chain expects from the public key and
        // compare against the caller-supplied signer. The 32-byte
        // `keypair.public` is reduced to a 20-byte account id (the
        // first 20 bytes — matches `address::address_to_bech32`).
        let expected = crate::cosmos::address::encode_bech32(
            signer.prefix(),
            &keypair.public[..crate::cosmos::address::ACCOUNT_ADDRESS_LEN],
        )?;
        if expected != signer.bech32 {
            return Err(CosmosError::Encode(format!(
                "signer mismatch: keypair derives '{expected}', caller passed '{}'",
                signer.bech32
            )));
        }

        let body = self.build_body(msg)?;
        let auth_info = self.build_auth_info(&keypair.public)?;

        let body_bytes = crate::cosmos::proto::encode(&body)
            .map_err(|e| CosmosError::Encode(format!("TxBody: {e}")))?;
        let auth_info_bytes = crate::cosmos::proto::encode(&auth_info)
            .map_err(|e| CosmosError::Encode(format!("AuthInfo: {e}")))?;

        sign_with_keypair(
            keypair,
            body_bytes,
            auth_info_bytes,
            self.chain_id,
            self.account_number,
        )
    }

    /// Build a `SignDoc` for the message. Exposed for tests that want
    /// to assert on the signing bytes without going through signing.
    pub fn build_sign_doc(&self, msg: &BridgeMessage, signer_pubkey: &[u8; 32]) -> Result<SignDoc> {
        let body = self.build_body(msg)?;
        let auth_info = self.build_auth_info(signer_pubkey)?;
        Ok(SignDoc {
            body_bytes: crate::cosmos::proto::encode(&body)
                .map_err(|e| CosmosError::Encode(format!("TxBody: {e}")))?,
            auth_info_bytes: crate::cosmos::proto::encode(&auth_info)
                .map_err(|e| CosmosError::Encode(format!("AuthInfo: {e}")))?,
            chain_id: self.chain_id.to_string(),
            account_number: self.account_number,
        })
    }
}

/// Encode the Ed25519 public key into the proto3 form expected by
/// `cosmos.crypto.ed25519.PubKey.value`.
///
/// The schema is: a single length-delimited field containing a 32-byte
/// raw public key.
fn pubkey_pubkey_to_proto_bytes(pubkey: &[u8; 32]) -> Vec<u8> {
    // Wire format:
    //   tag  = (1 << 3) | 2 = 0x0A   (field 1, length-delimited)
    //   len  = 32
    //   data = 32 raw bytes
    let mut buf = Vec::with_capacity(2 + 32);
    buf.push(0x0A);
    buf.push(32);
    buf.extend_from_slice(pubkey);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cosmos::address::DEFAULT_BECH32_PREFIX;
    use crate::cosmos::proto::{Message, TxRaw};

    fn test_address(byte: u8) -> CosmosAddress {
        let mut account = vec![0u8; 20];
        account[19] = byte;
        let bech = crate::cosmos::address::encode_bech32("cosmos", &account).unwrap();
        CosmosAddress {
            account,
            bech32: bech,
        }
    }

    #[test]
    fn finalize_epoch_emits_correct_any() {
        let msg = BridgeMessage::FinalizeEpoch {
            finalizer: test_address(1),
            epoch_id: 7,
        };
        let any = msg.to_any();
        assert_eq!(any.type_url, "/pole.chain.pole.v1.MsgFinalizeEpoch");
        assert!(!any.value.is_empty());
        // Round-trip through TxBody to confirm the Any is well-formed.
        let body = TxBody {
            messages: vec![any],
            memo: "".into(),
            timeout_height: 0,
            extension_options: Vec::new(),
            non_critical_extension_options: Vec::new(),
        };
        let bytes = crate::cosmos::proto::encode(&body).unwrap();
        let back = TxBody::decode(bytes.as_slice()).unwrap();
        assert_eq!(back.messages.len(), 1);
        assert_eq!(
            back.messages[0].type_url,
            "/pole.chain.pole.v1.MsgFinalizeEpoch"
        );
    }

    #[test]
    fn build_produces_proto_encoded_signed_tx() {
        use crate::cosmos::proto::Message;
        let kp = KeyPair::from_seed(&[3u8; 32]);
        // Phase 0.2: derive the signer from the keypair's actual public
        // key so the bech32-pubkey check in `build()` passes. The
        // older `test_address(0xAB)` fixture was a latent bug masked
        // by `_signer` being unused.
        let bech = crate::cosmos::address::encode_bech32(
            DEFAULT_BECH32_PREFIX,
            &kp.public[..crate::cosmos::address::ACCOUNT_ADDRESS_LEN],
        )
        .unwrap();
        let addr = CosmosAddress {
            account: kp.public[..20].to_vec(),
            bech32: bech,
        };
        let builder = TxBuilder::new("pole-test").with_sequence(1, 0);
        let msg = BridgeMessage::ClaimReward {
            claimer: addr.clone(),
            epoch_id: 5,
            recipient: addr.clone(),
        };
        let signed = builder.build(&msg, &addr, &kp).unwrap();
        assert_eq!(signed.signatures.len(), 1);
        assert_eq!(signed.signatures[0].len(), 64);

        // Confirm the signed bytes decode to a real TxRaw.
        let b64 = signed.to_base64().unwrap();
        let raw_bytes =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &b64).unwrap();
        let parsed = TxRaw::decode(raw_bytes.as_slice()).unwrap();
        // body_bytes contains the Any value (the proto-encoded MsgClaimReward)
        // plus the Any wrapper. We just check it parses cleanly and is non-empty.
        assert!(!parsed.body_bytes.is_empty());
        assert!(!parsed.auth_info_bytes.is_empty());
    }

    #[test]
    fn fee_estimation_uses_gas_limit_times_price() {
        let f = FeeConfig {
            gas_limit: 100,
            gas_price: 7,
            ..FeeConfig::default()
        };
        assert_eq!(f.estimated_fee(), 700);
    }

    #[test]
    fn pubkey_proto_bytes_have_correct_wire_format() {
        let pubkey = [0xAAu8; 32];
        let bytes = pubkey_pubkey_to_proto_bytes(&pubkey);
        assert_eq!(bytes[0], 0x0A); // field 1, length-delimited
        assert_eq!(bytes[1], 32);
        assert_eq!(&bytes[2..], &pubkey);
    }

    /// Phase 0.2: the `build()` entry point now verifies that the
    /// bech32 in `signer` matches the address derived from
    /// `keypair.public`. A mismatch must fail closed with a clear
    /// error before any signing or broadcasting.
    #[test]
    fn build_rejects_signer_pubkey_mismatch() {
        let kp = KeyPair::from_seed(&[5u8; 32]);
        let builder = TxBuilder::new("pole-test").with_sequence(1, 0);
        let msg = BridgeMessage::FinalizeEpoch {
            finalizer: test_address(0x11), // deliberately different from kp.public[..20]
            epoch_id: 1,
        };
        // 0x11 test address has a different account bytes than the
        // keypair-derived one — must produce an Encode error.
        let err = builder.build(&msg, &test_address(0x11), &kp).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("signer mismatch"),
            "expected signer-mismatch error, got: {msg}"
        );
    }

    /// Phase 0.2: when the caller-supplied signer matches the public
    /// key's bech32 derivation, `build()` succeeds.
    #[test]
    fn build_accepts_signer_matching_pubkey() {
        let kp = KeyPair::from_seed(&[6u8; 32]);
        // Derive the canonical bech32 from the public key (first 20 bytes).
        let bech = crate::cosmos::address::encode_bech32(
            DEFAULT_BECH32_PREFIX,
            &kp.public[..crate::cosmos::address::ACCOUNT_ADDRESS_LEN],
        )
        .unwrap();
        let signer = CosmosAddress {
            account: kp.public[..20].to_vec(),
            bech32: bech,
        };
        let builder = TxBuilder::new("pole-test").with_sequence(1, 0);
        let msg = BridgeMessage::FinalizeEpoch {
            finalizer: signer.clone(),
            epoch_id: 1,
        };
        let signed = builder.build(&msg, &signer, &kp).unwrap();
        assert_eq!(signed.signatures.len(), 1);
    }

    /// Phase 0.2: `OpenChallenge` produces a typed `Any` with empty
    /// value (the chain will reject it deterministically). The
    /// type_url must still be the real pole-chain path so the chain
    /// can route and emit a clear "skeleton" error rather than
    /// silently misparsing bytes.
    #[test]
    fn open_challenge_emits_stub_any_with_empty_value() {
        let msg = BridgeMessage::OpenChallenge {
            challenger: test_address(0x77),
            epoch_id: 99,
        };
        let any = msg.to_any();
        assert_eq!(any.type_url, "/pole.chain.pole.v1.MsgOpenChallenge");
        assert!(
            any.value.is_empty(),
            "OpenChallenge must emit empty value (chain rejects deterministically): got {} bytes",
            any.value.len()
        );
    }

    /// Phase 0.2: `BridgeMessage::Unsupported` round-trips through
    /// `to_any` as a fallback path. The harness uses this for
    /// messages that have not been hand-rolled yet.
    #[test]
    fn unsupported_arm_passes_through_type_url_and_value() {
        let msg = BridgeMessage::Unsupported {
            type_url: "/pole.node.v1.MsgUpsertNode".into(),
            note: "{\"operator_address\":\"x\"}".into(),
        };
        let any = msg.to_any();
        assert_eq!(any.type_url, "/pole.node.v1.MsgUpsertNode");
        assert_eq!(
            std::str::from_utf8(&any.value).unwrap(),
            "{\"operator_address\":\"x\"}"
        );
    }
}
