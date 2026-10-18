use crate::error::PalError;
use crate::traits::PlatformScheduler;
use std::time::Duration;

/// No-op scheduler for simulation, non-realtime environments, or testing fallbacks.
pub struct NoopScheduler;

impl PlatformScheduler for NoopScheduler {
    fn set_realtime_priority(_priority_level: u32) -> Result<(), PalError> {
        Ok(())
    }

    fn pin_to_core(_core_id: usize) -> Result<(), PalError> {
        Ok(())
    }

    fn sleep_nanos(nanoseconds: u64) {
        std::thread::sleep(Duration::from_nanos(nanoseconds));
    }
}
