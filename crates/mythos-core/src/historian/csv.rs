//! RFC 4180 Selective CSV Exporter API with Resampling Strategies.
//!
//! Complies with SYS-REQ-066 and ADR-007.
//! Supports Raw, Forward-Fill, Linear Interpolation, and Time-Weighted Average resampling.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CsvExporterError {
    #[error("IO error during CSV export: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid request parameters: {0}")]
    InvalidRequest(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResampleStrategy {
    Raw,
    ForwardFill,
    LinearInterpolation,
    TimeWeightedAverage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsvExportRequest {
    pub start_time_ns: u64,
    pub end_time_ns: u64,
    pub tag_names: Vec<String>,
    pub sample_interval_ms: u64,
    pub resample_strategy: ResampleStrategy,
    pub include_headers: bool,
    pub delimiter: char,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeSeriesPoint {
    pub timestamp_ns: u64,
    pub value: f64,
}

pub struct CsvExporter;

impl CsvExporter {
    /// Resamples a single tag's time-series points at uniform time steps [start_time_ns .. end_time_ns].
    pub fn resample_series(
        points: &[TimeSeriesPoint],
        start_ns: u64,
        end_ns: u64,
        interval_ms: u64,
        strategy: ResampleStrategy,
    ) -> Vec<(u64, Option<f64>)> {
        if points.is_empty() || end_ns < start_ns || interval_ms == 0 {
            return Vec::new();
        }

        let interval_ns = interval_ms * 1_000_000;
        let mut result = Vec::new();

        match strategy {
            ResampleStrategy::Raw => {
                for p in points {
                    if p.timestamp_ns >= start_ns && p.timestamp_ns <= end_ns {
                        result.push((p.timestamp_ns, Some(p.value)));
                    }
                }
            }
            ResampleStrategy::ForwardFill => {
                let mut current_ns = start_ns;
                let mut pt_idx = 0;
                let mut last_val: Option<f64> = None;

                while current_ns <= end_ns {
                    while pt_idx < points.len() && points[pt_idx].timestamp_ns <= current_ns {
                        last_val = Some(points[pt_idx].value);
                        pt_idx += 1;
                    }
                    result.push((current_ns, last_val));
                    current_ns = current_ns.saturating_add(interval_ns);
                }
            }
            ResampleStrategy::LinearInterpolation => {
                let mut current_ns = start_ns;
                while current_ns <= end_ns {
                    let val = Self::interpolate_point(points, current_ns);
                    result.push((current_ns, val));
                    current_ns = current_ns.saturating_add(interval_ns);
                }
            }
            ResampleStrategy::TimeWeightedAverage => {
                let mut current_ns = start_ns;
                while current_ns < end_ns {
                    let next_ns = (current_ns + interval_ns).min(end_ns);
                    let twa = Self::compute_twa(points, current_ns, next_ns);
                    result.push((current_ns, twa));
                    current_ns = next_ns;
                }
            }
        }

        result
    }

    fn interpolate_point(points: &[TimeSeriesPoint], target_ns: u64) -> Option<f64> {
        if points.is_empty() {
            return None;
        }
        if target_ns <= points[0].timestamp_ns {
            return Some(points[0].value);
        }
        if target_ns >= points[points.len() - 1].timestamp_ns {
            return Some(points[points.len() - 1].value);
        }

        // Binary search for surrounding points
        let idx = match points.binary_search_by_key(&target_ns, |p| p.timestamp_ns) {
            Ok(exact) => return Some(points[exact].value),
            Err(i) => i,
        };

        let p0 = &points[idx - 1];
        let p1 = &points[idx];
        let dt = (p1.timestamp_ns - p0.timestamp_ns) as f64;
        if dt <= 0.0 {
            return Some(p0.value);
        }
        let t = (target_ns - p0.timestamp_ns) as f64 / dt;
        Some(p0.value + t * (p1.value - p0.value))
    }

    fn compute_twa(points: &[TimeSeriesPoint], slice_start: u64, slice_end: u64) -> Option<f64> {
        let dt_total = (slice_end.saturating_sub(slice_start)) as f64;
        if dt_total <= 0.0 || points.is_empty() {
            return None;
        }

        let mut sum_area = 0.0;
        let mut t_prev = slice_start;
        let mut val_prev = Self::interpolate_point(points, slice_start)?;

        for p in points {
            if p.timestamp_ns <= slice_start {
                continue;
            }
            if p.timestamp_ns >= slice_end {
                break;
            }
            let dt = (p.timestamp_ns - t_prev) as f64;
            let val_curr = p.value;
            sum_area += ((val_prev + val_curr) / 2.0) * dt;
            t_prev = p.timestamp_ns;
            val_prev = val_curr;
        }

        let val_end = Self::interpolate_point(points, slice_end)?;
        let dt = (slice_end - t_prev) as f64;
        sum_area += ((val_prev + val_end) / 2.0) * dt;

        Some(sum_area / dt_total)
    }

    /// Stream exports aligned multiple tags into an RFC 4180 CSV writer.
    pub fn export_csv<W: Write>(
        writer: &mut W,
        request: &CsvExportRequest,
        tag_series: &BTreeMap<String, Vec<TimeSeriesPoint>>,
    ) -> Result<(), CsvExporterError> {
        let delim = request.delimiter;

        if request.include_headers {
            write!(writer, "timestamp")?;
            for tag in &request.tag_names {
                write!(writer, "{}{}", delim, tag)?;
            }
            writeln!(writer)?;
        }

        // Precompute resampled series per tag
        let mut resampled_map = BTreeMap::new();
        for tag in &request.tag_names {
            let pts = tag_series.get(tag).map(|v| v.as_slice()).unwrap_or(&[]);
            let res = Self::resample_series(
                pts,
                request.start_time_ns,
                request.end_time_ns,
                request.sample_interval_ms,
                request.resample_strategy,
            );
            resampled_map.insert(tag.clone(), res);
        }

        // Align by timestamp
        if let Some(first_tag) = request.tag_names.first() {
            if let Some(first_series) = resampled_map.get(first_tag) {
                for (row_idx, (ts, _)) in first_series.iter().enumerate() {
                    write!(writer, "{}", ts)?;
                    for tag in &request.tag_names {
                        if let Some(series) = resampled_map.get(tag) {
                            if let Some((_, val_opt)) = series.get(row_idx) {
                                match val_opt {
                                    Some(v) => write!(writer, "{}{:.4}", delim, v)?,
                                    None => write!(writer, "{}", delim)?,
                                }
                            } else {
                                write!(writer, "{}", delim)?;
                            }
                        }
                    }
                    writeln!(writer)?;
                }
            }
        }

        Ok(())
    }
}
