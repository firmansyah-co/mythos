//! Real-time cyclic scan thread and runtime coordinator (TSK-103 / SYS-REQ-021).

use crate::error::StrucppError;
use crate::hot_reload::HotReloadManager;
use crate::quality_gate::QualityGate;
use crate::scheduler::StagedSchedulerConfig;
use crate::sync::TagSynchronizer;
use mythos_pal::scheduler::LinuxScheduler;
use mythos_pal::traits::PlatformScheduler;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// Jitter and execution metrics for real-time monitoring.
#[derive(Debug, Clone, Default)]
pub struct RuntimeMetrics {
    pub total_cycles: u64,
    pub last_cycle_duration: Duration,
    pub max_cycle_duration: Duration,
    pub last_jitter_nanos: i64,
    pub max_jitter_nanos: i64,
}

/// Real-Time Soft-Logic Runtime Engine.
pub struct StrucppRuntime {
    scheduler_config: Arc<StagedSchedulerConfig>,
    hot_reload: Arc<HotReloadManager>,
    quality_gate: Arc<QualityGate>,
    synchronizer: Arc<parking_lot::RwLock<TagSynchronizer>>,
    running: Arc<AtomicBool>,
    metrics: Arc<parking_lot::RwLock<RuntimeMetrics>>,
    thread_handle: parking_lot::Mutex<Option<JoinHandle<()>>>,
}

impl StrucppRuntime {
    pub fn new(
        scheduler_config: Arc<StagedSchedulerConfig>,
        hot_reload: Arc<HotReloadManager>,
        quality_gate: Arc<QualityGate>,
        synchronizer: Arc<parking_lot::RwLock<TagSynchronizer>>,
    ) -> Self {
        Self {
            scheduler_config,
            hot_reload,
            quality_gate,
            synchronizer,
            running: Arc::new(AtomicBool::new(false)),
            metrics: Arc::new(parking_lot::RwLock::new(RuntimeMetrics::default())),
            thread_handle: parking_lot::Mutex::new(None),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    pub fn metrics(&self) -> RuntimeMetrics {
        self.metrics.read().clone()
    }

    /// Step exactly one scan cycle synchronously (ideal for testing and deterministic ticks).
    pub fn step_once(&self) {
        let active_mod_guard = self.hot_reload.active_module();
        if let Some(ref module) = *active_mod_guard {
            module.step_cycle();
        }

        // Check staging handshakes
        if self.scheduler_config.is_promotion_pending() {
            self.scheduler_config.commit_promotion_at_boundary();
        }
        if self.hot_reload.is_swap_pending() {
            drop(active_mod_guard);
            let _ = self.hot_reload.commit_swap_at_boundary();
        }
    }

    /// Spawn the real-time execution thread.
    pub fn start(&self) -> Result<(), StrucppError> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Ok(()); // Already running
        }

        let running = Arc::clone(&self.running);
        let scheduler_config = Arc::clone(&self.scheduler_config);
        let hot_reload = Arc::clone(&self.hot_reload);
        let _quality_gate = Arc::clone(&self.quality_gate);
        let _synchronizer = Arc::clone(&self.synchronizer);
        let metrics = Arc::clone(&self.metrics);

        let handle = thread::Builder::new()
            .name("mythos-rt-scan".to_string())
            .spawn(move || {
                let init_cfg = scheduler_config.get_active();

                // 1. Elevate real-time priority (graceful non-root fallback)
                if let Err(e) = LinuxScheduler::set_realtime_priority(init_cfg.priority) {
                    warn!(
                        "Real-time priority elevation to {} skipped/failed: {}",
                        init_cfg.priority, e
                    );
                } else {
                    info!(
                        "Successfully elevated thread to SCHED_FIFO priority {}",
                        init_cfg.priority
                    );
                }

                // 2. Core affinity pinning if requested
                if let Some(core) = init_cfg.core_affinity {
                    if let Err(e) = LinuxScheduler::pin_to_core(core) {
                        warn!("Core affinity pinning to core {} failed: {}", core, e);
                    } else {
                        info!("Pinned real-time scan thread to CPU core {}", core);
                    }
                }

                let mut next_cycle_start = Instant::now();

                while running.load(Ordering::Acquire) {
                    let active_cfg = scheduler_config.get_active();
                    let target_interval = active_cfg.interval;
                    let cycle_start = Instant::now();

                    // Measure jitter relative to expected target start
                    let jitter_nanos = cycle_start
                        .checked_duration_since(next_cycle_start)
                        .map(|d| d.as_nanos() as i64)
                        .unwrap_or_else(|| {
                            -(next_cycle_start.duration_since(cycle_start).as_nanos() as i64)
                        });

                    // 1. Scan step
                    {
                        let active_mod_guard = hot_reload.active_module();
                        if let Some(ref module) = *active_mod_guard {
                            module.step_cycle();
                        }
                    }

                    // 2. Boundary Handshakes (Staging & Hot-Reload)
                    if scheduler_config.is_promotion_pending() {
                        scheduler_config.commit_promotion_at_boundary();
                    }
                    if hot_reload.is_swap_pending() {
                        let _ = hot_reload.commit_swap_at_boundary();
                    }

                    // 3. Update runtime metrics
                    let cycle_duration = cycle_start.elapsed();
                    {
                        let mut m = metrics.write();
                        m.total_cycles += 1;
                        m.last_cycle_duration = cycle_duration;
                        if cycle_duration > m.max_cycle_duration {
                            m.max_cycle_duration = cycle_duration;
                        }
                        m.last_jitter_nanos = jitter_nanos;
                        let abs_jitter = jitter_nanos.abs();
                        if abs_jitter > m.max_jitter_nanos {
                            m.max_jitter_nanos = abs_jitter;
                        }
                    }

                    // 4. Calculate next period and sleep
                    next_cycle_start += target_interval;
                    let now = Instant::now();
                    if next_cycle_start > now {
                        let remaining = next_cycle_start - now;
                        LinuxScheduler::sleep_nanos(remaining.as_nanos() as u64);
                    } else {
                        // Scan overrun
                        debug!("Cycle overrun: target cycle deadline elapsed");
                        next_cycle_start = now + target_interval;
                    }
                }

                info!("Real-time scan loop terminated cleanly");
            })
            .map_err(StrucppError::Io)?;

        *self.thread_handle.lock() = Some(handle);
        Ok(())
    }

    /// Stop the real-time execution loop.
    pub fn stop(&self) {
        if self.running.swap(false, Ordering::SeqCst) {
            let mut guard = self.thread_handle.lock();
            if let Some(handle) = guard.take() {
                let _ = handle.join();
            }
        }
    }
}

impl Drop for StrucppRuntime {
    fn drop(&mut self) {
        self.stop();
    }
}
