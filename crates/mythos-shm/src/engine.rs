//! Lock-free TagBus engine, writer, and reader handles built on PlatformShm.

use crate::buffer::TripleBufferState;
use crate::error::ShmError;
use crate::slot::{QualityCode, TagSlot, TagValueRaw};
use crate::superblock::{
    calculate_bank_offset, calculate_shm_size, ShmSuperblock, SUPERBLOCK_SIZE,
};
use mythos_pal::{DefaultShm, PlatformShm};

/// Master TagBus shared memory coordinator.
pub struct TagBusEngine<S: PlatformShm = DefaultShm> {
    shm: S,
    max_tags: usize,
    active_write_bank: usize,
}

impl<S: PlatformShm> TagBusEngine<S> {
    /// Create or attach to a shared memory segment with the specified name and tag capacity.
    pub fn open_or_create(name: &str, max_tags: usize) -> Result<Self, ShmError> {
        if max_tags == 0 {
            return Err(ShmError::ZeroCapacity);
        }
        let total_bytes = calculate_shm_size(max_tags);
        let shm = S::open_or_create(name, total_bytes)?;

        if shm.len() < total_bytes {
            return Err(ShmError::SegmentTooSmall {
                expected: total_bytes,
                actual: shm.len(),
            });
        }

        // Initialize superblock if uninitialized or validate existing
        let sb_ptr = shm.as_ptr() as *mut ShmSuperblock;
        unsafe {
            let sb_ref = &*sb_ptr;
            if sb_ref.magic != crate::superblock::SHM_MAGIC {
                // Initialize clean superblock
                let sb = ShmSuperblock::new(max_tags as u32);
                std::ptr::write(sb_ptr, sb);

                // Zero-initialize slot banks
                let slots_base = shm.as_ptr().add(SUPERBLOCK_SIZE) as *mut TagSlot;
                let total_slots = max_tags * 3;
                for i in 0..total_slots {
                    std::ptr::write(slots_base.add(i), TagSlot::default());
                }
            } else {
                sb_ref.validate().map_err(ShmError::SuperblockCorrupt)?;
            }
        }

        let initial_write = unsafe { (*sb_ptr).bank_state.current_write_bank() };

        Ok(Self {
            shm,
            max_tags,
            active_write_bank: initial_write,
        })
    }

    /// Access reference to the mapped superblock.
    #[inline(always)]
    pub fn superblock(&self) -> &ShmSuperblock {
        unsafe { &*(self.shm.as_ptr() as *const ShmSuperblock) }
    }

    /// Access atomic triple-buffer state descriptor.
    #[inline(always)]
    pub fn bank_state(&self) -> &TripleBufferState {
        &self.superblock().bank_state
    }

    /// Returns max tag capacity.
    #[inline(always)]
    pub fn max_tags(&self) -> usize {
        self.max_tags
    }

    /// Returns the currently active write bank index.
    #[inline(always)]
    pub fn current_write_bank(&self) -> usize {
        self.active_write_bank
    }

    /// Direct pointer to the base of bank `bank_index`.
    #[inline(always)]
    unsafe fn bank_ptr(&self, bank_index: usize) -> *mut TagSlot {
        let offset = calculate_bank_offset(self.max_tags, bank_index);
        self.shm.as_ptr().add(offset) as *mut TagSlot
    }

    /// Write an updated tag into the active write bank.
    #[inline(always)]
    pub fn write_tag(
        &mut self,
        slot_id: usize,
        type_code: u16,
        quality: QualityCode,
        value: TagValueRaw,
        timestamp_ns: u64,
        flags: u32,
    ) -> Result<(), ShmError> {
        if slot_id >= self.max_tags {
            return Err(ShmError::SlotOutOfBounds {
                index: slot_id,
                max: self.max_tags,
            });
        }

        unsafe {
            let slot_ptr = self.bank_ptr(self.active_write_bank).add(slot_id);
            let prev_seq = (*slot_ptr).sequence;
            let slot = TagSlot {
                tag_id: slot_id as u32,
                type_code,
                quality,
                value,
                timestamp_ns,
                sequence: prev_seq.wrapping_add(1),
                flags,
                _padding: [0u8; 28],
            };
            std::ptr::write(slot_ptr, slot);
        }
        Ok(())
    }

    /// Atomically publish the current write buffer and advance writer to the new bank.
    /// Guarantees zero allocation and lock-free execution (< 50ns latency).
    #[inline(always)]
    pub fn publish_cycle(&mut self) -> usize {
        let next_bank = self.bank_state().publish_write(self.active_write_bank);
        self.active_write_bank = next_bank;
        next_bank
    }

