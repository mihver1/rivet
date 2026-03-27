use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::Aead,
};
use argon2::Argon2;
use rand::RngCore;
use rand::rngs::OsRng;
use zeroize::Zeroize;

use rivet_core::error::{Result, RivetError};

const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;
const TAG_LEN: usize = 16;

#[derive(Debug, Clone)]
pub struct Argon2Params {
    pub memory_cost: u32,
    pub time_cost: u32,
    pub parallelism: u32,
    pub salt: Vec<u8>,
}

impl Default for Argon2Params {
    fn default() -> Self {
        Self {
            memory_cost: 64 * 1024, // 64 MB
            time_cost: 3,
            parallelism: 4,
            salt: generate_salt(),
        }
    }
}

pub fn generate_salt() -> Vec<u8> {
    let mut salt = vec![0u8; 32];
    OsRng.fill_bytes(&mut salt);
    salt
}

pub fn generate_dek() -> [u8; KEY_LEN] {
    let mut key = [0u8; KEY_LEN];
    OsRng.fill_bytes(&mut key);
    key
}

pub fn derive_kek(password: &str, params: &Argon2Params) -> Result<[u8; KEY_LEN]> {
    let argon2 = Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon2::Params::new(
            params.memory_cost,
            params.time_cost,
            params.parallelism,
            Some(KEY_LEN),
        )
        .map_err(|e| RivetError::CryptoError(format!("argon2 params: {e}")))?,
    );

    let mut kek = [0u8; KEY_LEN];
    argon2
        .hash_password_into(password.as_bytes(), &params.salt, &mut kek)
        .map_err(|e| RivetError::CryptoError(format!("argon2 hash: {e}")))?;

    Ok(kek)
}

pub fn encrypt_aes_gcm(key: &[u8; KEY_LEN], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| RivetError::CryptoError(e.to_string()))?;

    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| RivetError::CryptoError(format!("encrypt: {e}")))?;

    // Format: nonce || ciphertext (includes tag)
    let mut result = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&ciphertext);
    Ok(result)
}

pub fn decrypt_aes_gcm(key: &[u8; KEY_LEN], data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < NONCE_LEN + TAG_LEN {
        return Err(RivetError::CryptoError("data too short".into()));
    }

    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| RivetError::CryptoError(e.to_string()))?;

    let nonce = Nonce::from_slice(&data[..NONCE_LEN]);
    let ciphertext = &data[NONCE_LEN..];

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| RivetError::InvalidPassword)?;

    Ok(plaintext)
}

pub fn encrypt_dek(kek: &[u8; KEY_LEN], dek: &[u8; KEY_LEN]) -> Result<Vec<u8>> {
    encrypt_aes_gcm(kek, dek)
}

pub fn decrypt_dek(kek: &[u8; KEY_LEN], encrypted: &[u8]) -> Result<[u8; KEY_LEN]> {
    let mut plaintext = decrypt_aes_gcm(kek, encrypted)?;
    if plaintext.len() != KEY_LEN {
        plaintext.zeroize();
        return Err(RivetError::CryptoError("invalid DEK length".into()));
    }
    let mut dek = [0u8; KEY_LEN];
    dek.copy_from_slice(&plaintext);
    plaintext.zeroize();
    Ok(dek)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = generate_dek();
        let plaintext = b"hello, rivet!";

        let encrypted = encrypt_aes_gcm(&key, plaintext).unwrap();
        let decrypted = decrypt_aes_gcm(&key, &encrypted).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_kek_dek_roundtrip() {
        let params = Argon2Params {
            memory_cost: 1024, // low for fast tests
            time_cost: 1,
            parallelism: 1,
            salt: generate_salt(),
        };

        let kek = derive_kek("my-password", &params).unwrap();
        let dek = generate_dek();

        let encrypted_dek = encrypt_dek(&kek, &dek).unwrap();
        let decrypted_dek = decrypt_dek(&kek, &encrypted_dek).unwrap();

        assert_eq!(dek, decrypted_dek);
    }

    #[test]
    fn test_wrong_password_fails() {
        let params = Argon2Params {
            memory_cost: 1024,
            time_cost: 1,
            parallelism: 1,
            salt: generate_salt(),
        };

        let correct_kek = derive_kek("correct-password", &params).unwrap();
        let wrong_kek = derive_kek("wrong-password", &params).unwrap();
        let dek = generate_dek();

        let encrypted_dek = encrypt_dek(&correct_kek, &dek).unwrap();
        let result = decrypt_dek(&wrong_kek, &encrypted_dek);

        assert!(result.is_err());
    }

    #[test]
    fn test_different_nonces() {
        let key = generate_dek();
        let plaintext = b"same data";

        let encrypted1 = encrypt_aes_gcm(&key, plaintext).unwrap();
        let encrypted2 = encrypt_aes_gcm(&key, plaintext).unwrap();

        // Different nonces produce different ciphertexts
        assert_ne!(encrypted1, encrypted2);

        // But both decrypt to the same plaintext
        let decrypted1 = decrypt_aes_gcm(&key, &encrypted1).unwrap();
        let decrypted2 = decrypt_aes_gcm(&key, &encrypted2).unwrap();
        assert_eq!(decrypted1, decrypted2);
    }

    #[test]
    fn test_data_too_short() {
        let key = generate_dek();
        let result = decrypt_aes_gcm(&key, &[0; 10]);
        assert!(result.is_err());
    }
}
