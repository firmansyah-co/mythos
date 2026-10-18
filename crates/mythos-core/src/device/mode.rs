//! Per-Device Operational Mode Toggle & Zero-Silent-Fallback Guard.
//!
//! Implements strict execution separation between physical hardware fieldbus
//! communications and manual simulation testing, enforcing SYS-REQ-096.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Operational error raised when execution invariants are violated.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DeviceModeError {
    #[error("Physical communication failed for device '{device_id}': {reason}. Zero-silent-fallback enforced.")]
    PhysicalCommunicationFailure { device_id: String, reason: String },

    #[error(
        "Simulation value injection rejected: device '{0}' is operating in PhysicalHardware mode"
    )]
    SimulationInjectionDisallowed(String),

    #[error("Unauthorized mode transition: {0}")]
    UnauthorizedTransition(String),
}

/// Operational execution mode for an industrial device (SYS-REQ-096).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DeviceExecutionMode {
    /// Normal production state: reads and writes strictly bind to physical field hardware.
    /// Hardware timeouts or communication losses MUST immediately raise an alarm and
    /// SHALL NEVER fall back to simulated synthetic values automatically.
    #[default]
    PhysicalHardware,

    /// Engineering / commissioning simulation state: manual slider inputs or mock providers
    /// inject synthetic tag values. Displays amber `[🧪 SIMULATION ACTIVE]` indicator in UI.
    ManualSimulation,
}

/// Append-only audit trail event recorded whenever operational mode changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimulationAuditEvent {
    pub event_id: u64,
    pub timestamp_ms: u64,
    pub device_id: String,
    pub previous_mode: DeviceExecutionMode,
    pub new_mode: DeviceExecutionMode,
    pub operator: String,
    pub reason: String,
}

static AUDIT_EVENT_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Per-device operational state controller enforcing zero-silent-fallback guards.
#[derive(Debug, Clone)]
pub struct DeviceModeController {
    device_id: String,
    mode: DeviceExecutionMode,
    audit_trail: Vec<SimulationAuditEvent>,
    mock_values: HashMap<String, f64>,
}

impl DeviceModeController {
    /// Initialize a new device controller in production `PhysicalHardware` mode.
    pub fn new(device_id: impl Into<String>) -> Self {
        Self {
            device_id: device_id.into(),
            mode: DeviceExecutionMode::PhysicalHardware,
            audit_trail: Vec::new(),
            mock_values: HashMap::new(),
        }
    }

    /// Current operational mode.
    pub fn mode(&self) -> DeviceExecutionMode {
        self.mode
    }

    /// Returns true if operating in `ManualSimulation` mode.
    pub fn is_simulation(&self) -> bool {
        self.mode == DeviceExecutionMode::ManualSimulation
    }

    /// Explicitly transition operational mode with mandatory operator credentials and audit trail.
    ///
    /// Changing to or from `ManualSimulation` requires an explicit operator identifier and reason.
    pub fn set_mode(
        &mut self,
        target_mode: DeviceExecutionMode,
        operator: impl Into<String>,
        reason: impl Into<String>,
    ) -> &SimulationAuditEvent {
        let prev = self.mode;
        self.mode = target_mode;

        let event = SimulationAuditEvent {
            event_id: AUDIT_EVENT_COUNTER.fetch_add(1, Ordering::SeqCst),
            timestamp_ms: current_timestamp_ms(),
            device_id: self.device_id.clone(),
            previous_mode: prev,
            new_mode: target_mode,
            operator: operator.into(),
            reason: reason.into(),
        };

        self.audit_trail.push(event);
        self.audit_trail.last().unwrap()
    }

    /// Retrieve the append-only audit trail for this device.
    pub fn audit_trail(&self) -> &[SimulationAuditEvent] {
        &self.audit_trail
    }

    /// Handle a physical communication failure according to SYS-REQ-096.
    ///
    /// ZERO-SILENT-FALLBACK MANDATE:
    /// In `PhysicalHardware` mode, a communication failure MUST return an error.
    /// It MUST NEVER silently switch to simulation or synthesize mock data.
    pub fn handle_comm_failure(&self, reason: impl Into<String>) -> Result<(), DeviceModeError> {
        match self.mode {
            DeviceExecutionMode::PhysicalHardware => {
                Err(DeviceModeError::PhysicalCommunicationFailure {
                    device_id: self.device_id.clone(),
                    reason: reason.into(),
                })
            }
            DeviceExecutionMode::ManualSimulation => {
                // In simulation mode, physical comm loss is expected/ignored
                Ok(())
            }
        }
    }

    /// Inject a simulated value for a tag (e.g. from an HMI Quick Input Slider).
    ///
    /// Rejected with `SimulationInjectionDisallowed` if currently in `PhysicalHardware` mode.
    pub fn inject_mock_value(
        &mut self,
        tag_suffix: impl Into<String>,
        value: f64,
    ) -> Result<(), DeviceModeError> {
        if self.mode != DeviceExecutionMode::ManualSimulation {
            return Err(DeviceModeError::SimulationInjectionDisallowed(
                self.device_id.clone(),
            ));
        }
        self.mock_values.insert(tag_suffix.into(), value);
        Ok(())
    }

    /// Retrieve a simulated tag value if available.
    pub fn get_mock_value(&self, tag_suffix: &str) -> Option<f64> {
        if self.mode == DeviceExecutionMode::ManualSimulation {
            self.mock_values.get(tag_suffix).copied()
        } else {
            None
        }
    }

    /// Clear all injected mock values.
    pub fn clear_mock_values(&mut self) {
        self.mock_values.clear();
    }
}

fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
