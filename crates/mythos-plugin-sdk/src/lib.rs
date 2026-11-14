//! # MythOS Universal Plugin SDK
//!
//! Standardized trait definitions, capability guards, lifecycle hooks,
//! and protocol driver interfaces for first-party and third-party extensions.

pub use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;
use thiserror::Error;

/// Error types occurring during plugin and driver lifecycle management.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PluginError {
    #[error("Plugin initialization error: {0}")]
    InitializationFailed(String),

    #[error("Plugin start failed: {0}")]
    StartFailed(String),

    #[error("Plugin runtime failure: {0}")]
    RuntimeError(String),

    #[error("Configuration validation error: {0}")]
    InvalidConfiguration(String),

    #[error("Capability violation: {0}")]
    CapabilityViolation(String),

    #[error("Driver communication timeout: {0}")]
    Timeout(String),

    #[error("Tag not supported or invalid: {0}")]
    InvalidTag(String),

    #[error("Driver communication failure: {0}")]
    CommunicationError(String),
}

/// Category classification of a MythOS plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginType {
    /// Fieldbus or communication protocol driver (e.g. Modbus, MQTT, Profinet)
    ProtocolDriver,
    /// Real-time or batch process execution engine (e.g. Native STruC++, Alarms, Recipes)
    ProcessEngine,
    /// Hardware and network discovery provider
    DiscoveryScanner,
    /// Storage, historian or analytics provider
    StorageEngine,
    /// Model Context Protocol AI tool contributor
    AiProvider,
    /// General-purpose user extension
    Extension,
}

impl fmt::Display for PluginType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProtocolDriver => write!(f, "protocol-driver"),
            Self::ProcessEngine => write!(f, "process-engine"),
            Self::DiscoveryScanner => write!(f, "discovery-scanner"),
            Self::StorageEngine => write!(f, "storage-engine"),
            Self::AiProvider => write!(f, "ai-provider"),
            Self::Extension => write!(f, "extension"),
        }
    }
}

/// Operational health state reported by a plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HealthStatus {
    /// Fully healthy and operating nominally
    Healthy,
    /// Operational but in a degraded state (e.g. transient comm retries)
    Degraded,
    /// Unhealthy or blocked
    Unhealthy,
    /// Stopped or offline
    Stopped,
}

/// Operational health diagnostic report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginHealthReport {
    pub status: HealthStatus,
    pub message: String,
    pub timestamp_ms: u64,
}

impl PluginHealthReport {
    pub fn healthy(message: impl Into<String>) -> Self {
        Self {
            status: HealthStatus::Healthy,
            message: message.into(),
            timestamp_ms: current_time_ms(),
        }
    }

    pub fn degraded(message: impl Into<String>) -> Self {
        Self {
            status: HealthStatus::Degraded,
            message: message.into(),
            timestamp_ms: current_time_ms(),
        }
    }

    pub fn unhealthy(message: impl Into<String>) -> Self {
        Self {
            status: HealthStatus::Unhealthy,
            message: message.into(),
            timestamp_ms: current_time_ms(),
        }
    }

    pub fn stopped(message: impl Into<String>) -> Self {
        Self {
            status: HealthStatus::Stopped,
            message: message.into(),
            timestamp_ms: current_time_ms(),
        }
    }
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Metadata identifying a loaded plugin.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub plugin_type: PluginType,
    pub capabilities: Vec<String>,
}

/// Execution and environment context provided by the microkernel to the plugin.
#[derive(Debug, Clone)]
pub struct PluginContext {
    pub plugin_id: String,
    pub allowed_capabilities: Arc<HashSet<String>>,
    pub working_directory: String,
}

impl PluginContext {
    pub fn new(
        plugin_id: impl Into<String>,
        allowed_capabilities: HashSet<String>,
        working_directory: impl Into<String>,
    ) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            allowed_capabilities: Arc::new(allowed_capabilities),
            working_directory: working_directory.into(),
        }
    }

    /// Check whether a specific capability scope is permitted for this plugin.
    pub fn has_capability(&self, capability: &str) -> bool {
        self.allowed_capabilities.contains(capability)
    }

    /// Assert that a required capability is granted, returning an error if missing.
    pub fn check_capability(&self, capability: &str) -> Result<(), PluginError> {
        if self.has_capability(capability) {
            Ok(())
        } else {
            Err(PluginError::CapabilityViolation(format!(
                "Plugin '{}' lacks required capability '{}'",
                self.plugin_id, capability
            )))
        }
    }
}

