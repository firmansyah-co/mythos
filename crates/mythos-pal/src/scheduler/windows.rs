use crate::error::PalError;
use crate::traits::PlatformScheduler;
use std::time::Duration;

#[cfg(windows)]
use windows_sys::Win32::System::Threading::{
    GetCurrentThread, SetThreadAffinityMask, SetThreadPriority,
    THREAD_PRIORITY_TIME_CRITICAL,
};

/// Windows scheduler implementation using THREAD_PRIORITY_TIME_CRITICAL and affinity masks.
pub struct WindowsScheduler;

impl PlatformScheduler for WindowsScheduler {
    fn set_realtime_priority(priority_level: u32) -> Result<(), PalError> {
        #[cfg(windows)]
        {
            let handle = unsafe { GetCurrentThread() };
            // On Windows, priority_level > 0 targets THREAD_PRIORITY_TIME_CRITICAL
            let priority = if priority_level > 0 {
                THREAD_PRIORITY_TIME_CRITICAL
            } else {
                0
            };

            let res = unsafe { SetThreadPriority(handle, priority) };
            if res == 0 {
                let err = std::io::Error::last_os_error();
                return Err(PalError::RealtimePriorityDenied {
                    priority_level,
                    reason: err.to_string(),
                });
            }
            Ok(())
        }

        #[cfg(not(windows))]
        {
            let _ = priority_level;
            Err(PalError::UnsupportedPlatform {
                operation: "WindowsScheduler::set_realtime_priority".into(),
                platform: "non-windows".into(),
            })
        }
    }

    fn pin_to_core(core_id: usize) -> Result<(), PalError> {
        #[cfg(windows)]
        {
            if core_id >= usize::BITS as usize {
                return Err(PalError::CorePinningFailed {
                    core_id,
                    reason: format!("Core ID exceeds bitmask capacity ({})", usize::BITS),
                });
            }

            let handle = unsafe { GetCurrentThread() };
            let mask = 1usize << core_id;
            let prev_mask = unsafe { SetThreadAffinityMask(handle, mask) };
            if prev_mask == 0 {
                let err = std::io::Error::last_os_error();
                return Err(PalError::CorePinningFailed {
                    core_id,
                    reason: err.to_string(),
                });
            }
            Ok(())
        }

        #[cfg(not(windows))]
        {
            let _ = core_id;
            Err(PalError::UnsupportedPlatform {
                operation: "WindowsScheduler::pin_to_core".into(),
                platform: "non-windows".into(),
            })
        }
    }

    fn sleep_nanos(nanoseconds: u64) {
        std::thread::sleep(Duration::from_nanos(nanoseconds));
    }
}
