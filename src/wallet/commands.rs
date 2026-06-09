use crate::wallet::error::Result;
use crate::wallet::keys::{hex_decode, KeyPair};
use crate::wallet::keystore::EncryptedKeystore;
use crate::wallet::mnemonic::{generate_mnemonic, Mnemonic};
use std::path::PathBuf;

pub fn create_wallet(
    data_dir: &PathBuf,
    comment: Option<String>,
    password: &str,
) -> Result<String> {
    let mnemonic = generate_mnemonic();
    let seed_bytes: [u8; 32] =
        mnemonic.to_seed("").as_slice().try_into().map_err(|_| {
            crate::wallet::error::WalletError::Crypto("seed not 32 bytes".to_string())
        })?;
    let keypair = KeyPair::from_seed(&seed_bytes);
    let ks = EncryptedKeystore::new(keypair, comment);
    let wallet_dir = data_dir.join("wallet");
    std::fs::create_dir_all(&wallet_dir)?;
    let path = wallet_dir.join("keystore.json");
    ks.encrypt(password, &path)?;
    Ok(mnemonic.sentence())
}

pub fn recover_wallet(
    words: &[String],
    data_dir: &PathBuf,
    comment: Option<String>,
    password: &str,
) -> Result<String> {
    let mnemonic = Mnemonic::from_words(words)?;
    let seed_bytes: [u8; 32] =
        mnemonic.to_seed("").as_slice().try_into().map_err(|_| {
            crate::wallet::error::WalletError::Crypto("seed not 32 bytes".to_string())
        })?;
    let keypair = KeyPair::from_seed(&seed_bytes);
    let address = keypair.address_hex();
    let ks = EncryptedKeystore::new(keypair, comment);
    let wallet_dir = data_dir.join("wallet");
    std::fs::create_dir_all(&wallet_dir)?;
    let path = wallet_dir.join("keystore.json");
    ks.encrypt(password, &path)?;
    Ok(address)
}

pub fn show_address(data_dir: &PathBuf) -> Result<String> {
    let path = data_dir.join("wallet").join("keystore.json");
    let ks = EncryptedKeystore::decrypt("", &path)?;
    Ok(ks.keypair.address_hex())
}

pub fn show_address_with_password(data_dir: &PathBuf, password: &str) -> Result<String> {
    let path = data_dir.join("wallet").join("keystore.json");
    let ks = EncryptedKeystore::decrypt(password, &path)?;
    Ok(ks.keypair.address_hex())
}

pub fn export_secret(data_dir: &PathBuf, password: &str) -> Result<String> {
    let path = data_dir.join("wallet").join("keystore.json");
    let ks = EncryptedKeystore::decrypt(password, &path)?;
    Ok(ks.keypair.secret_hex())
}

pub fn sign_transaction(data_dir: &PathBuf, password: &str, tx_hex: &str) -> Result<String> {
    let path = data_dir.join("wallet").join("keystore.json");
    let ks = EncryptedKeystore::decrypt(password, &path)?;
    let tx_bytes = hex_decode(tx_hex)?;
    let signature = ks.keypair.sign(&tx_bytes);
    Ok(hex_encode(&signature))
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}

pub fn set_reward_address(
    data_dir: &PathBuf,
    config_path: &PathBuf,
    password: &str,
) -> Result<String> {
    let wallet_path = data_dir.join("wallet").join("keystore.json");
    let ks = EncryptedKeystore::decrypt(password, &wallet_path)?;
    let addr_hex = ks.keypair.address_hex();

    let content = std::fs::read_to_string(config_path)?;
    let mut config: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| crate::wallet::error::WalletError::Json(e))?;
    config["reward_address_hex"] = serde_json::Value::String(addr_hex.clone());
    let output = serde_json::to_string_pretty(&config)?;
    std::fs::write(config_path, output)?;
    Ok(addr_hex)
}
