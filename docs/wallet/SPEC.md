# Wallet Module Specification

## Overview

A complete HD wallet system for PoLE, supporting BIP39 mnemonic generation, BIP44 key derivation, encrypted keystore storage, and transaction signing using ed25519.

## Architecture

```
wallet/
├── lib.rs           # Public API
├── mnemonic.rs      # BIP39 24-word mnemonic generation/parsing
├── keys.rs          # Key derivation (BIP32) + ed25519 signing
├── keystore.rs      # Encrypted file storage (AES-256-GCM + Argon2)
├── commands.rs      # CLI subcommands
└── error.rs         # WalletError type
```

## Data Structures

### MnemonicWord
24-word BIP39 wordlist (2048 words), using English wordlist.

### DerivedKey
- `secret: [u8; 32]` — private seed
- `public: [u8; 32]` — public key
- `address: [u8; 32]` — PoLE address (public key hash or raw public)

### EncryptedKeystore
JSON file stored at `<data_dir>/wallet/keystore.json`:
```json
{
  "version": 1,
  "address": "2222...2222",
  "crypto": {
    "cipher": "aes-256-gcm",
    "kdf": "argon2id",
    "salt": "<hex>",
    "nonce": "<hex>",
    "ciphertext": "<hex>"
  },
  "metadata": {
    "created_at": "<unix timestamp>",
    "comment": "<optional>"
  }
}
```

## CLI Commands

| Command | Description |
|---------|-------------|
| `wallet create [--comment "..."]` | Generate new mnemonic → derive keys → encrypt and save |
| `wallet recover <24 words...>` | Restore from mnemonic → derive keys → save |
| `wallet export --address <addr> --path <file>` | Decrypt keystore → export plaintext seed hex |
| `wallet import --path <keystore.json> --password <pw>` | Import existing encrypted keystore |
| `wallet sign --tx <tx.json> --key <secret_hex>` | Sign a transaction and output signature hex |
| `wallet address` | Derive and display address from keystore |
| `wallet set-reward-address` | Update node.json with wallet's address |

## Flow

1. **Create**: 128-bit entropy → BIP39 mnemonic → BIP39 seed → PBKDF2/Argon2 → 32-byte master seed → BIP44 derivation (m/44'/501'/0'/0') → ed25519 keypair → keystore file (AES-256-GCM encrypted)
2. **Sign**: Load keystore → Argon2 decrypt → derive key → ed25519 sign → return signature bytes
3. **Recover**: Parse 24 words → verify checksum → derive seed → same flow as create

## Dependencies

- `mnemonic` or manual BIP39 implementation (English wordlist, 2048 words)
- `ed25519-dalek` for signing
- `aes-gcm` for encryption
- `argon2` for KDF
- `zeroize` for secret clearing

## Integration

- `NodeConfig.reward_address_hex` can be set via `wallet set-reward-address`
- Wallet keystore lives at `<data_dir>/wallet/keystore.json`
- No automatic transaction signing yet — reward address only