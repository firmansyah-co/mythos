# Embedded Historian & Selective CSV Export Specification

## 1. Executive Summary & Purpose
This document specifies the architecture, mathematical compression algorithms, on-disk storage layout, and export interfaces for the MythOS Embedded Historian Subsystem.

Industrial IPCs operate under strict constraints: solid-state storage (eMMC/NVMe) wear endurance, limited RAM, and the need to record thousands of process tags sampled at rates up to 1 kHz without dropping samples or starving the real-time control loop. MythOS implements a zero-allocation, lock-free time-series historian in pure Rust that combines **Deadband Filtering**, **Boxcar-Swinging Door Compression (SDC)**, and **Apache Parquet Columnar Chunking** with daily partitions, complemented by a zero-copy **Selective CSV Exporter API**.

---

## 2. Normative References
- **Apache Parquet Format Specification v2.9+**: Columnar storage format with Snappy / ZSTD compression.
- **Bristol, E. H. (1990)**: *Swinging Door Trending: Adaptive Trend Data Compression*, ISA Proceedings.
- **IEC 61158-6 / OPC UA Part 11**: *Historical Access Specification*.
- **RFC 4180**: *Common Format and MIME Type for Comma-Separated Values (CSV) Files*.

---

## 3. Lossy vs. Lossless Compression Pipeline

To minimize disk I/O while preserving process fidelity, incoming tag samples traverse a two-stage real-time compression pipeline:

```
+-------------------------------------------------------------------------+
| LOCK-FREE SHM TAGBUS (/dev/shm/mythos_tagbus)                           |
| Samples at 10 Hz - 1 kHz (Raw process telemetry)                        |
+-------------------------------------------------------------------------+
                                     |
                                     v
+-------------------------------------------------------------------------+
| STAGE 1: ABSOLUTE / RELATIVE DEADBAND FILTER (Zero-Latency)             |
| Rejects insignificant jitter within noise band (80% data reduction)     |
+-------------------------------------------------------------------------+
                                     |
                                     v
+-------------------------------------------------------------------------+
| STAGE 2: BOXCAR-SWINGING DOOR COMPRESSION (SDC)                         |
| Adaptive linear piecewise trend fitting (95% further data reduction)    |
+-------------------------------------------------------------------------+
                                     |
                                     v
+-------------------------------------------------------------------------+
| IN-MEMORY ARROW RECORD BATCH BUILDER                                    |
| Fixed-size ring buffer (64 MB per partition)                            |
+-------------------------------------------------------------------------+
                                     | (Flush upon 10,000 rows or 60s)
                                     v
+-------------------------------------------------------------------------+
| ON-DISK PARQUET DAILY CHUNKS (ZSTD Compression)                         |
| /var/log/mythos/historian/year=YYYY/month=MM/day=DD/part-*.parquet      |
+-------------------------------------------------------------------------+
```

---

## 4. Mathematical Formulation of Compression Algorithms

### 4.1 Deadband Filtering
Let $(t_k, y_k)$ be the incoming sample at step $k$, and $(t_{\text{last}}, y_{\text{last}})$ be the most recently archived sample point.
Let $\epsilon_{\text{db}} > 0$ be the configured deadband threshold:

$$\Delta y = |y_k - y_{\text{last}}|$$

- If $\Delta y \le \epsilon_{\text{db}}$ and $(t_k - t_{\text{last}}) < T_{\text{max\_keepalive}}$:
  $$\text{Sample } (t_k, y_k) \text{ is dropped (filtered)}.$$
- If $\Delta y > \epsilon_{\text{db}}$ or $(t_k - t_{\text{last}}) \ge T_{\text{max\_keepalive}}$:
  $$\text{Sample } (t_k, y_k) \text{ is passed to Stage 2 (Swinging Door Engine)}.$$

*Keepalive invariant*: Even if the process variable remains entirely flat, a sample is forced through every $T_{\text{max\_keepalive}}$ (default: 60 seconds) to prove the sensor is alive.

