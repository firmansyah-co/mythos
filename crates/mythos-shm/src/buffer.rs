//! Lock-free triple-buffering state machine and atomic bank coordination.

use bytemuck::Zeroable;
use std::sync::atomic::{AtomicU64, Ordering};

/// State layout:
/// Bit 0..1: write_index (0, 1, or 2)
/// Bit 2..3: clean_index (0, 1, or 2)
/// Bit 4..5: read_index  (0, 1, or 2)
/// Bit 6:    has_new_data (1 if new buffer has been published since last acquire_read, 0 otherwise)
/// Bits 7..63: Monotonic write generation counter
#[derive(Debug)]
#[repr(transparent)]
pub struct TripleBufferState {
    state: AtomicU64,
}

// AtomicU64 is guaranteed to have standard 8-byte layout and zero-initializable bit pattern
unsafe impl Zeroable for TripleBufferState {}

impl TripleBufferState {
    /// Initial buffer configuration:
    /// Write buffer = Bank 0
    /// Clean buffer (Surface) = Bank 1
    /// Read buffer = Bank 2
    /// has_new_data = 0
    /// Generation = 0
    pub const fn new() -> Self {
        let initial_val: u64 = (1 << 2) | (2 << 4);
        Self {
            state: AtomicU64::new(initial_val),
        }
    }

    /// Read the raw 64-bit state descriptor.
    #[inline(always)]
    pub fn load_raw(&self, order: Ordering) -> u64 {
        self.state.load(order)
    }

    /// Unpack current state components: (write, clean, read, has_new, generation).
    #[inline(always)]
    pub fn unpack(raw: u64) -> (usize, usize, usize, bool, u64) {
        let write = (raw & 0x03) as usize;
        let clean = ((raw >> 2) & 0x03) as usize;
        let read = ((raw >> 4) & 0x03) as usize;
        let has_new = ((raw >> 6) & 0x01) == 1;
        let gen = raw >> 7;
        (write, clean, read, has_new, gen)
    }

    /// Returns the currently active write bank index without mutating state.
    #[inline(always)]
    pub fn current_write_bank(&self) -> usize {
        (self.state.load(Ordering::Acquire) & 0x03) as usize
    }

    /// Called by the Producer/Writer when it has finished updating the current write buffer.
    /// Atomically swaps the active write bank with the clean surface bank and sets has_new_data = 1.
    /// Returns the next buffer bank index that the Producer must write to.
    #[inline(always)]
    pub fn publish_write(&self, current_write: usize) -> usize {
        let mut curr = self.state.load(Ordering::Relaxed);
        loop {
            let write = (curr & 0x03) as usize;
            let clean = ((curr >> 2) & 0x03) as usize;
            let read = ((curr >> 4) & 0x03) as usize;
            let gen = curr >> 7;

            debug_assert_eq!(write, current_write, "Producer write bank mismatch");

            // Swap write and clean, mark new data flag, increment monotonic generation
            let next_write = clean;
            let next_clean = current_write;
            let next_has_new = 1u64;
            let next_gen = gen.wrapping_add(1);

            let next_state = (next_write as u64)
                | ((next_clean as u64) << 2)
                | ((read as u64) << 4)
                | (next_has_new << 6)
                | (next_gen << 7);

            match self.state.compare_exchange_weak(
                curr,
                next_state,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return next_write,
                Err(actual) => curr = actual,
            }
        }
    }

    /// Called by Consumer/Reader to acquire the freshest snapshot bank.
    /// If new data was published since the last read, clean and read banks are atomically swapped
    /// and has_new_data is cleared. Returns the active read bank index and a boolean indicating
    /// whether fresh data was acquired.
    #[inline(always)]
    pub fn acquire_read(&self) -> (usize, bool) {
        let mut curr = self.state.load(Ordering::Acquire);
        loop {
            let write = (curr & 0x03) as usize;
            let clean = ((curr >> 2) & 0x03) as usize;
            let read = ((curr >> 4) & 0x03) as usize;
            let has_new = ((curr >> 6) & 0x01) == 1;
            let gen = curr >> 7;

            if !has_new {
                // No fresh write published; retain current read buffer
                return (read, false);
            }

            // Swap clean and read, clear has_new
            let next_read = clean;
            let next_clean = read;
            let next_has_new = 0u64;

            let next_state = (write as u64)
                | ((next_clean as u64) << 2)
                | ((next_read as u64) << 4)
                | (next_has_new << 6)
                | (gen << 7);

            match self.state.compare_exchange_weak(
                curr,
                next_state,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return (next_read, true),
                Err(actual) => curr = actual,
            }
        }
    }

    /// Peek at current read bank without updating or clearing the has_new flag.
    #[inline(always)]
    pub fn peek_read_bank(&self) -> usize {
        ((self.state.load(Ordering::Acquire) >> 4) & 0x03) as usize
    }
}

impl Default for TripleBufferState {
    fn default() -> Self {
        Self::new()
    }
}