    /// Acquire the latest clean read bank snapshot and read a tag slot.
    #[inline(always)]
    pub fn read_tag(&self, slot_id: usize) -> Result<TagSlot, ShmError> {
        if slot_id >= self.max_tags {
            return Err(ShmError::SlotOutOfBounds {
                index: slot_id,
                max: self.max_tags,
            });
        }
        let (read_bank, _) = self.bank_state().acquire_read();
        unsafe {
            let slot_ptr = self.bank_ptr(read_bank).add(slot_id);
            Ok(std::ptr::read(slot_ptr))
        }
    }

    /// Read a tag slot from the currently designated read bank without advancing state.
    #[inline(always)]
    pub fn peek_tag(&self, slot_id: usize) -> Result<TagSlot, ShmError> {
        if slot_id >= self.max_tags {
            return Err(ShmError::SlotOutOfBounds {
                index: slot_id,
                max: self.max_tags,
            });
        }
        let read_bank = self.bank_state().peek_read_bank();
        unsafe {
            let slot_ptr = self.bank_ptr(read_bank).add(slot_id);
            Ok(std::ptr::read(slot_ptr))
        }
    }
}

/// Standalone Writer Handle for the control loop / drivers.
pub struct ShmWriterHandle<S: PlatformShm = DefaultShm> {
    engine: TagBusEngine<S>,
}

impl<S: PlatformShm> ShmWriterHandle<S> {
    pub fn new(name: &str, max_tags: usize) -> Result<Self, ShmError> {
        let engine = TagBusEngine::open_or_create(name, max_tags)?;
        Ok(Self { engine })
    }

    #[inline(always)]
    pub fn write_tag(
        &mut self,
        slot_id: usize,
        type_code: u16,
        quality: QualityCode,
        value: TagValueRaw,
        timestamp_ns: u64,
        flags: u32,
    ) -> Result<(), ShmError> {
        self.engine
            .write_tag(slot_id, type_code, quality, value, timestamp_ns, flags)
    }

    /// Restore a tag from NVRAM retention.
    ///
    /// `type_code` is a parameter rather than a hardcoded `0`: a restored slot that loses its
    /// declared type is unreadable to every typed consumer (telemetry reports it as
    /// `UNSPECIFIED`/null), so retention must carry the type across a restart. Passing `0` remains
    /// possible but now means "unknown", and the caller has to mean it.
    #[inline(always)]
    pub fn restore_retained_tag(&mut self, slot_id: u32, type_code: u16, value: TagValueRaw) {
        let _ = self.engine.write_tag(
            slot_id as usize,
            type_code,
            QualityCode::GOOD,
            value,
            0,
            crate::slot::TagFlags::RETENTION_RETAIN,
        );
    }

    #[inline(always)]
    pub fn publish(&mut self) -> usize {
        self.engine.publish_cycle()
    }

    #[inline(always)]
    pub fn max_tags(&self) -> usize {
        self.engine.max_tags()
    }
}

/// Standalone Reader Handle for HMI / SSE / Historian / MCP consumers.
pub struct ShmReaderHandle<S: PlatformShm = DefaultShm> {
    engine: TagBusEngine<S>,
    current_read_bank: usize,
}

impl<S: PlatformShm> ShmReaderHandle<S> {
    pub fn new(name: &str, max_tags: usize) -> Result<Self, ShmError> {
        let engine = TagBusEngine::open_or_create(name, max_tags)?;
        let initial_read = engine.bank_state().peek_read_bank();
        Ok(Self {
            engine,
            current_read_bank: initial_read,
        })
    }

    /// Refresh to latest published snapshot. Returns true if newer data was acquired.
    #[inline(always)]
    pub fn update(&mut self) -> bool {
        let (bank, has_new) = self.engine.bank_state().acquire_read();
        self.current_read_bank = bank;
        has_new
    }

    /// Read tag slot from current active read snapshot.
    #[inline(always)]
    pub fn read_tag(&self, slot_id: usize) -> Result<TagSlot, ShmError> {
        if slot_id >= self.engine.max_tags() {
            return Err(ShmError::SlotOutOfBounds {
                index: slot_id,
                max: self.engine.max_tags(),
            });
        }
        unsafe {
            let slot_ptr = self.engine.bank_ptr(self.current_read_bank).add(slot_id);
            Ok(std::ptr::read(slot_ptr))
        }
    }

    #[inline(always)]
    pub fn max_tags(&self) -> usize {
        self.engine.max_tags()
    }
}
