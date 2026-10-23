//! # Disaster Recovery Subsystem (`.mythos-pkg`)
//!
//! Provides single-click packing, ChaCha20-Poly1305 authenticated encryption,
//! and hot-restoration of complete MythOS IPC operational state in $< 25$ seconds.
//! Complies with TSK-506, SYS-REQ-068, SYS-REQ-034, and ADR-007.

pub mod container;
pub mod crypto;
pub mod engine;

pub use container::{
    PackageError, PackageHeader, PackageManifest, HEADER_LEN, PACKAGE_MAGIC, PACKAGE_VERSION,
};
pub use crypto::{decrypt_payload, encrypt_payload, CryptoError, NONCE_LEN, SALT_LEN};
pub use engine::{
    DisasterRecoveryBundle, RecoveryEngine, RecoveryError, RestoreReport, UserRecord,
};
