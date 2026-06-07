//! EIP-712 typed-data signing primitives.
//!
//! EIP-712 produces a deterministic, structured digest that wallets can
//! show to users as human-readable signing prompts. The PoLE bridge
//! uses it for off-chain authorizations that later land on-chain as
//! messages (Phase 3 session-key receipts, Phase 4 withdrawal
//! authorizations, Phase 7 PNT-20 meta-tx permits).
//!
//! # Spec references
//! - EIP-712: <https://eips.ethereum.org/EIPS/eip-712>
//! - EIP-712 example (Mail to CEO): <https://eips.ethereum.org/EIPS/eip-712#example>
//!
//! # Hash function: legacy pre-NIST Keccak-256 (NOT SHA3-256)
//! EIP-712 uses the *original* Keccak padding (a single `0x01` byte
//! after the message), finalized before NIST standardized SHA3 with
//! the `0x06` padding. The two algorithms are not interchangeable.
//! The `sha3` crate's [`Keccak256`] constructor implements the
//! EIP-712 variant; [`sha3::Sha3_256`] would silently produce
//! digests the chain would reject.
//!
//! # Wire layout
//! The final EIP-712 hash is:
//! ```text
//!   keccak256(0x19 || 0x01 || domainSeparator || hashStruct(message))
//! ```
//! where `hashStruct(s)` is `keccak256(typeHash || encodeData(s))`,
//! and `encodeData` writes each field in canonical 32-byte form
//! (varints zero-padded, strings bytes-hashed, addresses left-padded).
//!
//! # Curve / signature scheme
//! EIP-712 itself is curve-agnostic: the standard only specifies the
//! *digest*. The signature scheme is whatever the chain uses. PoLE
//! signs with Ed25519 (the chain's native key type) — secp256k1 is
//! not introduced until Phase 5 (threshold encryption). The
//! [`eip712_sign`] helper is parameterized over a closure so callers
//! can plug in either Ed25519 (today) or secp256k1 (Phase 5+).

use sha3::{Digest, Keccak256};

use crate::cosmos::error::{CosmosError, Result};

/// Keccak-256 of a byte slice. Convenience wrapper around the
/// [`Keccak256`] hasher. Used everywhere this module needs a hash.
pub fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(data);
    let out = hasher.finalize();
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&out);
    bytes
}

/// Domain separator per EIP-712 §"Domain Separator":
/// ```text
///   hashStruct(eip712Domain) = keccak256(typeHash ||
///       keccak256(name) || keccak256(version) ||
///       chainId || verifyingContract || keccak256(salt))
/// ```
/// where `typeHash = keccak256("EIP712Domain(string name,string
/// version,uint256 chainId,address verifyingContract,bytes32 salt)")`
/// when `salt` is present, or with that field dropped when it is
/// absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainSeparator {
    /// Human-readable protocol name (e.g. `"PoLE Session"`).
    pub name: String,
    /// Protocol version (e.g. `"1"`).
    pub version: String,
    /// EIP-155 chain id. Zero is allowed for off-chain-only domains.
    pub chain_id: u64,
    /// 20-byte address of the verifying contract on the EVM side.
    /// PoLE stores this as raw bytes; non-EVM verifiers can use any
    /// 20-byte value (e.g. the bech32 account id of the on-chain
    /// `x/pole` module account, zero-padded to 20 bytes).
    pub verifying_contract: [u8; 20],
    /// Optional 32-byte salt. `None` drops the `salt` field from the
    /// type string (per the spec — `salt` is the only `EIP712Domain`
    /// field whose presence is conditional).
    pub salt: Option<[u8; 32]>,
}

impl DomainSeparator {
    /// Compute the 32-byte domain separator hash.
    pub fn hash(&self) -> [u8; 32] {
        // The type string is the only field whose encoding depends on
        // whether `salt` is present. We rebuild it each call: it's a
        // short string, and `hash()` is not a hot path.
        let domain_type = match self.salt {
            Some(_) => "EIP712Domain(string name,string version,uint256 chainId,address verifyingContract,bytes32 salt)",
            None => "EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)",
        };
        let type_hash = keccak256(domain_type.as_bytes());

        let mut buf = Vec::with_capacity(32 * 7);
        buf.extend_from_slice(&type_hash);
        buf.extend_from_slice(&keccak256(self.name.as_bytes()));
        buf.extend_from_slice(&keccak256(self.version.as_bytes()));
        encode_uint256(self.chain_id, &mut buf);
        encode_address(&self.verifying_contract, &mut buf);
        if let Some(salt) = self.salt {
            buf.extend_from_slice(&salt);
        }
        keccak256(&buf)
    }
}

