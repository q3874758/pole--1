use crate::wallet::error::{Result, WalletError};
use crate::wallet::keys::KeyPair;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};

use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize)]
struct CryptoJson {
    cipher: String,
    kdf: String,
    salt: String,
    nonce: String,
    ciphertext: String,
}

#[derive(Serialize, Deserialize)]
struct KeystoreJson {
    version: u8,
    address: String,
    crypto: CryptoJson,
    metadata: MetadataJson,
}

#[derive(Serialize, Deserialize)]
struct MetadataJson {
    created_at: u64,
    comment: Option<String>,
}

pub struct EncryptedKeystore {
    pub keypair: KeyPair,
    pub comment: Option<String>,
    pub created_at: u64,
}

impl EncryptedKeystore {
    pub fn new(keypair: KeyPair, comment: Option<String>) -> Self {
        Self {
            keypair,
            comment,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; 32]> {
        let params =
            scrypt::Params::new(14, 8, 1, 32).map_err(|e| WalletError::KdfError(e.to_string()))?;
        let mut key = [0u8; 32];
        let res = scrypt::scrypt(password.as_bytes(), salt, &params, &mut key);
        if res.is_err() {
            return Err(WalletError::KdfError("scrypt error".to_string()));
        }
        Ok(key)
    }

    pub fn encrypt(&self, password: &str, path: &Path) -> Result<()> {
        let mut salt = [0u8; 32];
        rand::rng().fill_bytes(&mut salt);
        let key = Self::derive_key(password, &salt)?;

        let mut nonce_arr = [0u8; 12];
        rand::rng().fill_bytes(&mut nonce_arr);
        let nonce_bytes = aes_gcm::Nonce::from_slice(&nonce_arr);
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| WalletError::EncryptionFailed(e.to_string()))?;

        let plaintext = self.keypair.secret_hex();
        let ciphertext = cipher
            .encrypt(nonce_bytes, plaintext.as_bytes())
            .map_err(|e| WalletError::EncryptionFailed(e.to_string()))?;

        let keystore = KeystoreJson {
            version: 1,
            address: self.keypair.address_hex(),
            crypto: CryptoJson {
                cipher: "aes-256-gcm".to_string(),
                kdf: "scrypt".to_string(),
                salt: hex::encode(salt.as_slice()),
                nonce: hex::encode(nonce_arr),
                ciphertext: hex::encode(ciphertext),
            },
            metadata: MetadataJson {
                created_at: self.created_at,
                comment: self.comment.clone(),
            },
        };

        let json = serde_json::to_string_pretty(&keystore)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn decrypt(password: &str, path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let store: KeystoreJson = serde_json::from_str(&content)
            .map_err(|e| WalletError::InvalidKeystore(e.to_string()))?;

        let salt = hex::decode(&store.crypto.salt)?;
        let nonce_bytes = hex::decode(&store.crypto.nonce)?;
        let ciphertext = hex::decode(&store.crypto.ciphertext)?;

        let key = Self::derive_key(password, &salt)?;

        let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| WalletError::DecryptionFailed)?;

        let nonce = Nonce::from_slice(&nonce_bytes);
        let plaintext = cipher
            .decrypt(nonce, ciphertext.as_ref())
            .map_err(|_| WalletError::DecryptionFailed)?;

        let secret_hex = String::from_utf8(plaintext)
            .map_err(|_| WalletError::InvalidKeystore("invalid secret hex".to_string()))?;

        let secret_bytes = hex::decode(&secret_hex)?
            .try_into()
            .map_err(|_| WalletError::InvalidKeystore("secret must be 32 bytes".to_string()))?;

        let keypair = KeyPair::from_seed(&secret_bytes);

        Ok(Self {
            keypair,
            comment: store.metadata.comment,
            created_at: store.metadata.created_at,
        })
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        let seed = [0x22u8; 32];
        let keypair = KeyPair::from_seed(&seed);
        let ks = EncryptedKeystore::new(keypair, Some("test".to_string()));

        let tmp = std::env::temp_dir().join("wallet_test_keystore.json");
        let path = &tmp;

        ks.encrypt("password123", path).unwrap();

        let decrypted = EncryptedKeystore::decrypt("password123", path).unwrap();
        assert_eq!(decrypted.keypair.address_hex(), ks.keypair.address_hex());
        assert_eq!(decrypted.comment, Some("test".to_string()));

        let bad = EncryptedKeystore::decrypt("wrongpassword", path);
        assert!(bad.is_err());

        let _ = std::fs::remove_file(path);
    }
}
