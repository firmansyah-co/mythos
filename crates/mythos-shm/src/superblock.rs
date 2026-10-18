//! Superblock definition and shared memory segment layout calculations.

use crate::buffer::TripleBufferState;
use crate::slot::TagSlot;
use bytemuck::Zeroable;

/// Magic bytes identifying MythOS TagBus shared memory segments ("MYTHOS_1").
pub const SHM_MAGIC: [u8; 8] = *b"MYTHOS_1";

/// Current TagBus SHM schema layout version.
pub const SHM_VERSION: u32 = 1;

/// Default tag capacity allocated per buffer bank (32,768 slots).
pub const DEFAULT_TAG_CAPACITY: usize = 32_768;

/// Size of the superblock in bytes (4 KiB cache page).
pub const SUPERBLOCK_SIZE: usize = 4096;

/// Calculate total memory bytes required for a TagBus segment with `tag_count` slots.
#[inline(always)]
pub const fn calculate_shm_size(tag_count: usize) -> usize {
    SUPERBLOCK_SIZE + (tag_count * std::mem::size_of::<TagSlot>() * 3)
}

/// Calculate byte offset from segment base to the start of bank `index` (0, 1, or 2).
#[inline(always)]
pub const fn calculate_bank_offset(tag_count: usize, bank_index: usize) -> usize {
    SUPERBLOCK_SIZE + (bank_index * tag_count * std::mem::size_of::<TagSlot>())
}

/// 4 KiB Cache-line aligned Superblock placed at offset 0 of `/dev/shm/mythos_tagbus`.
#[derive(Debug)]
#[repr(C, align(64))]
pub struct ShmSuperblock {
    /// Magic identifier: "MYTHOS_1"
    pub magic: [u8; 8],

    /// Schema layout version (1)
    pub version: u32,

    /// Total number of registered tags in each buffer bank (e.g. 32,768)
    pub max_tags: u32,

    /// Byte offset from segment start to Bank A (Bank 0)
    pub bank_a_offset: u64,

    /// Byte offset from segment start to Bank B (Bank 1)
    pub bank_b_offset: u64,

    /// Byte offset from segment start to Bank C (Bank 2)
    pub bank_c_offset: u64,

    /// Master monotonic epoch counter
    pub master_epoch: u64,

    /// Triple-buffer lock-free state descriptor (AtomicU64)
    pub bank_state: TripleBufferState,

    /// CRC32 checksum of superblock configuration fields
    pub config_crc32: u32,

    /// Reserved padding to strictly occupy exactly 4,096 bytes (4 KiB)
    pub _reserved: [u8; 4036],
}

unsafe impl Zeroable for ShmSuperblock {}

const _: () = assert!(std::mem::size_of::<ShmSuperblock>() == 4096);
const _: () = assert!(std::mem::align_of::<ShmSuperblock>() == 64);

impl ShmSuperblock {
    /// Construct and initialize a new Superblock structure with calculated bank offsets.
    pub fn new(max_tags: u32) -> Self {
        let tag_count = max_tags as usize;
        let bank_a = calculate_bank_offset(tag_count, 0) as u64;
        let bank_b = calculate_bank_offset(tag_count, 1) as u64;
        let bank_c = calculate_bank_offset(tag_count, 2) as u64;

        let mut sb = Self {
            magic: SHM_MAGIC,
            version: SHM_VERSION,
            max_tags,
            bank_a_offset: bank_a,
            bank_b_offset: bank_b,
            bank_c_offset: bank_c,
            master_epoch: 1,
            bank_state: TripleBufferState::new(),
            config_crc32: 0,
            _reserved: [0u8; 4036],
        };
        sb.config_crc32 = sb.calculate_crc32();
        sb
    }

    /// Calculate CRC32 checksum over the configuration header bytes.
    pub fn calculate_crc32(&self) -> u32 {
        let mut hasher = crc32fast::Hasher::new();
        hasher.update(&self.magic);
        hasher.update(&self.version.to_le_bytes());
        hasher.update(&self.max_tags.to_le_bytes());
        hasher.update(&self.bank_a_offset.to_le_bytes());
        hasher.update(&self.bank_b_offset.to_le_bytes());
        hasher.update(&self.bank_c_offset.to_le_bytes());
        hasher.update(&self.master_epoch.to_le_bytes());
        hasher.finalize()
    }

    /// Validate magic signature and CRC32 integrity.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.magic != SHM_MAGIC {
            return Err("Invalid TagBus SHM magic header");
        }
        if self.version != SHM_VERSION {
            return Err("Unsupported TagBus SHM layout version");
        }
        if self.config_crc32 != self.calculate_crc32() {
            return Err("TagBus SHM superblock CRC32 checksum mismatch");
        }
        Ok(())
    }
}
