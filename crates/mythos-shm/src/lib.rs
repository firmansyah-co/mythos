//! # MythOS Lock-Free Shared Memory Engine
//!
//! Provides lock-free triple-buffered TagBus structures, cache-line aligned (64-byte)
//! Plain Old Data (POD) slot layouts, and cross-platform shared memory management.

pub mod buffer;
pub mod engine;
pub mod error;
pub mod pal;
pub mod slot;
pub mod superblock;

pub use buffer::TripleBufferState;
pub use engine::{ShmReaderHandle, ShmWriterHandle, TagBusEngine};
pub use error::ShmError;
pub use pal::*;
pub use slot::{DecodedValue, QualityCode, TagDataType, TagFlags, TagSlot, TagValueRaw};
pub use superblock::{
    calculate_bank_offset, calculate_shm_size, ShmSuperblock, DEFAULT_TAG_CAPACITY, SHM_MAGIC,
    SHM_VERSION, SUPERBLOCK_SIZE,
};