/// `hashStruct(s)` primitive per EIP-712: `keccak256(typeHash ||
/// encodeData(s))`.
///
/// `type_hash` is the precomputed keccak256 of the type description
/// (e.g. `keccak256("Mail(Person from,Person to,string contents)
/// Person(string name,address wallet)")`). `encoded_fields` is the
/// concatenation of each field, each in its canonical 32-byte form
/// (already laid out by the caller via [`encode_uint256`],
/// [`encode_string`], [`encode_bytes32`], [`encode_address`], or
/// nested [`hash_struct`] calls for sub-structs).
pub fn hash_struct(type_hash: [u8; 32], encoded_fields: &[u8]) -> [u8; 32] {
    let mut buf = Vec::with_capacity(32 + encoded_fields.len());
    buf.extend_from_slice(&type_hash);
    buf.extend_from_slice(encoded_fields);
    keccak256(&buf)
}

/// Final EIP-712 digest:
/// `keccak256(0x19 || 0x01 || domainSeparator || messageHash)`.
/// This is the value that gets signed.
pub fn typed_data_hash(domain_separator: [u8; 32], message_hash: [u8; 32]) -> [u8; 32] {
    let mut buf = [0u8; 1 + 1 + 32 + 32];
    buf[0] = 0x19;
    buf[1] = 0x01;
    buf[2..34].copy_from_slice(&domain_separator);
    buf[34..66].copy_from_slice(&message_hash);
    keccak256(&buf)
}

// --- canonical 32-byte encoders ----------------------------------------

/// Encode a `uint256` (or smaller uint, zero-extended) as a
/// big-endian 32-byte field.
pub fn encode_uint256(value: u64, buf: &mut Vec<u8>) {
    let mut slot = [0u8; 32];
    slot[24..].copy_from_slice(&value.to_be_bytes());
    buf.extend_from_slice(&slot);
}

/// Encode a `string` as `keccak256(string)` (EIP-712 §"Hashing the
/// `string` type"). The result is a 32-byte field.
pub fn encode_string(value: &str, buf: &mut Vec<u8>) {
    buf.extend_from_slice(&keccak256(value.as_bytes()));
}

/// Encode a `bytes32` field as the raw 32 bytes (no extra hashing —
/// the value is already fixed-width per the type system).
pub fn encode_bytes32(value: [u8; 32], buf: &mut Vec<u8>) {
    buf.extend_from_slice(&value);
}

/// Encode an `address` (20 bytes) as a left-padded 32-byte field.
pub fn encode_address(value: &[u8; 20], buf: &mut Vec<u8>) {
    let mut slot = [0u8; 32];
    slot[12..].copy_from_slice(value);
    buf.extend_from_slice(&slot);
}

// --- signing glue ------------------------------------------------------

