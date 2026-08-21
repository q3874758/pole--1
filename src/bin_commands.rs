//! Shared CLI command handlers merged from the `pole-client` / `pole-node`
//! binaries into the library so the two can eventually be unified into a
//! single executable.
//!
//! Commands are moved here incrementally; each batch keeps
//! `cargo test` / `cargo clippy -D warnings` / `cargo fmt` green.

use std::path::{Path, PathBuf};

/// Default client config path, used by client-side command handlers.
pub const DEFAULT_CONFIG_PATH: &str = "./node.json";

/// `wallet-create [data-dir] [password] [comment]` — create a new wallet.
pub fn wallet_create_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = args.get(2).map(PathBuf::from).unwrap_or_else(|| {
        crate::cli_support::default_data_dir_for_config(Path::new(DEFAULT_CONFIG_PATH)).into()
    });
    let password = args
        .get(3)
        .cloned()
        .unwrap_or_else(|| rpassword::prompt_password("password: ").unwrap_or_default());
    let comment = args.get(4).cloned();

    let mnemonic = crate::wallet::create_wallet(&data_dir, comment, &password)?;
    println!("wallet_created");
    println!("mnemonic={}", mnemonic);
    println!("data_dir={}", data_dir.display());
    Ok(())
}

/// `wallet-recover [data-dir] [password] <24-word-mnemonic...>` — recover
/// a wallet from its mnemonic.
pub fn wallet_recover_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = args
        .get(2)
        .map(PathBuf::from)
        .ok_or("usage: wallet-recover [data-dir] [password] <24-word-mnemonic...>")?;
    let password = args
        .get(3)
        .cloned()
        .unwrap_or_else(|| rpassword::prompt_password("password: ").unwrap_or_default());
    let words: Vec<String> = args[4..].to_vec();
    if words.len() != 24 {
        return Err("mnemonic must be exactly 24 words".into());
    }

    let address = crate::wallet::recover_wallet(&words[..], &data_dir, None, &password)?;
    println!("wallet_recovered");
    println!("address={}", address);
    println!("data_dir={}", data_dir.display());
    Ok(())
}

/// `wallet-address [data-dir] [password]` — print the wallet address.
pub fn wallet_address_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = args.get(2).map(PathBuf::from).unwrap_or_else(|| {
        crate::cli_support::default_data_dir_for_config(Path::new(DEFAULT_CONFIG_PATH)).into()
    });
    let password = args
        .get(3)
        .cloned()
        .unwrap_or_else(|| rpassword::prompt_password("wallet password: ").unwrap_or_default());
    let address = crate::wallet::show_address_with_password(&data_dir, &password)?;
    println!("{}", address);
    Ok(())
}

/// `wallet-set-reward-address <config-path> [data-dir] [password]` — set the
/// node's reward address from the wallet.
pub fn wallet_set_reward_address_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = PathBuf::from(
        args.get(2)
            .ok_or("usage: wallet-set-reward-address <config-path> [data-dir] [password]")?,
    );
    let data_dir = args.get(3).map(PathBuf::from).unwrap_or_else(|| {
        crate::cli_support::default_data_dir_for_config(Path::new(DEFAULT_CONFIG_PATH)).into()
    });
    let password = args
        .get(4)
        .cloned()
        .unwrap_or_else(|| rpassword::prompt_password("wallet password: ").unwrap_or_default());

    let address = crate::wallet::set_reward_address(&data_dir, &config_path, &password)?;
    println!("reward_address_updated");
    println!("address={}", address);
    println!("config={}", config_path.display());
    Ok(())
}