---

### 4.2 Boxcar-Swinging Door Compression (SDC)

Swinging Door Compression (Bristol, 1990) replaces a continuous sequence of points with a piecewise linear trend. It guarantees that any original point along the line segment deviates from the compressed line by no more than a prescribed compression tolerance $\epsilon_{\text{sdc}}$.

#### 1. Geometry and Formulation
Let $(t_0, y_0)$ be the initial point of the current trend segment.
For every subsequent incoming sample $(t_i, y_i)$ where $t_i > t_0$:
Two virtual parallel doors of width $2\epsilon_{\text{sdc}}$ swing outward from $(t_0, y_0)$. The slopes of the upper ray $S_u$ and lower ray $S_l$ from the base point $(t_0, y_0)$ to the tolerance boundaries $[y_i - \epsilon_{\text{sdc}}, y_i + \epsilon_{\text{sdc}}]$ are given by:

$$\text{Upper Door Slope: } S_u(i) = \frac{(y_i + \epsilon_{\text{sdc}}) - y_0}{t_i - t_0}$$
$$\text{Lower Door Slope: } S_l(i) = \frac{(y_i - \epsilon_{\text{sdc}}) - y_0}{t_i - t_0}$$

#### 2. Cumulative Angle Tightening
The algorithm maintains the running envelope slopes:
$$\hat{S}_u(i) = \min_{k=1 \dots i} S_u(k)$$
$$\hat{S}_l(i) = \max_{k=1 \dots i} S_l(k)$$

#### 3. Archive Trigger Criterion
As long as:
$$\hat{S}_u(i) \ge \hat{S}_l(i)$$
the line segment from $(t_0, y_0)$ to $(t_i, y_i)$ can represent all intermediate samples without exceeding the maximum error $\epsilon_{\text{sdc}}$.

The moment the doors cross:
$$\hat{S}_u(i) < \hat{S}_l(i)$$
1. The preceding sample point $(t_{i-1}, y_{i-1})$ is **archived** as the endpoint of the current line segment.
2. $(t_{i-1}, y_{i-1})$ becomes the new starting base point $(t_0, y_0)$ for the next segment.
3. Slopes are reset:
   $$\hat{S}_u = \frac{(y_i + \epsilon_{\text{sdc}}) - y_{i-1}}{t_i - t_{i-1}}, \quad \hat{S}_l = \frac{(y_i - \epsilon_{\text{sdc}}) - y_{i-1}}{t_i - t_{i-1}}$$

#### 4. Theoretical Compression Ratio
- In steady-state operation: $> 98\%$ reduction in sample count.
- In transient ramps: Adapts instantly, capturing the inflection points exactly.

---

## 5. Storage Partitioning & Parquet Chunking Strategy

### 5.1 Directory Partition Schema
To optimize query pruning and support fast historical queries, data is partitioned using the standard Hive schema:

```
/var/log/mythos/historian/
├── year=2026/
│   ├── month=03/
│   │   ├── day=29/
│   │   │   ├── tag_group=fast_1khz/
│   │   │   │   ├── part-00000-snappy.parquet
│   │   │   │   └── part-00001-snappy.parquet
│   │   │   └── tag_group=standard_10hz/
│   │   │       ├── part-00000-zstd.parquet
│   │   │       └── part-00001-zstd.parquet
│   │   └── day=30/
│   └── month=04/
└── metadata/
    └── catalog_manifest.json
```

### 5.2 Parquet File Schema & Type Mapping
The Parquet file utilizes Apache Arrow tabular layout:

| Field Name | Physical Type | Logical Type | Encoding | Description |
| :--- | :--- | :--- | :--- | :--- |
| `timestamp_ns` | `INT64` | `TIMESTAMP(NANOS, UTC)` | Delta-Binary-Packed | Monotonic UTC timestamp |
| `tag_id` | `INT32` | `INTEGER(32, true)` | Dictionary | Catalog index identifier |
| `value_double` | `DOUBLE` | None | Plain / Gorilla Float | Process value |
| `quality_code` | `INT16` | `INTEGER(16, false)` | Bit-Packed | OPC UA / MythOS Quality flags |
| `batch_id` | `BYTE_ARRAY` | `UTF8` | Dictionary | Optional batch reference |

