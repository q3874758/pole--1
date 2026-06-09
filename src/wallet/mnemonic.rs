use crate::wallet::error::{Result, WalletError};
use once_cell::sync::Lazy;
use std::sync::RwLock;

static BIP39_WORDLIST: Lazy<RwLock<Vec<&'static str>>> = Lazy::new(|| {
    let content = include_str!("bip39_wordlist.txt");
    let mut words: Vec<&'static str> = Vec::with_capacity(2048);
    for line in content.lines() {
        let trimmed = line.trim();
        let owned = String::from(trimmed);
        let boxed = owned.into_boxed_str();
        let leaked = Box::leak(boxed);
        words.push(leaked);
    }
    RwLock::new(words)
});

pub fn word_at_index(index: usize) -> &'static str {
    BIP39_WORDLIST.read().unwrap()[index]
}

pub fn word_to_index(word: &str) -> Result<usize> {
    let lower = word.to_lowercase();
    BIP39_WORDLIST
        .read()
        .unwrap()
        .iter()
        .position(|&w| w == lower)
        .ok_or_else(|| WalletError::InvalidWord(word.to_string()))
}

pub struct Mnemonic {
    words: Vec<String>,
}

impl Mnemonic {
    pub fn from_words(words: &[String]) -> Result<Self> {
        if words.len() != 24 {
            return Err(WalletError::InvalidWordCount(words.len()));
        }
        let list = BIP39_WORDLIST.read().unwrap();
        let mut validated = Vec::with_capacity(24);
        for w in words {
            let lower = w.to_lowercase();
            if !list.contains(&lower.as_str()) {
                return Err(WalletError::InvalidWord(lower.clone()));
            }
            validated.push(lower);
        }
        drop(list);
        Ok(Self { words: validated })
    }

    pub fn words(&self) -> &[String] {
        &self.words
    }

    pub fn to_seed(&self, passphrase: &str) -> Vec<u8> {
        let phrase = self.words.join(" ");
        let salt = format!("mnemonic{}", passphrase);
        let params = scrypt::Params::new(14, 8, 1, 32).expect("valid scrypt params");
        let mut output = [0u8; 32];
        let _ = scrypt::scrypt(phrase.as_bytes(), salt.as_bytes(), &params, &mut output);
        output.to_vec()
    }

    pub fn sentence(&self) -> String {
        self.words.join(" ")
    }
}

pub fn generate_mnemonic() -> Mnemonic {
    let mut entropy_bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut entropy_bytes);
    let hash = crate::stable_hash32(&entropy_bytes);
    let checksum = hash[0];
    let mut entropy_bits = Vec::with_capacity(264);
    for byte in entropy_bytes.iter() {
        for i in (0..8).rev() {
            entropy_bits.push((byte >> i) & 1);
        }
    }
    for i in (0..8).rev() {
        entropy_bits.push((checksum >> i) & 1);
    }
    let list = BIP39_WORDLIST.read().unwrap();
    let mut words_out = Vec::with_capacity(24);
    for chunk in entropy_bits.chunks(11) {
        let idx: usize = chunk
            .iter()
            .fold(0usize, |acc, &b| (acc << 1) | (b as usize));
        words_out.push(list[idx].to_string());
    }
    Mnemonic { words: words_out }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wordlist_count() {
        let list = BIP39_WORDLIST.read().unwrap();
        assert_eq!(list.len(), 2048);
        assert_eq!(list[0], "abandon");
    }

    #[test]
    fn test_word_to_index() {
        assert_eq!(word_to_index("abandon").unwrap(), 0);
        assert!(word_to_index("notaword").is_err());
    }
}
