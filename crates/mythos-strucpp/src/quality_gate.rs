//! Quality code alarm propagation and simulation gating (SYS-REQ-096 / TSK-108).

use crate::error::StrucppError;
use mythos_pal::DefaultShm;
use mythos_shm::slot::QualityCode;
use mythos_shm::ShmWriterHandle;
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

/// Where a latched quality landed, and therefore who can see it.
///
/// A latch that is recorded but not published is the defect this type exists to make
/// impossible to express silently: `ProcessLocalOnly` is a value a caller has to handle, not a
/// condition it can fail to notice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QualityVisibility {
    /// Written into the tag's slot in shared memory and published, so any other process reading
    /// the TagBus sees this quality.
    ShmPublished { slot_id: u32 },
    /// Held only in this process's latch map. Nothing outside this process can observe it.
    ProcessLocalOnly { reason: String },
}

impl QualityVisibility {
    /// True only when another process can observe the latched quality.
    pub fn is_cross_process(&self) -> bool {
        matches!(self, Self::ShmPublished { .. })
    }
}

/// The outcome of recording a quality event: the code that was latched, and its visibility.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityLatch {
    pub quality: QualityCode,
    pub visibility: QualityVisibility,
}

/// Shared-memory publication path for latched qualities.
///
/// Holds a writer handle plus the URI-to-slot map that turns a tag's namespace name into the slot
/// the TagBus stores it in. The map is supplied explicitly rather than read from a catalog, so the
/// gate cannot silently publish against a slot nobody reads.
pub struct ShmQualityPublisher {
    writer: parking_lot::Mutex<ShmWriterHandle<DefaultShm>>,
    uri_to_slot: HashMap<String, u32>,
}

impl ShmQualityPublisher {
    /// Attach a writer handle and the bindings the gate may publish against.
    pub fn new(writer: ShmWriterHandle<DefaultShm>, uri_to_slot: HashMap<String, u32>) -> Self {
        Self {
            writer: parking_lot::Mutex::new(writer),
            uri_to_slot,
        }
    }

    /// The slot a URI is bound to, if any.
    pub fn slot_for(&self, tag_uri: &str) -> Option<u32> {
        self.uri_to_slot.get(tag_uri).copied()
    }
}

/// Alarm and simulation gating supervisor.
pub struct QualityGate {
    mode: parking_lot::RwLock<DeviceExecutionMode>,
    latched_alarms: parking_lot::RwLock<HashMap<String, QualityCode>>,
    simulation_flag: Arc<AtomicBool>,
    /// Absent until the gate is given a shared-memory path. While absent, every latch is
    /// process-local and says so in its [`QualityVisibility`].
    publisher: Option<ShmQualityPublisher>,
}

impl QualityGate {
    pub fn new(initial_mode: DeviceExecutionMode) -> Self {
        let is_sim = initial_mode == DeviceExecutionMode::ManualSimulation;
        Self {
            mode: parking_lot::RwLock::new(initial_mode),
            latched_alarms: parking_lot::RwLock::new(HashMap::new()),
            simulation_flag: Arc::new(AtomicBool::new(is_sim)),
            publisher: None,
        }
    }

    /// Give the gate a shared-memory publication path.
    ///
    /// Without this, `record_comm_loss` can only latch inside this process and every result is
    /// `ProcessLocalOnly`. With it, a latch for a bound tag is written to `TagSlot::quality` and
    /// published, so a reader in another process sees it.
    pub fn with_shm_publication(mut self, publisher: ShmQualityPublisher) -> Self {
        self.publisher = Some(publisher);
        self
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
    ///
    /// The latch is written into the tag's `TagSlot.quality` in shared memory and published when
    /// the gate has a publication path and the tag is bound to a slot; otherwise it stays in this
    /// process. The returned [`QualityLatch`] always says which of the two happened, so a caller
    /// cannot record a field failure that no other process can see without being told.
    pub fn record_comm_loss(&self, tag_uri: &str) -> QualityLatch {
        self.latch(
            tag_uri,
            QualityCode::BAD_COMM_FAILURE,
            "Physical communication loss",
        )
    }

    /// Clear latched alarm for a recovered tag, publishing the recovery when possible.
    ///
    /// Clearing is published for the same reason latching is: a recovery that only this process
    /// knows about leaves every reader on the bad quality forever.
    pub fn clear_comm_loss(&self, tag_uri: &str) -> QualityLatch {
        let mut alarms = self.latched_alarms.write();
        let had = alarms.remove(tag_uri).is_some();
        drop(alarms);

        let visibility = self.publish(tag_uri, QualityCode::GOOD);
        if had {
            info!(
                "Communication recovered for {}. Cleared alarm latch ({}).",
                tag_uri,
                describe_visibility(&visibility)
            );
        }
        QualityLatch {
            quality: QualityCode::GOOD,
            visibility,
        }
    }

    /// Latch `quality` for a tag and publish it to shared memory when possible.
    fn latch(&self, tag_uri: &str, quality: QualityCode, what: &str) -> QualityLatch {
        {
            let mut alarms = self.latched_alarms.write();
            alarms.insert(tag_uri.to_string(), quality);
        }
        let visibility = self.publish(tag_uri, quality);

        if visibility.is_cross_process() {
            warn!(
                "{what} for {tag_uri}: latched {:#06x} ({}) and published to shared memory ({}).",
                quality.code(),
                quality.as_str(),
                describe_visibility(&visibility)
            );
        } else {
            // Louder than the success path on purpose: this is a real field failure that the HMI
            // and every other consumer will not see. Silence here is the defect.
            warn!(
                "{what} for {tag_uri}: latched {:#06x} ({}) IN THIS PROCESS ONLY ({}). \
                 No other consumer can observe it.",
                quality.code(),
                quality.as_str(),
                describe_visibility(&visibility)
            );
        }

        QualityLatch {
            quality,
            visibility,
        }
    }

    /// Write a quality word to the tag's slot and publish it, or explain why it could not be.
    fn publish(&self, tag_uri: &str, quality: QualityCode) -> QualityVisibility {
        let Some(publisher) = &self.publisher else {
            return QualityVisibility::ProcessLocalOnly {
                reason: "no shared-memory publication path was configured on this QualityGate"
                    .to_string(),
            };
        };

        let Some(slot_id) = publisher.slot_for(tag_uri) else {
            return QualityVisibility::ProcessLocalOnly {
                reason: format!("tag '{tag_uri}' is not bound to a TagBus slot"),
            };
        };

        let mut writer = publisher.writer.lock();
        match writer.set_quality_and_publish(slot_id as usize, quality) {
            Ok(()) => QualityVisibility::ShmPublished { slot_id },
            Err(e) => QualityVisibility::ProcessLocalOnly {
                reason: format!("shared-memory write for slot {slot_id} failed: {e}"),
            },
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

/// Human-readable form of a visibility outcome for logs.
fn describe_visibility(visibility: &QualityVisibility) -> String {
    match visibility {
        QualityVisibility::ShmPublished { slot_id } => format!("slot {slot_id}"),
        QualityVisibility::ProcessLocalOnly { reason } => format!("not published: {reason}"),
    }
}
