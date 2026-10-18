//! Error types for the Shared Memory and TagBus subsystem.

use mythos_pal::PalError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ShmError {
    #[error("Platform Abstraction Layer error: {0}")]
    Pal(#[from] PalError),

    #[error("Segment capacity exceeded: requested slot {index} >= max tags {max}")]
    SlotOutOfBounds { index: usize, max: usize },

    #[error("Superblock validation failure: {0}")]
    SuperblockCorrupt(&'static str),

    #[error("Segment size mismatch: expected at least {expected} bytes, got {actual}")]
    SegmentTooSmall { expected: usize, actual: usize },

    #[error("Zero tag capacity is invalid")]
    ZeroCapacity,
}
