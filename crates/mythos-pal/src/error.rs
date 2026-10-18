use std::io;
use thiserror::Error;

/// Platform Abstraction Layer (PAL) Error Types.
#[derive(Error, Debug)]
pub enum PalError {
    #[error("Failed to create shared memory '{name}': {reason}")]
    ShmCreateFailed { name: String, reason: String },

    #[error("Failed to open shared memory '{name}': {reason}")]
    ShmOpenFailed { name: String, reason: String },

    #[error("Failed to map shared memory '{name}': {reason}")]
    ShmMapFailed { name: String, reason: String },

    #[error("Failed to truncate shared memory '{name}' to {size} bytes: {reason}")]
    ShmTruncateFailed {
        name: String,
        size: usize,
        reason: String,
    },

    #[error("Failed to unlink shared memory '{name}': {reason}")]
    ShmUnlinkFailed { name: String, reason: String },

    #[error("Failed to flush shared memory '{name}': {reason}")]
    ShmFlushFailed { name: String, reason: String },

    #[error("Invalid shared memory size requested: {requested} bytes ({reason})")]
    InvalidSize { requested: usize, reason: String },

    #[error("Permission denied for operation '{operation}': {reason}")]
    PermissionDenied { operation: String, reason: String },

    #[error("Realtime scheduling priority denied for level {priority_level}: {reason}")]
    RealtimePriorityDenied {
        priority_level: u32,
        reason: String,
    },

    #[error("Failed to pin thread to CPU core {core_id}: {reason}")]
    CorePinningFailed { core_id: usize, reason: String },

    #[error("Failed to load dynamic library '{path}': {reason}")]
    LibraryLoadFailed { path: String, reason: String },

    #[error("Failed to lookup symbol '{symbol}': {reason}")]
    SymbolLookupFailed { symbol: String, reason: String },

    #[error("Operation '{operation}' not supported on target platform '{platform}'")]
    UnsupportedPlatform {
        operation: String,
        platform: String,
    },

    #[error("Underlying I/O error: {0}")]
    Io(#[from] io::Error),
}
