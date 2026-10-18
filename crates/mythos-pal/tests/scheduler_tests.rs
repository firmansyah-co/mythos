//! Integration tests for MythOS PAL Scheduler Abstraction.
//!
//! Verifies monotonic sleep precision, core pinning contract handling,
//! and real-time scheduling elevation without panics.

use mythos_pal::{DefaultScheduler, LinuxScheduler, NoopScheduler, PalError, PlatformScheduler};
use std::time::Instant;

#[test]
fn test_default_scheduler_sleep_nanos_accuracy() {
    let requested_nanos: u64 = 5_000_000; // 5 milliseconds
    let start = Instant::now();
    DefaultScheduler::sleep_nanos(requested_nanos);
    let elapsed = start.elapsed();

    // In a non-busy-wait system, elapsed time must be at least requested duration
    // allowing minimal margin for system timer granularity.
    assert!(
        elapsed.as_nanos() >= requested_nanos as u128,
        "sleep_nanos ended prematurely: elapsed {:?}, requested {}ns",
        elapsed,
        requested_nanos
    );
}

#[test]
fn test_noop_scheduler_sleep_nanos_accuracy() {
    let requested_nanos: u64 = 2_000_000; // 2 milliseconds
    let start = Instant::now();
    NoopScheduler::sleep_nanos(requested_nanos);
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_nanos() >= requested_nanos as u128,
        "NoopScheduler sleep ended prematurely: elapsed {:?}, requested {}ns",
        elapsed,
        requested_nanos
    );
}

#[test]
fn test_noop_scheduler_contracts() {
    // NoopScheduler must unconditionally succeed
    assert!(NoopScheduler::set_realtime_priority(85).is_ok());
    assert!(NoopScheduler::pin_to_core(0).is_ok());
}

#[test]
fn test_linux_scheduler_pin_to_core_contract() {
    // Pin to core 0: in container or multi-core system, must either succeed or return PalError cleanly
    match LinuxScheduler::pin_to_core(0) {
        Ok(()) => {
            // Successfully pinned to core 0
        }
        Err(PalError::CorePinningFailed { core_id, reason }) => {
            assert_eq!(core_id, 0);
            assert!(!reason.is_empty());
        }
        Err(PalError::PermissionDenied { .. }) => {
            // Valid error path under restricted container seccomp profiles
        }
        Err(e) => {
            panic!("Unexpected error variant from pin_to_core: {:?}", e);
        }
    }
}

#[test]
fn test_linux_scheduler_set_realtime_priority_contract() {
    // Set real-time priority 85: must either succeed (if CAP_SYS_NICE)
    // or return PalError::RealtimePriorityDenied, never panic or crash.
    match LinuxScheduler::set_realtime_priority(85) {
        Ok(()) => {
            // Running as root or with CAP_SYS_NICE capability
        }
        Err(PalError::RealtimePriorityDenied {
            priority_level,
            reason,
        }) => {
            assert_eq!(priority_level, 85);
            assert!(!reason.is_empty());
        }
        Err(PalError::PermissionDenied { .. }) => {
            // Also acceptable if OS reports EACCES
        }
        Err(e) => {
            panic!(
                "Unexpected error variant from set_realtime_priority: {:?}",
                e
            );
        }
    }
}

#[test]
fn test_default_scheduler_contracts_do_not_panic() {
    // DefaultScheduler is aliased to LinuxScheduler on Linux
    let _ = DefaultScheduler::pin_to_core(0);
    let _ = DefaultScheduler::set_realtime_priority(85);
}