/// Universal plugin lifecycle contract for all MythOS extensions.
#[async_trait]
pub trait MythosPlugin: Send + Sync {
    /// Retrieve manifest metadata describing this plugin.
    fn metadata(&self) -> PluginMetadata;

    /// Initialize plugin state and validate dependencies before starting.
    async fn init(&mut self, ctx: &PluginContext) -> Result<(), PluginError>;

    /// Apply runtime configuration and commence background tasks.
    async fn start(&mut self, ctx: &PluginContext, config: &JsonValue) -> Result<(), PluginError>;

    /// Optional cyclic tick hook invoked by the microkernel host scheduler.
    async fn tick(&mut self, ctx: &PluginContext, _delta_ms: u32) -> Result<(), PluginError> {
        let _ = ctx;
        Ok(())
    }

    /// Terminate execution tasks and release resources gracefully.
    async fn stop(&mut self, ctx: &PluginContext) -> Result<(), PluginError>;

    /// Check operational health and report diagnostic status.
    async fn health_check(&self) -> Result<PluginHealthReport, PluginError>;
}

/// Trait alias for backward compatibility.
pub trait Plugin: MythosPlugin {}
impl<T: MythosPlugin> Plugin for T {}

/// A typed process value read from a field device.
///
/// The variant is the type. A driver reports the value in the type the device holds it in; it
/// does not widen everything to a float, because a consumer that must branch on the type cannot
/// recover it afterwards.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SampleValue {
    Bool(bool),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    F32(f32),
    F64(f64),
}

/// Why a tag has no usable reading this cycle.
///
/// A driver that cannot read a tag says so with one of these. It never substitutes a number: a
/// zero, a last value or a default handed back as a reading is indistinguishable from a
/// measurement once it reaches an operator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SampleFault {
    /// The link to the device failed: no connection, a timeout, a malformed or mismatched frame.
    CommFailure(String),
    /// The device answered and reported that it cannot serve the request.
    DeviceFailure(String),
    /// The device rejected the request as invalid: the mapping names something it does not have.
    ConfigError(String),
}

impl fmt::Display for SampleFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CommFailure(m) => write!(f, "communication failure: {m}"),
            Self::DeviceFailure(m) => write!(f, "device failure: {m}"),
            Self::ConfigError(m) => write!(f, "configuration error: {m}"),
        }
    }
}

/// One tag's outcome for one poll cycle: a value, or the reason there is none.
#[derive(Debug, Clone, PartialEq)]
pub struct TagSample {
    pub uri: String,
    pub reading: Result<SampleValue, SampleFault>,
}

impl TagSample {
    pub fn value(uri: impl Into<String>, value: SampleValue) -> Self {
        Self {
            uri: uri.into(),
            reading: Ok(value),
        }
    }

    pub fn fault(uri: impl Into<String>, fault: SampleFault) -> Self {
        Self {
            uri: uri.into(),
            reading: Err(fault),
        }
    }
}

/// A tag a started driver will report on every poll cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagDeclaration {
    /// Canonical tag URI.
    pub uri: String,
    /// Where the tag comes from, in the driver's own terms (for example the register address).
    pub description: String,
}

/// Protocol Driver Service Provider Interface (SPI).
///
/// Every communication protocol driver (Modbus, MQTT, Profinet, etc.) implements this trait.
///
/// A driver never touches the tag bus. It returns samples and the host writes them, so a driver
/// cannot publish to a slot it does not own and the host decides the quality that reaches a
/// reader.
#[async_trait]
pub trait ProtocolDriver: MythosPlugin {
    /// Parse and validate the driver-specific JSON configuration schema.
    fn validate_config(&self, config: &JsonValue) -> Result<(), PluginError>;

    /// The tags this driver reports, known once it has been started with a configuration.
    ///
    /// The host registers exactly these. Empty before `start`.
    fn declared_tags(&self) -> Vec<TagDeclaration>;

    /// Execute a single bounded I/O polling cycle against configured target devices.
    ///
    /// Returns one [`TagSample`] for every declared tag: a value, or the fault that prevented a
    /// reading. `Err` means the cycle as a whole could not run; the host then treats every
    /// declared tag as a communication failure.
    async fn poll_cycle(&mut self, ctx: &PluginContext) -> Result<Vec<TagSample>, PluginError>;

    /// Write an output value to a field device target tag.
    async fn write_tag(
        &mut self,
        ctx: &PluginContext,
        tag_uri: &str,
        value: &JsonValue,
    ) -> Result<(), PluginError>;
}
