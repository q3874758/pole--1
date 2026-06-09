use crate::wallet::error::{Result, WalletError};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeyPair {
    pub secret: [u8; 32],
    pub public: [u8; 32],
    pub address: [u8; 32],
}

impl KeyPair {
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let signing_key = SigningKey::from_bytes(seed);
        let verifying_key = signing_key.verifying_key();
        let public = verifying_key.to_bytes();
        let address = crate::stable_hash32(&public);
        Self {
            secret: *seed,
            public,
            address,
        }
    }

    pub fn sign(&self, message: &[u8]) -> Vec<u8> {
        let signing_key = SigningKey::from_bytes(&self.secret);
        let signature = signing_key.sign(message);
        signature.to_bytes().to_vec()
    }

    pub fn verify(&self, message: &[u8], signature: &[u8]) -> bool {
        let verifying_key = match VerifyingKey::from_bytes(&self.public) {
            Ok(k) => k,
            Err(_) => return false,
        };
        let sig_bytes: [u8; 64] = match signature.try_into() {
            Ok(b) => b,
            Err(_) => return false,
        };
        let sig = Signature::from_bytes(&sig_bytes);
        verifying_key.verify(message, &sig).is_ok()
    }

    pub fn address_hex(&self) -> String {
        hex_encode(&self.address)
    }

    pub fn public_hex(&self) -> String {
        hex_encode(&self.public)
    }

    pub fn secret_hex(&self) -> String {
        hex_encode(&self.secret)
    }
}

pub fn hex_encode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}

pub fn hex_decode(hex: &str) -> Result<Vec<u8>> {
    let hex = hex.trim();
    if hex.len() % 2 != 0 {
        return Err(WalletError::InvalidHex("odd length".to_string()));
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    for i in (0..hex.len()).step_by(2) {
        let byte = u8::from_str_radix(&hex[i..i + 2], 16)
            .map_err(|_| WalletError::InvalidHex(hex.to_string()))?;
        out.push(byte);
    }
    Ok(out)
}

/// Derives a child key from a parent key pair.
///
/// NOTE: Full BIP44 hierarchical derivation is not yet implemented.
/// Currently returns a clone of the parent key.
/// This is sufficient for single-key wallets; multi-account support
/// will require proper BIP44 HD derivation.
pub fn derive_child_key(parent: &KeyPair, _index: u32) -> KeyPair {
    parent.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_sign_verify() {
        let seed = [0u8; 32];
        let keypair = KeyPair::from_seed(&seed);
        let message = b"hello world";
        let sig = keypair.sign(message);
        assert!(keypair.verify(message, &sig));
        assert!(!keypair.verify(b"wrong message", &sig));
    }

    #[test]
    fn test_address_hex() {
        let seed = [0x41u8; 32];
        let keypair = KeyPair::from_seed(&seed);
        let addr = keypair.address_hex();
        assert_eq!(addr.len(), 64);
    }
}
