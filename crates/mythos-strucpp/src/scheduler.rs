//! Real-time cyclic task scheduling and staging configuration isolation (SYS-REQ-080).

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

/// Cyclic task execution schedule parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskScheduleConfig {
    /// Task name identifier (e.g. "TASK0")
    pub name: String,
    /// Cyclic scan interval
    pub interval: Duration,
    /// Real-time scheduling priority (e.g., 80 for SCHED_FIFO)
    pub priority: u32,
    /// CPU core affinity pinning (optional)
    pub core_affinity: Option<usize>,
}

impl Default for TaskScheduleConfig {
    fn default() -> Self {
        Self {
            name: "TASK0".into(),
            interval: Duration::from_millis(10),
            priority: 80,
            core_affinity: None,
        }
    }
}

/// Dual-state staging coordinator isolating Edit-time draft modifications
/// from the live running real-time execution loop (SYS-REQ-080).
pub struct StagedSchedulerConfig {
    active_config: parking_lot::RwLock<TaskScheduleConfig>,
    draft_config: parking_lot::RwLock<TaskScheduleConfig>,
    pending_promotion: Arc<AtomicBool>,
}

impl StagedSchedulerConfig {
    pub fn new(initial: TaskScheduleConfig) -> Self {
        Self {
            active_config: parking_lot::RwLock::new(initial.clone()),
            draft_config: parking_lot::RwLock::new(initial),
            pending_promotion: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Read the currently active execution schedule used by the real-time thread.
    pub fn get_active(&self) -> TaskScheduleConfig {
        self.active_config.read().clone()
    }

    /// Read the current draft schedule being edited in Workbench / Monaco canvas.
    pub fn get_draft(&self) -> TaskScheduleConfig {
        self.draft_config.read().clone()
    }

    /// Mutate the draft schedule. Does NOT affect the active running loop.
    pub fn update_draft<F>(&self, update_fn: F)
    where
        F: FnOnce(&mut TaskScheduleConfig),
    {
        let mut draft = self.draft_config.write();
        update_fn(&mut draft);
    }

    /// Flag promotion handshake readiness.
    pub fn request_promotion(&self) {
        self.pending_promotion.store(true, Ordering::Release);
    }

    /// Check if promotion is requested.
    pub fn is_promotion_pending(&self) -> bool {
        self.pending_promotion.load(Ordering::Acquire)
    }

    /// Promote draft configuration to active at a deterministic scan boundary.
    /// Returns true if promotion was applied.
    pub fn commit_promotion_at_boundary(&self) -> bool {
        if self.pending_promotion.swap(false, Ordering::AcqRel) {
            let draft = self.draft_config.read().clone();
            let mut active = self.active_config.write();
            info!(
                "Promoting draft schedule config to active: interval={:?}, priority={}",
                draft.interval, draft.priority
            );
            *active = draft;
            true
        } else {
            false
        }
    }
}
