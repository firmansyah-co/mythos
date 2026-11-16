//! # MythOS Core
//!
//! The HTTP/SSE API gateway, the field I/O runtime that feeds the tag bus, controller project
//! storage, the tag catalog and namespace rules, the device catalog parser, HMI screen /
//! faceplate / data-type registries, the collaboration room, and the plugin package format.

pub mod api;
pub mod catalog;
pub mod collab;
pub mod datatype;
pub mod faceplate;
pub mod io;
pub mod mcp;
pub mod namespace;
pub mod pal;
pub mod plugin;
pub mod sdui;
pub mod server;
pub mod storage;
pub mod tag;

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
pub use plugin::{PluginPackageManifest, PluginPackager, VerifiedPluginPackage};
pub use sdui::*;
pub use storage::{
    default_projects_dir, ControllerStorage, ProjectConfig, ProjectWal, ProjectWalEntry,
    StorageError, TaskConfig, WalError,
};
pub use tag::{AccessMode, RetentionMode, TagCatalog, TagMetadata};
