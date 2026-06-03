use bech32::{encode, FromBase32, ToBase32, Variant};
use serde::{Deserialize, Serialize};

use crate::cosmos::error::{CosmosError, Result};
use crate::primitives::{Address, NodeId};

/// Default bech32 prefix for Cosmos mainnet / testnet accounts.
/// Override via `CosmosClient::with_prefix` for `pole1...` style chains.
pub const DEFAULT_BECH32_PREFIX: &str = "cosmos";

/// Wire format for the 20-byte account address derived from a 32-byte hash.
pub const ACCOUNT_ADDRESS_LEN: usize = 20;

/// Address type wrapping either the raw 32-byte hash (Rust internal) or
/// the 20-byte account identifier used by the Cosmos SDK.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CosmosAddress {
    /// 20 bytes — what the chain actually verifies against.
    pub account: Vec<u8>,
    /// bech32 representation, e.g. `cosmos1...`
    pub bech32: String,
}

impl CosmosAddress {
    pub fn prefix(&self) -> &str {
        // safe: bech32 strings are ASCII and always contain a `1`
        self.bech32.split('1').next().unwrap_or(DEFAULT_BECH32_PREFIX)
    }
}

impl From<CosmosAddress> for String {
    fn from(addr: CosmosAddress) -> Self {
        addr.bech32
    }
}

impl TryFrom<String> for CosmosAddress {
    type Error = CosmosError;
    fn try_from(s: String) -> Result<Self> {
        // Caller already gave us a bech32 string — wrap it without
        // re-decoding. If the account bytes are needed, call
        // `bech32_to_address` afterwards.
        Ok(CosmosAddress {
            account: decode_bech32(s.split('1').next().unwrap_or(""), &s)?,
            bech32: s,
        })
    }
}

impl Serialize for CosmosAddress {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.bech32)
    }
}

impl<'de> Deserialize<'de> for CosmosAddress {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        CosmosAddress::try_from(s).map_err(serde::de::Error::custom)
    }
}

/// Encode a bech32 string with the given prefix.
pub fn encode_bech32(prefix: &str, bytes: &[u8]) -> Result<String> {
    Ok(encode(prefix, bytes.to_base32(), Variant::Bech32)?)
}

/// Decode a bech32 string and verify the prefix matches `expected`.
pub fn decode_bech32(expected_prefix: &str, addr: &str) -> Result<Vec<u8>> {
    let (prefix, data, _) = bech32::decode(addr)?;
    if prefix != expected_prefix {
        return Err(CosmosError::Bech32PrefixMismatch {
            expected: expected_prefix.to_string(),
            actual: prefix,
        });
    }
    Ok(Vec::<u8>::from_base32(&data)?)
}

/// Hex (lowercase) ↔ bech32 round-trip. Operates on the raw bytes the hex
/// string decodes to, so this works for any fixed-width key.
pub fn hex_to_bech32(prefix: &str, hex: &str) -> Result<String> {
    let bytes = hex::decode(hex.trim_start_matches("0x"))?;
    encode_bech32(prefix, &bytes)
}

pub fn bech32_to_hex(addr: &str) -> Result<String> {
    let bytes = decode_bech32(DEFAULT_BECH32_PREFIX, addr)
        .or_else(|_| decode_bech32(addr.split('1').next().unwrap_or(""), addr))?;
    Ok(hex::encode(bytes))
}

/// Rust internal 32-byte `Address` → bech32 account address.
///
/// Takes the first 20 bytes of the 32-byte address as the Cosmos SDK account
/// identifier (matches the convention used by the existing `wallet` module).
pub fn address_to_bech32(prefix: &str, address: &Address) -> Result<String> {
    let account = &address[..ACCOUNT_ADDRESS_LEN];
    let bech32 = encode_bech32(prefix, account)?;
    Ok(bech32)
}

/// Rust internal 32-byte `NodeId` → bech32 account address.
pub fn node_id_to_bech32(prefix: &str, node_id: &NodeId) -> Result<String> {
    let account = &node_id[..ACCOUNT_ADDRESS_LEN];
    let bech32 = encode_bech32(prefix, account)?;
    Ok(bech32)
}

/// bech32 string → 32-byte `Address` (zero-padded to 32 bytes on the left).
pub fn bech32_to_address(addr: &str) -> Result<Address> {
    let bytes = decode_bech32(DEFAULT_BECH32_PREFIX, addr)
        .or_else(|_| decode_bech32(addr.split('1').next().unwrap_or(""), addr))?;
    if bytes.len() > 32 {
        return Err(CosmosError::InvalidBech32(format!(
            "decoded length {} exceeds 32",
            bytes.len()
        )));
    }
    let mut out = [0u8; 32];
    let offset = 32 - bytes.len();
    out[offset..].copy_from_slice(&bytes);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bech32_roundtrip_account() {
        let hex = "0123456789abcdef0123456789abcdef01234567";
        let bech = hex_to_bech32(DEFAULT_BECH32_PREFIX, hex).unwrap();
        let back = bech32_to_hex(&bech).unwrap();
        assert_eq!(hex, back);
    }

    #[test]
    fn address_to_bech32_uses_20_bytes() {
        // Build a 32-byte address whose last 20 bytes are all set (no
        // leading zero ambiguity in bech32's 5-bit packing).
        let addr = [0xABu8; 32];
        let bech = address_to_bech32(DEFAULT_BECH32_PREFIX, &addr).unwrap();
        // bech32 strings start with `<prefix>1`
        assert!(bech.starts_with("cosmos1"));
        // Round-trip via `bech32_to_address` left-pads to 32 bytes.
        let back = bech32_to_address(&bech).unwrap();
        // The last 20 bytes should match; the first 12 are zero-padded.
        assert_eq!(back[12..], addr[12..]);
        // Sanity: the first 12 are zero.
        assert!(back[..12].iter().all(|b| *b == 0));
    }

    #[test]
    fn prefix_mismatch_is_rejected() {
        let hex = "00".repeat(20);
        let bech = hex_to_bech32("cosmos", &hex).unwrap();
        let err = decode_bech32("osmo", &bech).unwrap_err();
        assert!(matches!(err, CosmosError::Bech32PrefixMismatch { .. }));
    }
}
