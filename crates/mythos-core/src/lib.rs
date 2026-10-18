//! # MythOS Core Microkernel
//!
//! Kernel supervisor, actor coordinator, tag namespace engine, NVRAM WAL persistence,
//! microkernel plugin host, Axum REST/SSE API Gateway, and target controller storage.

pub mod api;
pub mod namespace;
pub mod nvram;
pub mod pal;
pub mod plugin;
pub mod storage;
pub mod tag;

pub use api::{create_router, ApiState, HealthResponse, TelemetryBatch};
pub use namespace::{TagDomain, TagNamespaceError, TagUri};
pub use nvram::{NvramError, NvramWalEngine, WalRecordHeader, DEFAULT_NVRAM_PATH, WAL_MAGIC};
pub use pal::*;
pub use plugin::PluginHost;
pub use storage::{
    ControllerStorage, ProjectConfig, ProjectWal, ProjectWalEntry, StorageError, TaskConfig,
    WalError, DEFAULT_PROJECTS_DIR,
};
pub use tag::{AccessMode, RetentionMode, TagCatalog, TagMetadata};