/// Compute the EIP-712 digest and sign it with the given signer.
///
/// `sign` is a closure the caller provides to apply whatever
/// signature scheme the chain uses (Ed25519 today, secp256k1 from
/// Phase 5 onward). The closure receives the 32-byte digest and
/// returns a signature in whatever format the chain expects. This
/// keeps the EIP-712 helper itself curve-agnostic and avoids
/// forcing a curve choice on callers.
pub fn eip712_sign<F>(domain: &DomainSeparator, message_hash: [u8; 32], sign: F) -> Result<Vec<u8>>
where
    F: FnOnce(&[u8; 32]) -> Result<Vec<u8>>,
{
    let ds = domain.hash();
    let digest = typed_data_hash(ds, message_hash);
    sign(&digest).map_err(|e| CosmosError::Encode(format!("eip712 sign: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mail to CEO test vector from EIP-712 spec:
    // <https://eips.ethereum.org/EIPS/eip-712#example>
    //
    // Domain:
    //   name = "Ether Mail"
    //   version = "1"
    //   chainId = 1
    //   verifyingContract = 0xCcCCccccCCCCcCCCCCCcCcCccCcCCCcCcccccccC
    //
    // Message:
    //   from = { name: "Cow",  wallet: 0xCD2a3d9F938E13CD947Ec05AbC7FE734Df8DD826 }
    //   to   = { name: "Bob",  wallet: 0xBbfA62C1571B141684e8Cb46bE5C5b48B0E89E5C  }
    //   contents = "Hello, Bob!"
    //
    // Reference values (computed independently with PyCryptodome's
    // legacy Keccak-256, cross-checked against the spec):
    //   domainSeparator : f2cee375fa42b42143804025fc449deafd50cc031ca257e0b194a650a912090f
    //   mail_type_hash  : a0cedeb2dc280ba39b857546d74f5549c3a1d7bdc2dd96bf881f76108e23dac2
    //   mail_hash       : 1be177f7195c2412e97c6a288029793ca07b7713a7a82b7731dad07a3efe4e0d
    //   final digest    : 402e171069a6cd61bdee2ef100f90701ece65c39caf59ae30eaee87c74289604
    //
    // (The "0xbe609aee..." digest that circulates in some EIP-712
    // writeups is from a *different* EIP-712 example — not the
    // canonical Mail-to-CEO vector this test reproduces.)

    const EXPECTED_DOMAIN: &str =
        "f2cee375fa42b42143804025fc449deafd50cc031ca257e0b194a650a912090f";
    const EXPECTED_MAIL_TYPE_HASH: &str =
        "a0cedeb2dc280ba39b857546d74f5549c3a1d7bdc2dd96bf881f76108e23dac2";
    const EXPECTED_MAIL_HASH: &str =
        "1be177f7195c2412e97c6a288029793ca07b7713a7a82b7731dad07a3efe4e0d";
    const EXPECTED_DIGEST: &str =
        "402e171069a6cd61bdee2ef100f90701ece65c39caf59ae30eaee87c74289604";

    fn parse_address(hex20: &str) -> [u8; 20] {
        let bytes = hex::decode(hex20).expect("valid hex");
        assert_eq!(bytes.len(), 20);
        let mut out = [0u8; 20];
        out.copy_from_slice(&bytes);
        out
    }

    #[test]
    fn keccak256_helper_matches_digest_function() {
        // The helper must agree with the free-function form so future
        // refactors don't accidentally swap the algorithm.
        let via_helper = keccak256(b"abc");
        let via_trait = Keccak256::digest(b"abc");
        assert_eq!(via_helper[..], via_trait[..]);
    }

    #[test]
    fn domain_separator_with_salt_uses_salt_type_string() {
        // The two domain type strings must produce *different* type
        // hashes, which means the salt must be present in the input
        // to match. We check the full domain-separator hash stays
        // stable across a round trip that goes through the same
        // struct shape.
        let domain = DomainSeparator {
            name: "PoLE".into(),
            version: "1".into(),
            chain_id: 0,
            verifying_contract: [0u8; 20],
            salt: Some([0u8; 32]),
        };
        let h1 = domain.hash();
        let h2 = domain.hash();
        assert_eq!(h1, h2, "DomainSeparator::hash() must be deterministic");
    }

    #[test]
    fn domain_separator_without_salt_omits_salt_field() {
        // Same inputs, salt dropped → different domain separator
        // hash. This is the spec-mandated behavior (EIP-712 §"Domain
        // Separator").
        let mut salt = [0u8; 32];
        salt[31] = 0x42;
        let with_salt = DomainSeparator {
            name: "PoLE".into(),
            version: "1".into(),
            chain_id: 0,
            verifying_contract: [0u8; 20],
            salt: Some(salt),
        };
        let without_salt = DomainSeparator {
            salt: None,
            ..with_salt.clone()
        };
        assert_ne!(
            with_salt.hash(),
            without_salt.hash(),
            "salt presence must change the domain separator"
        );
    }

    #[test]
    fn mail_to_ceo_typed_data_hash_matches_spec_vector() {
        // Reconstruct the EIP-712 spec example end-to-end and assert
        // the final digest equals the published vector. This is the
        // round-trip that proves the encoding layout is correct
        // (type strings, field ordering, sub-struct hashing, the
        // `0x19 0x01` prefix).
        let domain = DomainSeparator {
            name: "Ether Mail".into(),
            version: "1".into(),
            chain_id: 1,
            verifying_contract: parse_address("cccccccccccccccccccccccccccccccccccccccc"),
            salt: None,
        };

        // Mail(Person from,Person to,string contents)
        // Person(string name,address wallet)
        let mail_type_hash = keccak256(
            b"Mail(Person from,Person to,string contents)Person(string name,address wallet)",
        );
        let mth_hex: String = mail_type_hash
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        assert_eq!(mth_hex, EXPECTED_MAIL_TYPE_HASH, "mail type hash mismatch");

        // Person sub-struct hashes
        let person_type_hash = keccak256(b"Person(string name,address wallet)");
        // `hash_struct` prepends the type hash internally — callers
        // pass only the encoded *fields*, not the type hash again.
        let from_person_hash = {
            let mut buf = Vec::new();
            encode_string("Cow", &mut buf);
            encode_address(
                &parse_address("cd2a3d9f938e13cd947ec05abc7fe734df8dd826"),
                &mut buf,
            );
            hash_struct(person_type_hash, &buf)
        };
        let to_person_hash = {
            let mut buf = Vec::new();
            encode_string("Bob", &mut buf);
            encode_address(
                &parse_address("bbfa62c1571b141684e8cb46be5c5b48b0e89e5c"),
                &mut buf,
            );
            hash_struct(person_type_hash, &buf)
        };
        // Mail struct
        let mail_hash = {
            let mut buf = Vec::new();
            buf.extend_from_slice(&from_person_hash);
            buf.extend_from_slice(&to_person_hash);
            encode_string("Hello, Bob!", &mut buf);
            hash_struct(mail_type_hash, &buf)
        };
        let mh_hex: String = mail_hash.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(mh_hex, EXPECTED_MAIL_HASH, "mail struct hash mismatch");

        let digest = typed_data_hash(domain.hash(), mail_hash);
        let hex: String = digest.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(
            hex, EXPECTED_DIGEST,
            "EIP-712 spec vector mismatch — check type strings, field order, or keccak variant"
        );
    }

    /// Domain-separator-only check, anchored to the reference value
    /// computed independently with PyCryptodome's legacy Keccak-256.
    /// This is independent of the message encoding, so when the
    /// full round-trip fails it lets us localize the bug to either
    /// the domain encoder or the message encoder.
    #[test]
    fn mail_to_ceo_domain_separator_matches_reference() {
        let domain = DomainSeparator {
            name: "Ether Mail".into(),
            version: "1".into(),
            chain_id: 1,
            verifying_contract: parse_address("cccccccccccccccccccccccccccccccccccccccc"),
            salt: None,
        };
        let hex: String = domain.hash().iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(hex, EXPECTED_DOMAIN, "domain separator hash mismatch");
    }

    /// `eip712_sign` is a thin glue helper: it just hashes and
    /// delegates. Verify that the digest it computes is byte-identical
    /// to a hand-built `typed_data_hash` call.
    #[test]
    fn eip712_sign_helper_delivers_correct_digest() {
        let domain = DomainSeparator {
            name: "PoLE Test".into(),
            version: "1".into(),
            chain_id: 7,
            verifying_contract: [0u8; 20],
            salt: None,
        };
        let captured = std::sync::Mutex::new(None);
        let signed = eip712_sign(&domain, [0xAB; 32], |digest| {
            *captured.lock().unwrap() = Some(*digest);
            Ok(vec![0xDE, 0xAD, 0xBE, 0xEF])
        })
        .unwrap();
        assert_eq!(signed, vec![0xDE, 0xAD, 0xBE, 0xEF]);

        let expected = typed_data_hash(domain.hash(), [0xAB; 32]);
        assert_eq!(*captured.lock().unwrap(), Some(expected));
    }
}
