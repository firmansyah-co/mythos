//! # MythOS Core Microkernel
//!
//! Kernel supervisor, actor coordinator, tag namespace engine, NVRAM WAL persistence,
//! and target controller storage.

pub mod namespace;
pub mod nvram;
pub mod pal;
pub mod storage;
pub mod tag;

pub use namespace::{TagDomain, TagNamespaceError, TagUri};
pub use nvram::{NvramError, NvramWalEngine, WalRecordHeader, DEFAULT_NVRAM_PATH, WAL_MAGIC};
pub use pal::*;
pub use storage::{
    ControllerStorage, ProjectConfig, ProjectWal, ProjectWalEntry, StorageError, TaskConfig,
    WalError, DEFAULT_PROJECTS_DIR,
};
pub use tag::{AccessMode, RetentionMode, TagCatalog, TagMetadata};
