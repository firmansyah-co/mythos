use crate::error::PalError;
use crate::traits::PlatformShm;
use std::ffi::CString;
use std::os::raw::c_int;
use std::ptr;

/// POSIX shared memory implementation for Linux/UNIX systems.
#[derive(Debug)]
pub struct PosixShm {
    name: String,
    fd: c_int,
    ptr: *mut u8,
    size: usize,
}

// PosixShm is safe to send and share across threads
unsafe impl Send for PosixShm {}
unsafe impl Sync for PosixShm {}

impl PosixShm {
    /// Format POSIX shm name ensuring standard leading slash.
    fn format_name(name: &str) -> String {
        let trimmed = name.trim_start_matches('/');
        format!("/{}", trimmed)
    }

    /// Return the name identifier of this POSIX shared memory segment.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Unlink a POSIX shared memory segment by name.
    pub fn unlink(name: &str) -> Result<(), PalError> {
        let formatted = Self::format_name(name);
        let c_name = CString::new(formatted.as_str()).map_err(|e| PalError::ShmUnlinkFailed {
            name: formatted.clone(),
            reason: format!("CString conversion error: {}", e),
        })?;

        let res = unsafe { libc::shm_unlink(c_name.as_ptr()) };
        if res != 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() != Some(libc::ENOENT) {
                return Err(PalError::ShmUnlinkFailed {
                    name: formatted,
                    reason: err.to_string(),
                });
            }
        }
        Ok(())
    }
}

impl PlatformShm for PosixShm {
    fn open_or_create(name: &str, size_bytes: usize) -> Result<Self, PalError> {
        if size_bytes == 0 {
            return Err(PalError::InvalidSize {
                requested: size_bytes,
                reason: "SHM segment size must be greater than zero".into(),
            });
        }

        let formatted = Self::format_name(name);
        let c_name = CString::new(formatted.as_str()).map_err(|e| PalError::ShmCreateFailed {
            name: formatted.clone(),
            reason: format!("CString conversion error: {}", e),
        })?;

        // Open or create POSIX SHM
        let oflags = libc::O_RDWR | libc::O_CREAT;
        let mode = 0o660; // rw-rw----
        let fd = unsafe { libc::shm_open(c_name.as_ptr(), oflags, mode as libc::mode_t) };
        if fd < 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EACCES) {
                return Err(PalError::PermissionDenied {
                    operation: "shm_open".into(),
                    reason: err.to_string(),
                });
            }
            return Err(PalError::ShmOpenFailed {
                name: formatted,
                reason: err.to_string(),
            });
        }

        // Set size with ftruncate
        let trunc_res = unsafe { libc::ftruncate(fd, size_bytes as libc::off_t) };
        if trunc_res != 0 {
            let err = std::io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(PalError::ShmTruncateFailed {
                name: formatted,
                size: size_bytes,
                reason: err.to_string(),
            });
        }

        // Memory-map the segment
        let map_ptr = unsafe {
            libc::mmap(
                ptr::null_mut(),
                size_bytes,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };

        if map_ptr == libc::MAP_FAILED {
            let err = std::io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(PalError::ShmMapFailed {
                name: formatted,
                reason: err.to_string(),
            });
        }

        Ok(Self {
            name: formatted,
            fd,
            ptr: map_ptr as *mut u8,
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
        let res = unsafe { libc::msync(self.ptr as *mut libc::c_void, self.size, libc::MS_SYNC) };
        if res != 0 {
            let err = std::io::Error::last_os_error();
            return Err(PalError::ShmFlushFailed {
                name: self.name.clone(),
                reason: err.to_string(),
            });
        }
        Ok(())
    }
}

impl Drop for PosixShm {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                libc::munmap(self.ptr as *mut libc::c_void, self.size);
            }
        }
        if self.fd >= 0 {
            unsafe {
                libc::close(self.fd);
            }
        }
    }
}
