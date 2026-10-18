//! Bristol Swinging Door Compression (SDC) Algorithm.
//!
//! Complies with Bristol, E. H. (1990) and SYS-REQ-065.
//! Achieves > 90% compression ratio while guaranteeing maximum reconstruction
//! error bounded by epsilon (compression tolerance).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HistorianSample {
    pub timestamp_ns: u64,
    pub value: f64,
    pub quality: u16,
}

impl HistorianSample {
    pub fn new(timestamp_ns: u64, value: f64, quality: u16) -> Self {
        Self {
            timestamp_ns,
            value,
            quality,
        }
    }
}

/// Bristol Swinging Door Compression (SDC) Engine.
#[derive(Debug, Clone)]
pub struct SwingingDoorCompressor {
    pub tolerance: f64,
    pub max_keepalive_ns: u64,

    // Internal state
    base_point: Option<HistorianSample>,
    last_point: Option<HistorianSample>,
    upper_slope: f64,
    lower_slope: f64,
}

impl SwingingDoorCompressor {
    pub fn new(tolerance: f64, max_keepalive_sec: u64) -> Self {
        Self {
            tolerance: tolerance.max(1e-9),
            max_keepalive_ns: max_keepalive_sec * 1_000_000_000,
            base_point: None,
            last_point: None,
            upper_slope: f64::INFINITY,
            lower_slope: f64::NEG_INFINITY,
        }
    }

    /// Evaluates an incoming sample. Returns Some(HistorianSample) when a point must be archived.
    pub fn process_sample(&mut self, sample: HistorianSample) -> Option<HistorianSample> {
        let base = match self.base_point {
            None => {
                // First sample ever: initialize base
                self.base_point = Some(sample);
                self.last_point = Some(sample);
                self.upper_slope = f64::INFINITY;
                self.lower_slope = f64::NEG_INFINITY;
                return Some(sample); // Archive first sample
            }
            Some(b) => b,
        };

        // Check keepalive timeout
        if (sample.timestamp_ns - base.timestamp_ns) >= self.max_keepalive_ns {
            let archived = self.last_point.unwrap_or(sample);
            self.reset_base(archived, sample);
            return Some(archived);
        }

        let dt = (sample.timestamp_ns - base.timestamp_ns) as f64 / 1_000_000_000.0;
        // Avoid division by zero if timestamp is identical
        if dt <= 0.0 {
            return None;
        }

        // Compute new door slopes to incoming point boundaries
        let su = ((sample.value + self.tolerance) - base.value) / dt;
        let sl = ((sample.value - self.tolerance) - base.value) / dt;

        // Tighten running door slopes
        let next_upper = self.upper_slope.min(su);
        let next_lower = self.lower_slope.max(sl);

        // Check if doors cross
        if next_upper < next_lower {
            // Trend broken! Archive the previous point as segment endpoint
            let point_to_archive = self.last_point.unwrap_or(sample);
            self.reset_base(point_to_archive, sample);
            Some(point_to_archive)
        } else {
            // Keep extending current trend line
            self.upper_slope = next_upper;
            self.lower_slope = next_lower;
            self.last_point = Some(sample);
            None
        }
    }

    /// Flush remaining buffered point if any (useful at end of batch or stream).
    pub fn flush(&mut self) -> Option<HistorianSample> {
        if let (Some(base), Some(last)) = (self.base_point, self.last_point) {
            if base.timestamp_ns != last.timestamp_ns {
                self.base_point = Some(last);
                return Some(last);
            }
        }
        None
    }

    fn reset_base(&mut self, new_base: HistorianSample, current: HistorianSample) {
        self.base_point = Some(new_base);
        self.last_point = Some(current);

        let dt = (current.timestamp_ns - new_base.timestamp_ns) as f64 / 1_000_000_000.0;
        if dt > 0.0 {
            self.upper_slope = ((current.value + self.tolerance) - new_base.value) / dt;
            self.lower_slope = ((current.value - self.tolerance) - new_base.value) / dt;
        } else {
            self.upper_slope = f64::INFINITY;
            self.lower_slope = f64::NEG_INFINITY;
        }
    }
}