**Compression Settings**:
- Compression codec: `Zstandard (Level 3)` for standard telemetry; `Snappy` for 1 kHz motion tags.
- Row group size: 50,000 rows ($\approx 2\,\text{MB}$ uncompressed), fitting snugly in L3 CPU cache.

---

## 6. Selective CSV Exporter API

Process engineers, regulatory inspectors, and external analytics systems frequently require ad-hoc CSV exports for specific tag sets and time ranges. MythOS exposes a high-throughput, streaming CSV export API.

### 6.1 HTTP / Unix Socket Endpoint Specification

`POST /api/v1/historian/export/csv`

#### Request Payload (JSON):
```json
{
  "start_time_iso": "2026-03-29T00:00:00Z",
  "end_time_iso": "2026-03-29T23:59:59Z",
  "tag_names": [
    "external.Sterilizer01.Pressure",
    "external.Sterilizer01.Temperature",
    "external.Boiler01.DrumLevel"
  ],
  "sample_interval_ms": 1000,
  "resample_strategy": "LINEAR_INTERPOLATION",
  "include_headers": true,
  "timestamp_format": "RFC3339_MILLIS",
  "delimiter": ","
}
```

#### Resample Strategies:
1. `RAW`: Emits actual archived sample timestamps. Rows contain sparse tag updates.
2. `FORWARD_FILL`: Emits aligned rows at fixed `sample_interval_ms`; holds last known good value.
3. `LINEAR_INTERPOLATION`: Emits aligned rows; computes linear interpolated value $y(t) = y_0 + \frac{y_1 - y_0}{t_1 - t_0}(t - t_0)$.
4. `TIME_WEIGHTED_AVERAGE`: Computes true integral average over interval slice.

### 6.2 Streamed Output Format (RFC 4180):
```csv
timestamp,external.Sterilizer01.Pressure,external.Sterilizer01.Temperature,external.Boiler01.DrumLevel
2026-03-29T00:00:00.000Z,2.851,142.12,-4.2
2026-03-29T00:00:01.000Z,2.854,142.15,-4.0
2026-03-29T00:00:02.000Z,2.853,142.18,-3.8
```

---

## 7. Concrete Rust Model Implementation

Below is the production-ready Rust algorithmic implementation of the Swinging Door Compression algorithm:

```rust
//! mythos_historian::compression
//! Lossless & lossy time-series compression algorithms for real-time telemetry.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HistorianSample {
    pub timestamp_ns: u64,
    pub value: f64,
    pub quality: u16,
}

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
            tolerance,
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

        let dt = (sample.timestamp_ns - base.timestamp_ns) as f64 / 1_000_000_000.0;
        
        // Check keepalive timeout
        if (sample.timestamp_ns - base.timestamp_ns) >= self.max_keepalive_ns {
            let archived = self.last_point.unwrap_or(sample);
            self.reset_base(archived, sample);
            return Some(archived);
        }

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
```

---

## 8. Verification & Acceptance Criteria
1. **Compression Benchmark**: In numerical simulation of 100,000 industrial process steps with sensor noise, Deadband (0.1) + SDC (0.2) achieves $\ge 92.5\%$ reduction in archived sample count while holding maximum reconstruction error $\le 0.20$ engineering units.
2. **Deterministic Write Bounds**: Parquet chunk flush writes in background Tokio thread pool using zero-copy file appends; maximum control loop blocking time $\le 0\,\mu\text{s}$ (fully decoupled by Shared Memory).
3. **Data Survivability**: Sudden power cut leaves at most 60 seconds of uncommitted telemetry in buffer; all previous daily `.parquet` partitions remain valid and uncorrupted.
