//! # Disaster Recovery Package Format & Container Layout (`.mythos-pkg`)
//!
//! Defines the single-file binary container format with ChaCha20-Poly1305 AEAD header,
//! manifest serialization, and payload integrity verification.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::crypto::{NONCE_LEN, SALT_LEN};

/// Magic bytes identifying MythOS disaster recovery archive.
pub const PACKAGE_MAGIC: &[u8; 10] = b"MYTHOS_PKG";
/// Container layout specification version.
pub const PACKAGE_VERSION: u16 = 1;
/// Fixed header length in bytes: 10 (magic) + 2 (ver) + 32 (salt) + 12 (nonce) + 8 (len) = 64 bytes.
pub const HEADER_LEN: usize = 64;

#[derive(Error, Debug)]
pub enum PackageError {
    #[error("Invalid magic bytes: expected 'MYTHOS_PKG'")]
    InvalidMagic,

    #[error("Unsupported package version: {0} (supported: 1)")]
    UnsupportedVersion(u16),

    #[error("Unexpected end of file while reading package header or payload")]
    UnexpectedEof,

    #[error("Corrupted package payload: length mismatch (header: {expected}, actual: {actual})")]
    PayloadLengthMismatch { expected: usize, actual: usize },

    #[error("Content checksum verification failed (expected: {expected}, computed: {computed})")]
    ChecksumMismatch { expected: String, computed: String },

    #[error("Serialization / Deserialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("ZIP archive error: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Binary container header for `.mythos-pkg` files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageHeader {
    pub magic: [u8; 10],
    pub version: u16,
    pub salt: [u8; SALT_LEN],
    pub nonce: [u8; NONCE_LEN],
    pub payload_len: u64,
}

impl PackageHeader {
    pub fn new(salt: [u8; SALT_LEN], nonce: [u8; NONCE_LEN], payload_len: u64) -> Self {
        Self {
            magic: *PACKAGE_MAGIC,
            version: PACKAGE_VERSION,
            salt,
            nonce,
            payload_len,
        }
    }

    /// Serialize header to 64-byte array.
    pub fn to_bytes(&self) -> [u8; HEADER_LEN] {
        let mut buf = [0u8; HEADER_LEN];
        buf[0..10].copy_from_slice(&self.magic);
        buf[10..12].copy_from_slice(&self.version.to_le_bytes());
        buf[12..44].copy_from_slice(&self.salt);
        buf[44..56].copy_from_slice(&self.nonce);
        buf[56..64].copy_from_slice(&self.payload_len.to_le_bytes());
        buf
    }

    /// Parse header from 64-byte slice.
    pub fn from_bytes(slice: &[u8]) -> Result<Self, PackageError> {
        if slice.len() < HEADER_LEN {
            return Err(PackageError::UnexpectedEof);
        }

        let mut magic = [0u8; 10];
        magic.copy_from_slice(&slice[0..10]);
        if &magic != PACKAGE_MAGIC {
            return Err(PackageError::InvalidMagic);
        }

        let version = u16::from_le_bytes(slice[10..12].try_into().unwrap());
        if version != PACKAGE_VERSION {
            return Err(PackageError::UnsupportedVersion(version));
        }

        let mut salt = [0u8; SALT_LEN];
        salt.copy_from_slice(&slice[12..44]);

        let mut nonce = [0u8; NONCE_LEN];
        nonce.copy_from_slice(&slice[44..56]);

        let payload_len = u64::from_le_bytes(slice[56..64].try_into().unwrap());

        Ok(Self {
            magic,
            version,
            salt,
            nonce,
            payload_len,
        })
    }
}

/// Metadata manifest stored within the disaster recovery archive.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageManifest {
    pub project_id: String,
    pub project_name: String,
    pub project_version: String,
    pub created_at_utc: String,
    pub content_sha256: String,
    pub files: Vec<String>,
    pub recipe_count: usize,
    pub user_count: usize,
    pub screen_count: usize,
    pub logic_file_count: usize,
}

impl PackageManifest {
    pub fn compute_sha256(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }
}
