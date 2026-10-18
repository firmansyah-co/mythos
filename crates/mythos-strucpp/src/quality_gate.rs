//! Quality code alarm propagation and simulation gating (SYS-REQ-096 / TSK-108).

use crate::error::StrucppError;
use mythos_shm::slot::QualityCode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{info, warn};

/// Explicit per-device operational execution mode (SYS-REQ-096).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceExecutionMode {
    /// Physical production hardware: direct field I/O, manual forcing locked.
    PhysicalHardware,
    /// Manual bench simulation: manual slider injection and variable forcing unlocked.
    ManualSimulation,
}

/// Alarm and simulation gating supervisor.
pub struct QualityGate {
    mode: parking_lot::RwLock<DeviceExecutionMode>,
    latched_alarms: parking_lot::RwLock<HashMap<String, QualityCode>>,
    simulation_flag: Arc<AtomicBool>,
}

impl QualityGate {
    pub fn new(initial_mode: DeviceExecutionMode) -> Self {
        let is_sim = initial_mode == DeviceExecutionMode::ManualSimulation;
        Self {
            mode: parking_lot::RwLock::new(initial_mode),
            latched_alarms: parking_lot::RwLock::new(HashMap::new()),
            simulation_flag: Arc::new(AtomicBool::new(is_sim)),
        }
    }

    /// Read active execution mode.
    pub fn current_mode(&self) -> DeviceExecutionMode {
        *self.mode.read()
    }

    /// Set execution mode. Toggling to `ManualSimulation` unlocks forcing.
    pub fn set_mode(&self, new_mode: DeviceExecutionMode) {
        let mut mode = self.mode.write();
        *mode = new_mode;
        let is_sim = new_mode == DeviceExecutionMode::ManualSimulation;
        self.simulation_flag.store(is_sim, Ordering::Release);
        info!("QualityGate device mode set to: {:?}", new_mode);
    }

    /// Check if simulation flag is set.
    pub fn is_simulation(&self) -> bool {
        self.simulation_flag.load(Ordering::Acquire)
    }

    /// Record a physical field communication disconnect event.
    /// In PhysicalHardware mode, immediately latches `BAD_COMM_FAILURE` (0x0018)
    /// without synthetic fallback (SYS-REQ-096).
    pub fn record_comm_loss(&self, tag_uri: &str) -> QualityCode {
        let mut alarms = self.latched_alarms.write();
        let code = QualityCode::BAD_COMM_FAILURE;
        alarms.insert(tag_uri.to_string(), code);
        warn!(
            "Physical communication loss detected for {}. Latched BAD_COMM_FAILURE (0x0018)",
            tag_uri
        );
        code
    }

    /// Clear latched alarm for a recovered tag.
    pub fn clear_comm_loss(&self, tag_uri: &str) {
        let mut alarms = self.latched_alarms.write();
        if alarms.remove(tag_uri).is_some() {
            info!("Communication recovered for {}. Cleared alarm latch.", tag_uri);
        }
    }

    /// Check if tag has an active latched quality alarm.
    pub fn get_latched_quality(&self, tag_uri: &str) -> Option<QualityCode> {
        let alarms = self.latched_alarms.read();
        alarms.get(tag_uri).copied()
    }

    /// Gate manual slider injection or forced value write.
    /// Strictly rejects forcing in PhysicalHardware mode.
    pub fn validate_forcing_permitted(&self, tag_uri: &str) -> Result<(), StrucppError> {
        let mode = *self.mode.read();
        match mode {
            DeviceExecutionMode::PhysicalHardware => {
                warn!(
                    "Forcing attempt rejected for tag '{}': Device is in PhysicalHardware mode",
                    tag_uri
                );
                Err(StrucppError::ManualForcingRejected {
                    uri: tag_uri.to_string(),
                })
            }
            DeviceExecutionMode::ManualSimulation => {
                info!(
                    "Forcing permitted for tag '{}': Device is in ManualSimulation mode",
                    tag_uri
                );
                Ok(())
            }
        }
    }
}
