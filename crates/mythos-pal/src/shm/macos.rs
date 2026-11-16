use crate::error::PalError;
use crate::traits::PlatformShm;
use std::ffi::CString;
use std::os::raw::c_int;
use std::ptr;

/// macOS shared memory implementation with 31-character name constraint handling.
#[derive(Debug)]
pub struct MacOsShm {
    name: String,
    fd: c_int,
    ptr: *mut u8,
    size: usize,
}

unsafe impl Send for MacOsShm {}
unsafe impl Sync for MacOsShm {}

impl MacOsShm {
    /// Format and ensure macOS POSIX shm name conforms to <= 31 characters.
    fn format_name(name: &str) -> String {
        let trimmed = name.trim_start_matches('/');
        if trimmed.len() <= 30 {
            format!("/{}", trimmed)
        } else {
            // Hash the long name using crc32fast to keep it within 31 characters
            let mut hasher = crc32fast::Hasher::new();
            hasher.update(trimmed.as_bytes());
            let hash = hasher.finalize();
            // Characters, not a byte slice: a byte index can fall inside a multi-byte character.
            let prefix: String = trimmed
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                .take(20)
                .collect();
            format!("/{:08x}_{}", hash, prefix)
        }
    }

    /// Return the name identifier of this macOS shared memory segment.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Unlink a macOS shared memory segment by name.
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

impl MacOsShm {
    /// Open the segment, creating it at `size_bytes` if it does not exist.
    ///
    /// macOS sets a shared memory object's size once: `ftruncate` on one that already has a size
    /// fails with `EINVAL`, even to the same size. So the segment is created exclusively and sized
    /// by whoever creates it; everyone else opens it and checks that it is large enough.
    fn open_sized(formatted: &str, c_name: &CString, size_bytes: usize) -> Result<c_int, PalError> {
        let mode = 0o660 as libc::c_uint;
        let open_error = |err: std::io::Error| {
            if err.raw_os_error() == Some(libc::EACCES) {
                PalError::PermissionDenied {
                    operation: "shm_open".into(),
                    reason: err.to_string(),
                }
            } else {
                PalError::ShmOpenFailed {
                    name: formatted.to_string(),
                    reason: err.to_string(),
                }
            }
        };

        let fd = unsafe {
            libc::shm_open(
                c_name.as_ptr(),
                libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
                mode,
            )
        };
        if fd >= 0 {
            if unsafe { libc::ftruncate(fd, size_bytes as libc::off_t) } != 0 {
                let err = std::io::Error::last_os_error();
                unsafe {
                    libc::close(fd);
                    libc::shm_unlink(c_name.as_ptr());
                }
                return Err(PalError::ShmTruncateFailed {
                    name: formatted.to_string(),
                    size: size_bytes,
                    reason: err.to_string(),
                });
            }
            return Ok(fd);
        }
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() != Some(libc::EEXIST) {
            return Err(open_error(err));
        }

        let fd = unsafe { libc::shm_open(c_name.as_ptr(), libc::O_RDWR, mode) };
        if fd < 0 {
            return Err(open_error(std::io::Error::last_os_error()));
        }
        // The creator sizes the segment just after creating it; a size of zero means it has not
        // got there yet. Wait for it briefly rather than map an empty object.
        for _ in 0..200 {
            let mut stat: libc::stat = unsafe { std::mem::zeroed() };
            if unsafe { libc::fstat(fd, &mut stat) } != 0 {
                let err = std::io::Error::last_os_error();
                unsafe { libc::close(fd) };
                return Err(open_error(err));
            }
            let existing = stat.st_size as usize;
            if existing >= size_bytes {
                return Ok(fd);
            }
            if existing > 0 {
                unsafe { libc::close(fd) };
                return Err(PalError::ShmTruncateFailed {
                    name: formatted.to_string(),
                    size: size_bytes,
                    reason: format!(
                        "the segment already exists with {existing} bytes, and macOS cannot \
                         resize it"
                    ),
                });
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        unsafe { libc::close(fd) };
        Err(PalError::ShmTruncateFailed {
            name: formatted.to_string(),
            size: size_bytes,
            reason: "the segment exists but its creator never gave it a size".into(),
        })
    }
}

impl PlatformShm for MacOsShm {
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

        let fd = Self::open_sized(&formatted, &c_name, size_bytes)?;

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

impl Drop for MacOsShm {
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
