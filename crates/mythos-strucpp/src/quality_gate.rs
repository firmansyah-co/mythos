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

/// What releasing a latch did — or refused to do.
///
/// A clear that finds no latch must not vouch for the tag. `clear_comm_loss` used to publish GOOD
/// unconditionally, so a recovery path firing for a tag this process never saw fail would erase a
/// real `BAD_SENSOR_FAILURE` set by another path, cross-process, and replace it with GOOD. That is
/// the same defect as the one this gate was fixed to stop causing: a true BAD destroyed rather
/// than an invented device created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClearOutcome {
    /// A latch existed and was released.
    ///
    /// `restored` is the quality published back to the slot — the quality the slot carried *before*
    /// this gate latched, so another path's failure is restored rather than erased. It is `None`
    /// when that prior quality was never recorded (the gate had no readable slot at latch time),
    /// in which case the slot is left exactly as the latch left it and the `visibility` says why
    /// nothing was written.
    Cleared {
        restored: Option<QualityCode>,
        visibility: QualityVisibility,
    },
    /// There was no latch for this tag. The slot was NOT touched.
    NothingToClear,
}

/// A latch plus the quality that preceded it, so releasing the latch can restore rather than erase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LatchedQuality {
    code: QualityCode,
    /// `None` when the slot's quality could not be read at latch time (no publisher, or the tag was
    /// not bound). A clear can then only leave the slot alone, never guess.
    prior: Option<QualityCode>,
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
    latched_alarms: parking_lot::RwLock<HashMap<String, LatchedQuality>>,
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

    /// Release the latch for a recovered tag, restoring the quality the slot carried before it.
    ///
    /// Clearing is published for the same reason latching is: a recovery that only this process
    /// knows about leaves every reader on the bad quality forever.
    ///
    /// Two things this deliberately does **not** do:
    ///
    /// - It does not publish GOOD when there was no latch to release. A recovery path firing for a
    ///   tag this process never saw fail would erase a real `BAD_SENSOR_FAILURE` — set by another
    ///   path, visible to another process — and replace it with GOOD. That is a true BAD destroyed
    ///   rather than an invented device created: the same class of defect, one turn worse. With no
    ///   latch, the slot is left untouched and [`ClearOutcome::NothingToClear`] says so.
    /// - It does not publish GOOD when the slot carried something else before the comm loss. The
    ///   latch records the quality it overwrote, and that is what is restored, so a sensor failure
    ///   that was live underneath the comm failure comes back when the link does.
    pub fn clear_comm_loss(&self, tag_uri: &str) -> ClearOutcome {
        let removed = {
            let mut alarms = self.latched_alarms.write();
            alarms.remove(tag_uri)
        };

        let Some(latched) = removed else {
            // Deliberately no publish here. See this method's doc comment.
            info!(
                "Communication recovery reported for {tag_uri} with no latch to clear. \
                 Slot left untouched (nothing was cleared)."
            );
            return ClearOutcome::NothingToClear;
        };

        let Some(prior) = latched.prior else {
            // The slot's quality was not readable when the latch was taken, so there is no honest
            // value to restore. Publishing GOOD would be a claim we cannot support.
            info!(
                "Communication recovered for {tag_uri}. The quality preceding the latch was never \
                 recorded, so the slot is left as the latch left it."
            );
            return ClearOutcome::Cleared {
                restored: None,
                visibility: QualityVisibility::ProcessLocalOnly {
                    reason: "the quality preceding the latch was not recorded, so nothing was \
                             written back to the slot"
                        .to_string(),
                },
            };
        };

        // Compare-and-restore: undo *our own* overwrite, and only that.
        //
        // Writing `prior` unconditionally would revert a quality another path published while the
        // latch was held. That is reachable, not hypothetical: `TagSynchronizer::sync_outputs`
        // rewrites slot quality on every scan. The replay that found this:
        //
        //   1) code=68 (UNCERTAIN_SUBSTITUTE) value=42.5
        //   2) code=24 (BAD_COMM_FAILURE)     value=42.5   <- the latch
        //   3) code=192 (GOOD)                value=77.25  <- another path's real measurement
        //   4) code=68 (UNCERTAIN_SUBSTITUTE) value=77.25  <- unconditional restore
        //
        // Step 4 labels a measured value as a substitute: the inverse of the lie this slice
        // removes. So the restore fires only while the slot still carries the word this latch
        // wrote. If anything else owns the quality now, the gate has nothing to undo and leaves it.
        match self.peek_slot_quality(tag_uri) {
            Some(current) if current == latched.code => {
                let visibility = self.publish(tag_uri, prior);
                info!(
                    "Communication recovered for {tag_uri}. Restored {:#06x} ({}) over this gate's \
                     own {:#06x} ({}) ({}).",
                    prior.code(),
                    prior.as_str(),
                    latched.code.code(),
                    latched.code.as_str(),
                    describe_visibility(&visibility)
                );
                ClearOutcome::Cleared {
                    restored: Some(prior),
                    visibility,
                }
            }
            Some(current) => {
                // Someone else has taken the slot's quality since we latched. Their value is
                // fresher than anything we recorded, so ours is stale by definition.
                warn!(
                    "Communication recovered for {tag_uri}, but the slot now carries {:#06x} ({}) \
                     rather than this gate's {:#06x} ({}). Another path owns the quality; the slot \
                     is left untouched.",
                    current.code(),
                    current.as_str(),
                    latched.code.code(),
                    latched.code.as_str()
                );
                ClearOutcome::Cleared {
                    restored: None,
                    visibility: QualityVisibility::ProcessLocalOnly {
                        reason: format!(
                            "the slot carries {:#06x} ({}) rather than the {:#06x} ({}) this gate \
                             latched, so another path owns the quality and nothing was written back",
                            current.code(),
                            current.as_str(),
                            latched.code.code(),
                            latched.code.as_str()
                        ),
                    },
                }
            }
            None => {
                // Cannot read the slot, so cannot prove the word is still ours. Leave it.
                warn!(
                    "Communication recovered for {tag_uri}, but the slot's current quality could \
                     not be read, so this gate cannot tell whether it still owns the word. Slot \
                     left untouched."
                );
                ClearOutcome::Cleared {
                    restored: None,
                    visibility: QualityVisibility::ProcessLocalOnly {
                        reason: "the slot's current quality could not be read, so this gate could \
                                 not confirm it still owned the word and wrote nothing back"
                            .to_string(),
                    },
                }
            }
        }
    }

    /// Latch `quality` for a tag and publish it to shared memory when possible.
    fn latch(&self, tag_uri: &str, quality: QualityCode, what: &str) -> QualityLatch {
        {
            let mut alarms = self.latched_alarms.write();
            let prior = match alarms.get(tag_uri) {
                // Already latched: keep the quality that was there *before the first* latch, so a
                // repeated comm loss does not overwrite the thing a clear must restore.
                Some(existing) => existing.prior,
                None => self.peek_slot_quality(tag_uri),
            };
            alarms.insert(
                tag_uri.to_string(),
                LatchedQuality {
                    code: quality,
                    prior,
                },
            );
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
        alarms.get(tag_uri).map(|l| l.code)
    }

    /// The quality the tag's slot carries right now, if this gate can read it.
    ///
    /// `None` means "not readable" (no publication path, tag unbound, or the read failed) — never a
    /// guess. A caller that gets `None` has no honest quality to restore later, and says so rather
    /// than substituting GOOD.
    fn peek_slot_quality(&self, tag_uri: &str) -> Option<QualityCode> {
        let publisher = self.publisher.as_ref()?;
        let slot_id = publisher.slot_for(tag_uri)?;
        let writer = publisher.writer.lock();
        match writer.peek_quality(slot_id as usize) {
            Ok(q) => Some(q),
            Err(e) => {
                warn!(
                    "Could not read slot {slot_id} quality for {tag_uri} before latching ({e}); \
                     a later clear will leave the slot untouched rather than guess."
                );
                None
            }
        }
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
