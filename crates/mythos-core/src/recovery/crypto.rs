//! # Cryptographic Engine for Disaster Recovery Packages
//!
//! Implements ChaCha20-Poly1305 authenticated symmetric encryption
//! with key derivation from passphrase and salt.

use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};
use rand::RngCore;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const SALT_LEN: usize = 32;
pub const NONCE_LEN: usize = 12;
pub const KDF_ITERATIONS: u32 = 10_000;

#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("ChaCha20-Poly1305 AEAD decryption or authentication tag mismatch")]
    AeadError,

    #[error("Invalid key or nonce length")]
    InvalidParameters,
}

/// Derive a 256-bit (32-byte) key from a passphrase and salt using iterative SHA-256 KDF.
pub fn derive_key(passphrase: &str, salt: &[u8; SALT_LEN], iterations: u32) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"MYTHOS_DR_KEY_DERIVATION_V1");
    hasher.update(passphrase.as_bytes());
    hasher.update(salt);
    let mut round_key = hasher.finalize();

    // Key stretching rounds
    for i in 1..iterations {
        let mut h = Sha256::new();
        h.update(round_key);
        h.update(i.to_le_bytes());
        h.update(passphrase.as_bytes());
        round_key = h.finalize();
    }

    round_key.into()
}

/// Generate cryptographically secure random bytes for salt and nonce.
pub fn generate_random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes
}

/// Encrypt plaintext using ChaCha20-Poly1305 AEAD with key derived from passphrase.
/// Returns (salt, nonce, ciphertext_with_poly1305_tag).
#[allow(clippy::type_complexity)]
pub fn encrypt_payload(
    plaintext: &[u8],
    passphrase: &str,
) -> Result<([u8; SALT_LEN], [u8; NONCE_LEN], Vec<u8>), CryptoError> {
    let salt = generate_random_bytes::<SALT_LEN>();
    let nonce_bytes = generate_random_bytes::<NONCE_LEN>();

    let key_bytes = derive_key(passphrase, &salt, KDF_ITERATIONS);
    let key = Key::from_slice(&key_bytes);
    let cipher = ChaCha20Poly1305::new(key);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|_| CryptoError::AeadError)?;

    Ok((salt, nonce_bytes, ciphertext))
}

/// Decrypt ciphertext using ChaCha20-Poly1305 AEAD with key derived from passphrase and salt.
pub fn decrypt_payload(
    ciphertext: &[u8],
    passphrase: &str,
    salt: &[u8; SALT_LEN],
    nonce_bytes: &[u8; NONCE_LEN],
) -> Result<Vec<u8>, CryptoError> {
    let key_bytes = derive_key(passphrase, salt, KDF_ITERATIONS);
    let key = Key::from_slice(&key_bytes);
    let cipher = ChaCha20Poly1305::new(key);
    let nonce = Nonce::from_slice(nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| CryptoError::AeadError)
}
