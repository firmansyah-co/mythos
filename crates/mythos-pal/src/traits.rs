use crate::error::PalError;
use std::path::Path;

/// Cross-platform abstraction for shared memory segments.
pub trait PlatformShm: Send + Sync {
    /// Open an existing shared memory segment or create a new one with the given byte size.
    fn open_or_create(name: &str, size_bytes: usize) -> Result<Self, PalError>
    where
        Self: Sized;

    /// Direct raw pointer to the mapped memory region.
    fn as_ptr(&self) -> *mut u8;

    /// Immutable byte slice of the mapped memory region.
    fn as_slice(&self) -> &[u8];

    /// Mutable byte slice of the mapped memory region.
    fn as_mut_slice(&mut self) -> &mut [u8];

    /// Total byte length of the mapped memory region.
    fn len(&self) -> usize;

    /// Returns true if the mapped region has length 0.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Flush pending writes to backing memory or storage if applicable.
    fn flush(&self) -> Result<(), PalError>;
}

/// Cross-platform real-time thread scheduling and affinity abstraction.
pub trait PlatformScheduler {
    /// Elevate calling thread to deterministic real-time execution priority.
    fn set_realtime_priority(priority_level: u32) -> Result<(), PalError>;

    /// Bind calling thread to a specific hardware CPU core.
    fn pin_to_core(core_id: usize) -> Result<(), PalError>;

    /// High-resolution monotonic sleep guaranteeing minimal timer overrun.
    fn sleep_nanos(nanoseconds: u64);
}

/// Cross-platform dynamic library loader abstraction.
pub trait PlatformLibrary: Send + Sync {
    /// Load a dynamic library (`.so`, `.dll`, or `.dylib`) from the filesystem.
    fn load<P: AsRef<Path>>(path: P) -> Result<Self, PalError>
    where
        Self: Sized;

    /// Lookup a symbol within the loaded dynamic library.
    ///
    /// # Safety
    /// Caller must guarantee that the resolved symbol conforms to the requested type `T`.
    unsafe fn get_symbol<T>(&self, symbol_name: &str) -> Result<*const T, PalError>;
}
