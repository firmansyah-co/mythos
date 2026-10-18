use crate::error::PalError;
use crate::traits::PlatformShm;

#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE};
#[cfg(windows)]
use windows_sys::Win32::System::Memory::{
    CreateFileMappingW, FlushViewOfFile, MapViewOfFile, UnmapViewOfFile,
    FILE_MAP_ALL_ACCESS, PAGE_READWRITE,
};

/// Windows named shared memory implementation using Pagefile-backed File Mappings.
#[derive(Debug)]
pub struct WindowsShm {
    name: String,
    #[cfg(windows)]
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

    /// Format Windows named shared memory object path under Local\\MythOS_ namespace.
    pub fn format_name(name: &str) -> String {
        let clean = name.trim_start_matches('/');
        format!("Local\\MythOS_{}", clean)
    }

    #[cfg(not(windows))]
    pub fn unlink(_name: &str) -> Result<(), PalError> {
        Ok(())
    }

    #[cfg(windows)]
    pub fn unlink(_name: &str) -> Result<(), PalError> {
        // Windows kernel automatically cleans up named file mappings when the last handle is closed.
        Ok(())
    }
}

#[cfg(windows)]
impl PlatformShm for WindowsShm {
    fn open_or_create(name: &str, size_bytes: usize) -> Result<Self, PalError> {
        if size_bytes == 0 {
            return Err(PalError::InvalidSize {
                requested: size_bytes,
                reason: "SHM segment size must be greater than zero".into(),
            });
        }

        let full_name = Self::format_name(name);
        let mut wide_name: Vec<u16> = full_name.encode_utf16().collect();
        wide_name.push(0);

        let high_size = (size_bytes >> 32) as u32;
        let low_size = (size_bytes & 0xFFFF_FFFF) as u32;

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

        let view_ptr = unsafe {
            MapViewOfFile(
                handle,
                FILE_MAP_ALL_ACCESS,
                0,
                0,
                size_bytes,
            )
        };

        if view_ptr.Value.is_null() {
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
            ptr: view_ptr.Value as *mut u8,
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
        let res = unsafe {
            FlushViewOfFile(
                windows_sys::Win32::System::Memory::MEMORY_MAPPED_VIEW_ADDRESS { Value: self.ptr as *mut _ },
                self.size,
            )
        };
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

#[cfg(windows)]
impl Drop for WindowsShm {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                UnmapViewOfFile(windows_sys::Win32::System::Memory::MEMORY_MAPPED_VIEW_ADDRESS { Value: self.ptr as *mut _ });
            }
        }
        if !self.handle.is_null() {
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }
}

// Fallback stub for non-windows builds so it compiles without warnings
#[cfg(not(windows))]
impl PlatformShm for WindowsShm {
    fn open_or_create(_name: &str, _size_bytes: usize) -> Result<Self, PalError> {
        Err(PalError::UnsupportedPlatform {
            operation: "WindowsShm::open_or_create".into(),
            platform: "non-windows".into(),
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
        Ok(())
    }
}
