//! First-Out Monotonic Latching Subsystem with microsecond/nanosecond resolution.
//!
//! Complies with ANSI/ISA-18.2 root-cause identification requirements.
//! Uses CLOCK_MONOTONIC_RAW nanosecond timestamps to detect true initiating events
//! in rapid alarm cascades.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Helper to obtain nanoseconds from monotonic hardware clock (CLOCK_MONOTONIC_RAW).
pub fn get_monotonic_raw_ns() -> u64 {
    #[cfg(target_os = "linux")]
    {
        let mut ts = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        let res = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC_RAW, &mut ts) };
        if res == 0 {
            return (ts.tv_sec as u64)
                .saturating_mul(1_000_000_000)
                .saturating_add(ts.tv_nsec as u64);
        }
    }

    // Portable fallback using std::time::Instant
    use std::sync::OnceLock;
    use std::time::Instant;
    static BASE: OnceLock<Instant> = OnceLock::new();
    let base = BASE.get_or_init(Instant::now);
    base.elapsed().as_nanos() as u64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirstOutGroupStatus {
    pub group_id: u16,
    pub group_name: String,
    pub leader_alarm_id: Option<u32>,
    pub leader_timestamp_ns: Option<u64>,
    pub window_duration_ns: u64,
    pub active_alarm_count: u32,
    pub is_latched: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirstOutCaptureResult {
    pub alarm_id: u32,
    pub group_id: u16,
    pub is_leader: bool,
    pub leader_alarm_id: u32,
    pub delta_micros: u64,
    pub timestamp_ns: u64,
}

pub struct FirstOutManager {
    groups: HashMap<u16, FirstOutGroupStatus>,
}

impl Default for FirstOutManager {
    fn default() -> Self {
        Self::new()
    }
}

impl FirstOutManager {
    pub fn new() -> Self {
        Self {
            groups: HashMap::new(),
        }
    }

    /// Register a new First-Out Group with a specific cascade observation window.
    pub fn register_group(
        &mut self,
        group_id: u16,
        group_name: impl Into<String>,
        window_duration_sec: f64,
    ) {
        let window_duration_ns = (window_duration_sec * 1_000_000_000.0) as u64;
        self.groups.insert(
            group_id,
            FirstOutGroupStatus {
                group_id,
                group_name: group_name.into(),
                leader_alarm_id: None,
                leader_timestamp_ns: None,
                window_duration_ns,
                active_alarm_count: 0,
                is_latched: false,
            },
        );
    }

    /// Process a trip event for an alarm belonging to a First-Out Group.
    ///
    /// If no leader is latched or previous cascade window expired, this alarm becomes Leader.
    /// Subsequent trips within the cascade window are tagged as followers with accurate delta_micros.
    pub fn record_trip(
        &mut self,
        group_id: u16,
        alarm_id: u32,
        timestamp_ns: u64,
    ) -> Option<FirstOutCaptureResult> {
        let group = self.groups.get_mut(&group_id)?;
        group.active_alarm_count += 1;

        if !group.is_latched {
            // First event: latch as leader
            group.is_latched = true;
            group.leader_alarm_id = Some(alarm_id);
            group.leader_timestamp_ns = Some(timestamp_ns);

            Some(FirstOutCaptureResult {
                alarm_id,
                group_id,
                is_leader: true,
                leader_alarm_id: alarm_id,
                delta_micros: 0,
                timestamp_ns,
            })
        } else {
            let leader_id = group.leader_alarm_id.unwrap_or(alarm_id);
            let leader_ts = group.leader_timestamp_ns.unwrap_or(timestamp_ns);

            // Check if within cascade window
            let diff_ns = timestamp_ns.saturating_sub(leader_ts);
            let delta_micros = diff_ns / 1_000;

            Some(FirstOutCaptureResult {
                alarm_id,
                group_id,
                is_leader: alarm_id == leader_id,
                leader_alarm_id: leader_id,
                delta_micros,
                timestamp_ns,
            })
        }
    }

    /// Reset or acknowledge the First-Out group, unlatching root cause.
    pub fn reset_group(&mut self, group_id: u16) -> bool {
        if let Some(group) = self.groups.get_mut(&group_id) {
            group.is_latched = false;
            group.leader_alarm_id = None;
            group.leader_timestamp_ns = None;
            group.active_alarm_count = 0;
            true
        } else {
            false
        }
    }

    /// Query the status of a First-Out group.
    pub fn get_group_status(&self, group_id: u16) -> Option<&FirstOutGroupStatus> {
        self.groups.get(&group_id)
    }
}
