use crate::error::PalError;
use crate::traits::PlatformShm;
use std::alloc::{alloc_zeroed, dealloc, Layout};

/// Cross-platform in-memory simulated shared memory for unit tests and unprivileged containers.
#[derive(Debug)]
pub struct MemoryShm {
    name: String,
    ptr: *mut u8,
    layout: Layout,
    size: usize,
}

unsafe impl Send for MemoryShm {}
unsafe impl Sync for MemoryShm {}

impl MemoryShm {
    /// Return the identifier of this memory segment.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Create an isolated in-memory buffer simulating shared memory.
    pub fn new_isolated(name: &str, size_bytes: usize) -> Result<Self, PalError> {
        if size_bytes == 0 {
            return Err(PalError::InvalidSize {
                requested: size_bytes,
                reason: "SHM segment size must be greater than zero".into(),
            });
        }

        // Align to 4096 bytes (standard page boundary)
        let layout = Layout::from_size_align(size_bytes, 4096).map_err(|e| {
            PalError::InvalidSize {
                requested: size_bytes,
                reason: format!("Memory alignment error: {}", e),
            }
        })?;

        let raw_ptr = unsafe { alloc_zeroed(layout) };
        if raw_ptr.is_null() {
            return Err(PalError::ShmCreateFailed {
                name: name.to_string(),
                reason: "Memory allocation failed".into(),
            });
        }

        Ok(Self {
            name: name.to_string(),
            ptr: raw_ptr,
            layout,
            size: size_bytes,
        })
    }
}

impl PlatformShm for MemoryShm {
    fn open_or_create(name: &str, size_bytes: usize) -> Result<Self, PalError> {
        Self::new_isolated(name, size_bytes)
    }

    fn as_ptr(&self) -> *mut u8 {
        self.ptr
    }

    fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr, self.size) }
    }

    fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.size) }
    }

    fn len(&self) -> usize {
        self.size
    }

    fn flush(&self) -> Result<(), PalError> {
        // In-memory buffer does not require synchronization to disk
        Ok(())
    }
}

impl Drop for MemoryShm {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                dealloc(self.ptr, self.layout);
            }
        }
    }
}
