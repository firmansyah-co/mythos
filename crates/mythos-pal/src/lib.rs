//! # MythOS Platform Abstraction Layer (PAL)
//!
//! Encapsulates operating-system dependencies (POSIX shared memory, Windows named SHM,
//! macOS tmpfs/mmap, real-time priority scheduling, and dynamic library loading) behind
//! clean, OS-agnostic Rust traits.

pub mod error;
pub mod library;
pub mod scheduler;
pub mod shm;
pub mod traits;

pub use error::PalError;
pub use library::DynamicLibrary;
pub use scheduler::{
    DefaultScheduler, LinuxScheduler, MacOsScheduler, NoopScheduler, WindowsScheduler,
};
pub use shm::{DefaultShm, MacOsShm, MemoryShm, PosixShm, WindowsShm};
pub use traits::{PlatformLibrary, PlatformScheduler, PlatformShm};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_memory_shm_create_read_write() {
        let name = "test_memory_segment";
        let size = 4096;
        let mut shm = MemoryShm::open_or_create(name, size).expect("Failed to create MemoryShm");

        assert_eq!(shm.name(), name);
        assert_eq!(shm.len(), size);
        assert!(!shm.is_empty());
        assert!(!shm.as_ptr().is_null());

        // Verify initial slice is zeroed
        assert!(shm.as_slice().iter().all(|&b| b == 0));

        // Test write and read through mutable slice
        let test_payload = b"MythOS Platform Abstraction Layer Verified";
        shm.as_mut_slice()[..test_payload.len()].copy_from_slice(test_payload);
        assert_eq!(&shm.as_slice()[..test_payload.len()], test_payload);

        // Test offset writes
        let offset = 2048;
        let tail_payload = b"End-of-block telemetry signature";
        shm.as_mut_slice()[offset..offset + tail_payload.len()].copy_from_slice(tail_payload);
        assert_eq!(
            &shm.as_slice()[offset..offset + tail_payload.len()],
            tail_payload
        );

        // Test flush
        shm.flush().expect("Flush must succeed on MemoryShm");
    }

    #[test]
    fn test_memory_shm_invalid_size() {
        let err = MemoryShm::open_or_create("invalid_zero", 0);
        assert!(err.is_err());
        match err {
            Err(PalError::InvalidSize { requested, .. }) => assert_eq!(requested, 0),
            other => panic!("Expected PalError::InvalidSize, got {:?}", other),
        }
    }

    #[test]
    fn test_windows_shm_format_name() {
        assert_eq!(
            WindowsShm::format_name("tagbus"),
            "Local\\MythOS_tagbus"
        );
        assert_eq!(
            WindowsShm::format_name("/tagbus"),
            "Local\\MythOS_tagbus"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_posix_shm_lifecycle() {
        let name = "mythos_test_posix_shm";
        let size = 8192;

        let mut shm = PosixShm::open_or_create(name, size).expect("Failed to create PosixShm");
        assert_eq!(shm.name(), format!("/{}", name));
        assert_eq!(shm.len(), size);
        assert!(!shm.is_empty());
        assert!(!shm.as_ptr().is_null());

        let payload = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        shm.as_mut_slice()[..payload.len()].copy_from_slice(&payload);
        assert_eq!(&shm.as_slice()[..payload.len()], &payload);

        shm.flush().expect("POSIX shm flush should succeed");

        drop(shm);
        // Clean up unlinking
        PosixShm::unlink(name).expect("POSIX shm unlink should succeed");
    }

    #[test]
    fn test_noop_scheduler_operations() {
        NoopScheduler::set_realtime_priority(80).expect("NoopScheduler priority should succeed");
        NoopScheduler::pin_to_core(0).expect("NoopScheduler pin should succeed");
        NoopScheduler::sleep_nanos(1000);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_linux_scheduler_graceful_handling() {
        // In unprivileged CI/container environments, set_realtime_priority might fail with RealtimePriorityDenied
        match LinuxScheduler::set_realtime_priority(85) {
            Ok(_) => {}
            Err(PalError::RealtimePriorityDenied { priority_level, .. }) => {
                assert_eq!(priority_level, 85);
            }
            Err(e) => panic!("Unexpected error from LinuxScheduler: {:?}", e),
        }

        // Test sleep_nanos
        LinuxScheduler::sleep_nanos(500);
    }

    #[test]
    fn test_dynamic_library_not_found() {
        let non_existent = PathBuf::from("/non/existent/path/to/libmythos_dummy.so");
        let result = DynamicLibrary::load(&non_existent);
        assert!(result.is_err());
        match result {
            Err(PalError::LibraryLoadFailed { path, .. }) => {
                assert!(path.contains("libmythos_dummy"));
            }
            other => panic!("Expected PalError::LibraryLoadFailed, got {:?}", other),
        }
    }

    #[test]
    fn test_pal_error_display() {
        let err = PalError::InvalidSize {
            requested: 0,
            reason: "size must be positive".to_string(),
        };
        let formatted = format!("{}", err);
        assert!(formatted.contains("Invalid shared memory size"));

        let err = PalError::RealtimePriorityDenied {
            priority_level: 99,
            reason: "Permission denied".to_string(),
        };
        let formatted = format!("{}", err);
        assert!(formatted.contains("Realtime scheduling priority denied"));
    }
}
