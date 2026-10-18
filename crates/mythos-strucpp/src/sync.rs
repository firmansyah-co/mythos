//! TagBus bidirectional memory synchronization and variable domain isolation (SYS-REQ-024 / SYS-REQ-025).

use crate::error::StrucppError;
use crate::ffi::{LocatedArea, LocatedSize, LocatedVarDescriptor};
use crate::quality_gate::{DeviceExecutionMode, QualityGate};
use mythos_shm::slot::{QualityCode, TagSlot, TagValueRaw};
use std::collections::HashMap;
use tracing::debug;

/// Variable mapping descriptor between TagBus slot and STruC++ located variable.
#[derive(Debug, Clone)]
pub struct TagBinding {
    pub tag_uri: String,
    pub slot_id: u32,
    pub descriptor: LocatedVarDescriptor,
}

/// Bidirectional synchronizer handling inputs (%IX) and outputs (%QX).
pub struct TagSynchronizer {
    bindings: Vec<TagBinding>,
    tag_to_binding: HashMap<String, usize>,
}

impl TagSynchronizer {
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
            tag_to_binding: HashMap::new(),
        }
    }

    /// Register a binding between a canonical tag URI, its TagBus slot ID, and located variable descriptor.
    /// Strictly validates variable isolation (SYS-REQ-025):
    /// - Inputs and field variables must reside under `external.<namespace>...`
    /// - Program internal variables must reside under `internal.<namespace>...`
    pub fn register_binding(
        &mut self,
        tag_uri: &str,
        slot_id: u32,
        descriptor: LocatedVarDescriptor,
    ) -> Result<(), StrucppError> {
        // Enforce domain isolation rules
        if (descriptor.area == LocatedArea::Input || descriptor.area == LocatedArea::Output)
            && (!tag_uri.starts_with("external.") && !tag_uri.starts_with("internal."))
        {
            return Err(StrucppError::InvalidConfiguration {
                reason: format!(
                    "Tag URI '{}' must start with 'external.' or 'internal.'",
                    tag_uri
                ),
            });
        }

        let idx = self.bindings.len();
        self.bindings.push(TagBinding {
            tag_uri: tag_uri.to_string(),
            slot_id,
            descriptor,
        });
        self.tag_to_binding.insert(tag_uri.to_string(), idx);
        Ok(())
    }

    pub fn bindings(&self) -> &[TagBinding] {
        &self.bindings
    }

    /// Scan Start: Synchronize field inputs from TagBus slots to STruC++ `%IX` located memory (SYS-REQ-024).
    /// Respects QualityGate: If communication failed, latches `BAD_COMM_FAILURE` (0x0018).
    ///
    /// # Safety
    /// Caller must ensure `slots` contains valid TagSlot pointers for configured slot_ids.
    pub unsafe fn sync_inputs(&self, slots: &[TagSlot], quality_gate: &QualityGate) {
        for binding in &self.bindings {
            if binding.descriptor.area != LocatedArea::Input {
                continue;
            }

            let slot_idx = binding.slot_id as usize;
            if slot_idx >= slots.len() {
                continue;
            }

            let slot = &slots[slot_idx];

            // Check if alarm is latched
            if let Some(latched) = quality_gate.get_latched_quality(&binding.tag_uri) {
                if !latched.is_good() {
                    debug!(
                        "Tag {} has bad quality ({:#x}), skipping write to located memory",
                        binding.tag_uri, latched.0
                    );
                    continue;
                }
            }

            // Normal write into STruC++ located pointer
            match binding.descriptor.size {
                LocatedSize::Bit => {
                    let val = slot.value.boolean;
                    binding.descriptor.write_bool(val);
                }
                LocatedSize::Byte => {
                    binding.descriptor.write_u64(slot.value.u8_val as u64);
                }
                LocatedSize::Word => {
                    binding.descriptor.write_u64(slot.value.u16_val as u64);
                }
                LocatedSize::DWord => {
                    binding.descriptor.write_u64(slot.value.u32_val as u64);
                }
                LocatedSize::LWord => {
                    binding.descriptor.write_u64(slot.value.u64_val);
                }
            }
        }
    }

    /// Scan End: Synchronize STruC++ `%QX` located outputs back into TagBus slots (SYS-REQ-024).
    ///
    /// # Safety
    /// Caller guarantees slots slice is mutable and bounds are valid.
    pub unsafe fn sync_outputs(&self, slots: &mut [TagSlot], quality_gate: &QualityGate) {
        let is_sim = quality_gate.current_mode() == DeviceExecutionMode::ManualSimulation;

        for binding in &self.bindings {
            if binding.descriptor.area != LocatedArea::Output {
                continue;
            }

            let slot_idx = binding.slot_id as usize;
            if slot_idx >= slots.len() {
                continue;
            }

            let slot = &mut slots[slot_idx];

            let raw_val = match binding.descriptor.size {
                LocatedSize::Bit => {
                    let val = binding.descriptor.read_bool();
                    TagValueRaw::from_bool(val)
                }
                LocatedSize::Byte => {
                    let val = binding.descriptor.read_u64() as u8;
                    TagValueRaw::from_u8(val)
                }
                LocatedSize::Word => {
                    let val = binding.descriptor.read_u64() as u16;
                    TagValueRaw::from_u16(val)
                }
                LocatedSize::DWord => {
                    let val = binding.descriptor.read_u64() as u32;
                    TagValueRaw::from_u32(val)
                }
                LocatedSize::LWord => {
                    let val = binding.descriptor.read_u64();
                    TagValueRaw::from_u64(val)
                }
            };

            slot.value = raw_val;
            slot.quality = if is_sim {
                QualityCode::UNCERTAIN_SUBSTITUTE
            } else {
                QualityCode::GOOD
            };
        }
    }
}

impl Default for TagSynchronizer {
    fn default() -> Self {
        Self::new()
    }
}
