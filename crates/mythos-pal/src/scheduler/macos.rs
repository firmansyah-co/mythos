use crate::error::PalError;
use crate::traits::PlatformScheduler;
use std::time::Duration;

/// macOS scheduler.
///
/// Neither real-time priority nor core pinning is implemented on macOS, and both say so instead
/// of reporting a success that changed nothing. (macOS offers only Mach time-constraint policies
/// and affinity *hints*; nothing here calls them.)
pub struct MacOsScheduler;

impl PlatformScheduler for MacOsScheduler {
    fn set_realtime_priority(priority_level: u32) -> Result<(), PalError> {
        let _ = priority_level;
        Err(PalError::UnsupportedPlatform {
            operation: "MacOsScheduler::set_realtime_priority".into(),
            platform: std::env::consts::OS.into(),
        })
    }

    fn pin_to_core(core_id: usize) -> Result<(), PalError> {
        let _ = core_id;
        Err(PalError::UnsupportedPlatform {
            operation: "MacOsScheduler::pin_to_core".into(),
            platform: std::env::consts::OS.into(),
        })
    }

    fn sleep_nanos(nanoseconds: u64) {
        std::thread::sleep(Duration::from_nanos(nanoseconds));
    }
}
