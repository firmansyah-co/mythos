//! Deadband Hysteresis, Time-Delay Filters and Chatter Suppression.
//!
//! Complies with ANSI/ISA-18.2 Section 5 & EEMUA 191 recommendations.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DeadbandConfig {
    Absolute(f64),
    SpanPercent { span: f64, percent: f64 },
}

impl DeadbandConfig {
    #[inline]
    pub fn delta(&self) -> f64 {
        match self {
            Self::Absolute(d) => d.max(0.0),
            Self::SpanPercent { span, percent } => span.abs() * (percent.max(0.0) / 100.0),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlarmFilterTimer {
    pub on_delay_ns: u64,
    pub off_delay_ns: u64,

    // Internal state
    raw_condition_start_ns: Option<u64>,
    raw_clear_start_ns: Option<u64>,
    filtered_active: bool,
}

impl AlarmFilterTimer {
    pub fn new(on_delay_ms: u32, off_delay_ms: u32) -> Self {
        Self {
            on_delay_ns: (on_delay_ms as u64) * 1_000_000,
            off_delay_ns: (off_delay_ms as u64) * 1_000_000,
            raw_condition_start_ns: None,
            raw_clear_start_ns: None,
            filtered_active: false,
        }
    }

    /// Evaluates raw condition with on-delay and off-delay filters.
    ///
    /// Returns true if filtered condition is active (trip sustained for on_delay_ns).
    pub fn update(&mut self, raw_active: bool, timestamp_ns: u64) -> bool {
        if raw_active {
            self.raw_clear_start_ns = None;
            if !self.filtered_active {
                match self.raw_condition_start_ns {
                    None => {
                        self.raw_condition_start_ns = Some(timestamp_ns);
                        if self.on_delay_ns == 0 {
                            self.filtered_active = true;
                        }
                    }
                    Some(start) => {
                        if timestamp_ns.saturating_sub(start) >= self.on_delay_ns {
                            self.filtered_active = true;
                        }
                    }
                }
            }
        } else {
            self.raw_condition_start_ns = None;
            if self.filtered_active {
                match self.raw_clear_start_ns {
                    None => {
                        self.raw_clear_start_ns = Some(timestamp_ns);
                        if self.off_delay_ns == 0 {
                            self.filtered_active = false;
                        }
                    }
                    Some(start) => {
                        if timestamp_ns.saturating_sub(start) >= self.off_delay_ns {
                            self.filtered_active = false;
                        }
                    }
                }
            }
        }

        self.filtered_active
    }

    pub fn is_filtered_active(&self) -> bool {
        self.filtered_active
    }
}

/// Dynamic Chatter Detector per EEMUA 191 recommendations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatterDetector {
    pub max_transitions_in_window: u16,
    pub window_duration_ns: u64,
    pub cooling_duration_ns: u64,

    // Internal state
    recent_transition_timestamps: Vec<u64>,
    is_suppressed: bool,
    suppression_started_ns: Option<u64>,
}

impl ChatterDetector {
    pub fn new(max_transitions: u16, window_seconds: u32, cooling_seconds: u32) -> Self {
        Self {
            max_transitions_in_window: max_transitions,
            window_duration_ns: (window_seconds as u64) * 1_000_000_000,
            cooling_duration_ns: (cooling_seconds as u64) * 1_000_000_000,
            recent_transition_timestamps: Vec::new(),
            is_suppressed: false,
            suppression_started_ns: None,
        }
    }

    /// Default configuration: 3 transitions in 60s -> suppress for 10 minutes cooling.
    pub fn default_industrial() -> Self {
        Self::new(3, 60, 600)
    }

    /// Record a state change transition (e.g., active <-> inactive).
    ///
    /// Returns true if chatter suppression status changed.
    pub fn record_transition(&mut self, timestamp_ns: u64) -> bool {
        // Prune old timestamps
        let cutoff = timestamp_ns.saturating_sub(self.window_duration_ns);
        self.recent_transition_timestamps.retain(|&t| t >= cutoff);
        self.recent_transition_timestamps.push(timestamp_ns);

        if !self.is_suppressed
            && self.recent_transition_timestamps.len() > (self.max_transitions_in_window as usize)
        {
            self.is_suppressed = true;
            self.suppression_started_ns = Some(timestamp_ns);
            return true;
        }

        false
    }

    /// Check and update suppression cooling period.
    pub fn tick(&mut self, timestamp_ns: u64) -> bool {
        if self.is_suppressed {
            if let Some(start) = self.suppression_started_ns {
                if timestamp_ns.saturating_sub(start) >= self.cooling_duration_ns {
                    self.is_suppressed = false;
                    self.suppression_started_ns = None;
                    self.recent_transition_timestamps.clear();
                    return true; // Un-suppressed
                }
            }
        }
        false
    }

    pub fn is_chatter_suppressed(&self) -> bool {
        self.is_suppressed
    }
}
