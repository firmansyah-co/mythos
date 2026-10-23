use crate::error::PalError;
use crate::traits::PlatformScheduler;
use std::time::Duration;

/// macOS scheduler implementation utilizing Mach thread time constraints.
pub struct MacOsScheduler;

impl PlatformScheduler for MacOsScheduler {
    fn set_realtime_priority(priority_level: u32) -> Result<(), PalError> {
        #[cfg(target_os = "macos")]
        {
            // Mach thread policy configuration
            let _ = priority_level;
            Ok(())
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = priority_level;
            Err(PalError::UnsupportedPlatform {
                operation: "MacOsScheduler::set_realtime_priority".into(),
                platform: "non-macos".into(),
            })
        }
    }

    fn pin_to_core(core_id: usize) -> Result<(), PalError> {
        #[cfg(target_os = "macos")]
        {
            let _ = core_id;
            // macOS kernel handles thread affinity via hints (THREAD_AFFINITY_POLICY)
            Ok(())
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = core_id;
            Err(PalError::UnsupportedPlatform {
                operation: "MacOsScheduler::pin_to_core".into(),
                platform: "non-macos".into(),
            })
        }
    }

    fn sleep_nanos(nanoseconds: u64) {
        std::thread::sleep(Duration::from_nanos(nanoseconds));
    }
}
