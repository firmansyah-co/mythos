use crate::error::PalError;
use crate::traits::PlatformShm;

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Memory::{
    CreateFileMappingW, FlushViewOfFile, MapViewOfFile, UnmapViewOfFile, FILE_MAP_ALL_ACCESS,
    MEMORY_MAPPED_VIEW_ADDRESS, PAGE_READWRITE,
};

/// Windows named shared memory: a file mapping backed by the paging file.
///
/// Unlike a POSIX segment, a Windows file mapping has no existence of its own. It lives while some
/// process holds a handle to it and is gone when the last one is closed, so there is nothing to
/// unlink and nothing stale is left behind by a process that dies.
#[derive(Debug)]
pub struct WindowsShm {
    name: String,
    handle: HANDLE,
    ptr: *mut u8,
    size: usize,
}

unsafe impl Send for WindowsShm {}
unsafe impl Sync for WindowsShm {}

impl WindowsShm {
    /// Return the name identifier of this Windows shared memory object.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The object name of a segment, in the session's `Local\` namespace.
    pub fn format_name(name: &str) -> String {
        let clean = name.trim_start_matches('/');
        format!("Local\\MythOS_{}", clean)
    }

    /// Nothing to do: the kernel removes a file mapping when its last handle is closed.
    pub fn unlink(_name: &str) -> Result<(), PalError> {
        Ok(())
    }
}

impl PlatformShm for WindowsShm {
    fn open_or_create(name: &str, size_bytes: usize) -> Result<Self, PalError> {
        if size_bytes == 0 {
            return Err(PalError::InvalidSize {
                requested: size_bytes,
                reason: "SHM segment size must be greater than zero".into(),
            });
        }

        let full_name = Self::format_name(name);
        // The name is passed as a NUL-terminated string, so a NUL inside it would silently cut it
        // short and open some other segment; a backslash would name another namespace.
        if let Some(bad) = full_name["Local\\".len()..]
            .chars()
            .find(|c| *c == '\0' || *c == '\\')
        {
            return Err(PalError::ShmCreateFailed {
                name: full_name.clone(),
                reason: format!("invalid character {:?} in segment name", bad),
            });
        }
        let mut wide_name: Vec<u16> = full_name.encode_utf16().collect();
        wide_name.push(0);

        let size = size_bytes as u64;
        let high_size = (size >> 32) as u32;
        let low_size = (size & 0xFFFF_FFFF) as u32;

        let handle = unsafe {
            CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                std::ptr::null(),
                PAGE_READWRITE,
                high_size,
                low_size,
                wide_name.as_ptr(),
            )
        };

        if handle.is_null() {
            let err = unsafe { GetLastError() };
            return Err(PalError::ShmCreateFailed {
                name: full_name,
                reason: format!("CreateFileMappingW failed with OS error code {}", err),
            });
        }

        let view = unsafe { MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, size_bytes) };

        if view.Value.is_null() {
            let err = unsafe { GetLastError() };
            unsafe { CloseHandle(handle) };
            return Err(PalError::ShmMapFailed {
                name: full_name,
                reason: format!("MapViewOfFile failed with OS error code {}", err),
            });
        }

        Ok(Self {
            name: full_name,
            handle,
            ptr: view.Value as *mut u8,
            size: size_bytes,
        })
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
        let res = unsafe { FlushViewOfFile(self.ptr as *const _, self.size) };
        if res == 0 {
            let err = unsafe { GetLastError() };
            return Err(PalError::ShmFlushFailed {
                name: self.name.clone(),
                reason: format!("FlushViewOfFile failed with OS error code {}", err),
            });
        }
        Ok(())
    }
}

impl Drop for WindowsShm {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                    Value: self.ptr as *mut _,
                });
            }
        }
        if !self.handle.is_null() {
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }
}
