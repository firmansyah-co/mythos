//! Error types for the MythOS Native STruC++ soft-logic runtime engine.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum StrucppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Platform Abstraction Layer error: {0}")]
    Pal(#[from] mythos_pal::error::PalError),

    #[error("Shared memory error: {0}")]
    Shm(#[from] mythos_shm::error::ShmError),

    #[error("Dynamic library loading error: {0}")]
    Library(#[from] libloading::Error),

    #[error("Compiler error during ST compilation: {reason}")]
    CompilationFailed { reason: String },

    #[error("C++ compilation error: {reason}")]
    CppCompilationFailed { reason: String },

    #[error("Symbol lookup failed for '{symbol}': {reason}")]
    SymbolNotFound { symbol: String, reason: String },

    #[error("Invalid located variable descriptor: area={area}, size={size}, byte_index={byte_index}, bit_index={bit_index}")]
    InvalidLocatedDescriptor {
        area: u8,
        size: u8,
        byte_index: u16,
        bit_index: u8,
    },

    #[error("Hot-reload timeout exceeded: {elapsed_ms}ms > {limit_ms}ms limit")]
    HotReloadTimeout { elapsed_ms: u64, limit_ms: u64 },

    #[error("Quality gate violation: tag '{uri}' cannot be forced in PhysicalHardware mode")]
    ManualForcingRejected { uri: String },

    #[error("Physical device disconnect latched: BAD_COMM_FAILURE on tag '{uri}'")]
    BadCommFailureLatched { uri: String },

    #[error("Invalid program configuration: {reason}")]
    InvalidConfiguration { reason: String },

    #[error("Lock acquisition failed: {0}")]
    LockPoisoned(String),
}
