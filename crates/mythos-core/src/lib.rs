//! # MythOS Core Microkernel
//!
//! Kernel supervisor, actor coordinator, tag namespace engine, NVRAM WAL persistence,
//! microkernel plugin host, Axum REST/SSE API Gateway, target controller storage,
//! open hardware catalog parser, network topology model, and universal discovery engine.

pub mod alarm;
pub mod api;
pub mod catalog;
pub mod collab;
pub mod device;
pub mod discovery;
pub mod historian;
pub mod namespace;
pub mod nvram;
pub mod pal;
pub mod plugin;
pub mod recipe;
pub mod sdui;
pub mod storage;
pub mod tag;
pub mod topology;

pub use api::{create_router, ApiState, HealthResponse, TelemetryBatch, DEFAULT_INDEX_HTML};
pub use catalog::{
    AccessType, CatalogError, CatalogFingerprintEngine, DataType, DeviceCatalog, DeviceCategory,
    DeviceManifest, DeviceMetadata, DiscoveryConfig, FingerprintMatch, ModbusRtuConfig,
    PhysicalInterface, PinoutEntry, ProtocolConfig, RegisterDefinition, ScalingConfig,
    SupplyVoltage, UiConfig,
};
pub use collab::{handle_collab_ws, CollabRoom, RoomManager};
pub use device::{
    DeviceExecutionMode, DeviceModeController, DeviceModeError, SimulationAuditEvent,
};
pub use discovery::{
    DiscoveredDevice, DiscoveryContext, DiscoveryEngine, DiscoveryError, DiscoveryMode,
    DiscoveryTimeoutBounds, MockModbusTransport, ModbusProbeTransport, ModbusSweepDiscoverer,
    ProtocolDiscoverer,
};
pub use namespace::{TagDomain, TagNamespaceError, TagUri};
pub use nvram::{NvramError, NvramWalEngine, WalRecordHeader, DEFAULT_NVRAM_PATH, WAL_MAGIC};
pub use pal::*;
pub use plugin::{PluginHost, PluginPackageManifest, PluginPackager, VerifiedPluginPackage};
pub use sdui::*;
pub use storage::{
    ControllerStorage, ProjectConfig, ProjectWal, ProjectWalEntry, StorageError, TaskConfig,
    WalError, DEFAULT_PROJECTS_DIR,
};
pub use tag::{AccessMode, RetentionMode, TagCatalog, TagMetadata};
pub use topology::{
    EthernetBusTopology, IpBlock, NetworkTopology, NodeType, PortDefinition, TopologyEdge,
    TopologyError, TopologyNode, TopologyValidationError, ValidationSeverity,
};

pub use alarm::{
    get_monotonic_raw_ns, AlarmDefinition, AlarmEventTrigger, AlarmFilterTimer, AlarmRuntimeState,
    AlarmSeverity, AlarmState, AlarmStateTransitionEvent, AlarmTripType, ChatterDetector,
    DeadbandConfig, FirstOutCaptureResult, FirstOutGroupStatus, FirstOutManager,
};
pub use historian::{
    CsvExportRequest, CsvExporter, CsvExporterError, HistorianSample, ParquetHistorianError,
    ParquetHistorianStorage, ResampleStrategy, SwingingDoorCompressor, TelemetryRecord,
    TimeSeriesPoint,
};
pub use recipe::{
    BatchExecutionInstance, BatchRunStatus, MasterRecipe, PackMlState, ParameterConstraint,
    ParameterValue, RecipeBusRegisters, RecipeLifecycleState, RecipeStep, RecipeSyncCoordinator,
    RecipeSyncError, StepTransitionCondition,
};
