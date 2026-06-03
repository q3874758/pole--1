//! Hand-rolled protobuf encoders for the PoLE `Msg*` types.
//!
//! The `pole.chain.pole.v1` package types live in
//! `chain/proto/pole/chain/pole/v1/tx.proto`. They aren't shipped in
//! `cosmos-sdk-proto`, so we encode them directly into the byte stream
//! the chain expects inside `google.protobuf.Any.value`.
//!
//! The encoding follows the standard proto3 wire format:
//!   - field tag = (field_number << 3) | wire_type
//!   - wire_type 0 = varint, 2 = length-delimited
//!
//! We provide encoders for the messages used by the bridge skeleton's
//! happy path. Adding a new message is a matter of writing one more
//! `encode_msg_xxx` function that lays out its fields.

use crate::cosmos::proto::Any;

/// `MsgFinalizeEpoch` — the simplest message in the suite.
///   pole.chain.pole.v1.MsgFinalizeEpoch {
///     string finalizer = 1;
///     uint64 epoch_id   = 2;
///   }
pub fn encode_msg_finalize_epoch(finalizer_bech32: &str, epoch_id: u64) -> Any {
    let mut buf = Vec::with_capacity(finalizer_bech32.len() + 16);
    encode_string(1, finalizer_bech32, &mut buf);
    encode_uint64(2, epoch_id, &mut buf);
    Any {
        type_url: "/pole.chain.pole.v1.MsgFinalizeEpoch".to_string(),
        value: buf,
    }
}

/// `MsgClaimReward` — the second-simplest.
pub fn encode_msg_claim_reward(claimer_bech32: &str, epoch_id: u64, recipient_bech32: &str) -> Any {
    let mut buf = Vec::with_capacity(claimer_bech32.len() + recipient_bech32.len() + 24);
    encode_string(1, claimer_bech32, &mut buf);
    encode_uint64(2, epoch_id, &mut buf);
    encode_string(3, recipient_bech32, &mut buf);
    Any {
        type_url: "/pole.chain.pole.v1.MsgClaimReward".to_string(),
        value: buf,
    }
}

/// `MsgOpenChallenge` — referenced by the integration harness for the
/// challenge path. Kept minimal: only `challenger` + `epoch_id` for
/// now; the real proto also includes a `Challenge` payload that we'd
/// add by hand once the rest of the suite is in.
pub fn encode_msg_open_challenge(challenger_bech32: &str, epoch_id: u64) -> Any {
    let mut buf = Vec::with_capacity(challenger_bech32.len() + 16);
    encode_string(1, challenger_bech32, &mut buf);
    encode_uint64(2, epoch_id, &mut buf);
    Any {
        type_url: "/pole.chain.pole.v1.MsgOpenChallenge".to_string(),
        value: buf,
    }
}

// --- low-level proto wire format helpers --------------------------------

/// Encode field tag (varint).
pub(crate) fn encode_tag(field_number: u32, wire_type: u32, buf: &mut Vec<u8>) {
    let tag = (field_number << 3) | (wire_type & 0x7);
    encode_varint(tag as u64, buf);
}

/// Encode a varint. (Proto3 uses standard unsigned LEB128.)
pub(crate) fn encode_varint(mut value: u64, buf: &mut Vec<u8>) {
    while value >= 0x80 {
        buf.push((value as u8 & 0x7F) | 0x80);
        value >>= 7;
    }
    buf.push(value as u8);
}

/// Encode a length-delimited byte string (wire type 2).
pub(crate) fn encode_bytes(field_number: u32, value: &[u8], buf: &mut Vec<u8>) {
    encode_tag(field_number, 2, buf);
    encode_varint(value.len() as u64, buf);
    buf.extend_from_slice(value);
}

/// Encode a UTF-8 string as a length-delimited field.
pub(crate) fn encode_string(field_number: u32, value: &str, buf: &mut Vec<u8>) {
    encode_bytes(field_number, value.as_bytes(), buf);
}

/// Encode a uint64 as a varint field (wire type 0).
pub(crate) fn encode_uint64(field_number: u32, value: u64, buf: &mut Vec<u8>) {
    encode_tag(field_number, 0, buf);
    encode_varint(value, buf);
}

/// Encode a bool as a varint (wire type 0).
#[allow(dead_code)]
pub(crate) fn encode_bool(field_number: u32, value: bool, buf: &mut Vec<u8>) {
    encode_uint64(field_number, value as u64, buf);
}

/// Encode a nested message (any prost Message that implements
/// `Message::encode_to_vec`) as a length-delimited field.
#[allow(dead_code)]
pub(crate) fn encode_message<M: prost::Message>(field_number: u32, msg: &M, buf: &mut Vec<u8>) {
    encode_bytes(field_number, &msg.encode_to_vec(), buf);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_encoding_matches_proto_spec() {
        let mut buf = Vec::new();
        encode_varint(1, &mut buf);
        assert_eq!(buf, vec![0x01]);
        encode_varint(300, &mut buf);
        assert_eq!(buf, vec![0x01, 0xAC, 0x02]);
    }

    #[test]
    fn finalize_epoch_encodes_to_expected_bytes() {
        let any = encode_msg_finalize_epoch("cosmos1abc", 42);
        assert_eq!(any.type_url, "/pole.chain.pole.v1.MsgFinalizeEpoch");
        // Field 1 (string): tag=0x0A, length=10, "cosmos1abc"
        // Field 2 (uint64): tag=0x10, value=42
        assert_eq!(
            any.value,
            vec![
                0x0A, 0x0A, b'c', b'o', b's', b'm', b'o', b's', b'1', b'a', b'b', b'c',
                0x10, 0x2A,
            ]
        );
    }

    #[test]
    fn claim_reward_handles_empty_recipient() {
        let any = encode_msg_claim_reward("cosmos1abc", 1, "");
        // Three fields, all should encode cleanly
        assert!(!any.value.is_empty());
        // Last byte should mark the end of the empty string field
        assert_eq!(any.value.last(), Some(&0x00));
    }
}
