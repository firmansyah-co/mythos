//! Discovery Engine Trait Definitions & Common Models.
//!
//! Conforms to SYS-REQ-074 and ADR-004.

use crate::catalog::DeviceCatalog;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

/// Discovery runtime errors.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DiscoveryError {
    #[error("Discovery timed out after {0:?}")]
    Timeout(Duration),

    #[error("Discovery cancelled by operator or token")]
    Cancelled,

    #[error("Interface error on '{0}': {1}")]
    InterfaceError(String, String),

    #[error("Protocol error during discovery: {0}")]
    ProtocolError(String),
}

/// Discovery scanning modes (SYS-REQ-074).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryMode {
    /// Catalog-assisted fingerprinting using known device profile signatures.
    GuidedDiscovery,
    /// Progressive heuristic sweep across unknown buses.
    BlindBusSweep,
}

/// Execution timing bounds for deterministic fieldbus scans.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryTimeoutBounds {
    /// Single fast probe timeout (SYS-REQ-074: <= 1.0s).
    pub fast_probe_timeout: Duration,
    /// Total guided catalog sweep timeout (SYS-REQ-074: <= 4.0s).
    pub total_scan_timeout: Duration,
}

impl Default for DiscoveryTimeoutBounds {
    fn default() -> Self {
        Self {
            fast_probe_timeout: Duration::from_millis(1000),
            total_scan_timeout: Duration::from_millis(4000),
        }
    }
}

/// Execution context for a discovery run.
#[derive(Clone)]
pub struct DiscoveryContext {
    pub mode: DiscoveryMode,
    pub target_interface: String,
    pub timeout_bounds: DiscoveryTimeoutBounds,
    pub cancel_token: CancellationToken,
    pub catalog: Option<Arc<DeviceCatalog>>,
}

impl DiscoveryContext {
    pub fn new(target_interface: impl Into<String>, mode: DiscoveryMode) -> Self {
        Self {
            mode,
            target_interface: target_interface.into(),
            timeout_bounds: DiscoveryTimeoutBounds::default(),
            cancel_token: CancellationToken::new(),
            catalog: None,
        }
    }

    pub fn with_catalog(mut self, catalog: Arc<DeviceCatalog>) -> Self {
        self.catalog = Some(catalog);
        self
    }

    pub fn with_timeouts(mut self, fast_probe: Duration, total_scan: Duration) -> Self {
        self.timeout_bounds = DiscoveryTimeoutBounds {
            fast_probe_timeout: fast_probe,
            total_scan_timeout: total_scan,
        };
        self
    }

    pub fn with_cancellation_token(mut self, token: CancellationToken) -> Self {
        self.cancel_token = token;
        self
    }
}

/// Discovered device record produced by a protocol discoverer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveredDevice {
    pub device_id: String,
    pub protocol: String,
    #[serde(default)]
    pub station_name: Option<String>,
    #[serde(default)]
    pub mac_address: Option<String>,
    #[serde(default)]
    pub ip_address: Option<String>,
    #[serde(default)]
    pub modbus_slave_id: Option<u8>,
    #[serde(default)]
    pub fingerprint_model: Option<String>,
    #[serde(default)]
    pub vendor: Option<String>,
    pub confidence: f32,
    #[serde(default)]
    pub properties: HashMap<String, String>,
}

/// Asynchronous universal protocol discoverer trait.
#[async_trait]
pub trait ProtocolDiscoverer: Send + Sync {
    /// Unique identifier of the protocol (e.g. "modbus-rtu", "profinet-dcp", "mdns").
    fn protocol_id(&self) -> &str;

    /// Execute discovery probe on the configured target interface.
    async fn discover(
        &self,
        ctx: &DiscoveryContext,
    ) -> Result<Vec<DiscoveredDevice>, DiscoveryError>;
}
