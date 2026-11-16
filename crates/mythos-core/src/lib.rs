//! # MythOS Core
//!
//! The HTTP/SSE API gateway, the field I/O runtime that feeds the tag bus, controller project
//! storage, the tag catalog, the device catalog parser, the data-type registry, accounts, the
//! module host and the collaboration room. The HMI is a service module (`plugins/hmi`, ADR-011).

pub mod api;
pub mod auth;
pub mod catalog;
pub mod collab;
pub mod datatype;
pub mod io;
pub mod mcp;
pub mod modules;
pub mod node;
pub mod pairing;
/// Tag URIs, shared with modules through the module protocol crate.
pub use mythos_module::namespace;
pub mod pal;
pub mod sdui;
pub mod server;
pub mod services;
pub mod storage;
pub mod tag;
pub mod tls;
pub mod updates;

pub use api::{create_router, ApiState, HealthResponse, TelemetryBatch, DEFAULT_INDEX_HTML};
pub use catalog::{
    AccessType, CatalogError, CatalogFingerprintEngine, DataType, DeviceCatalog, DeviceCategory,
    DeviceManifest, DeviceMetadata, DiscoveryConfig, FingerprintMatch, ModbusRtuConfig,
    PhysicalInterface, PinoutEntry, ProtocolConfig, RegisterDefinition, ScalingConfig,
    SupplyVoltage, UiConfig,
};
pub use collab::{handle_collab_ws, CollabRoom, RoomManager};
pub use namespace::{TagDomain, TagNamespaceError, TagUri};
pub use pal::*;
pub use sdui::*;
pub use storage::{
    default_projects_dir, ControllerStorage, ProjectConfig, ProjectWal, ProjectWalEntry,
    StorageError, TaskConfig, WalError,
};
pub use tag::{AccessMode, RetentionMode, TagCatalog, TagMetadata};
