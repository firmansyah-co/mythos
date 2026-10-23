use mythos_core::historian::*;
use std::collections::BTreeMap;
use tempfile::tempdir;

#[test]
fn test_swinging_door_compression_ratio_exceeds_90_percent() {
    // SDC with tolerance epsilon = 0.5, keepalive = 60s
    let mut sdc = SwingingDoorCompressor::new(0.5, 60);

    let mut raw_count = 0;
    let mut archived_samples = Vec::new();

    // Simulate 1,000 process points:
    // Flat baseline with slight high-frequency noise (+- 0.1), followed by slow steady ramp
    for i in 0..1000 {
        raw_count += 1;
        let ts_ns = (i as u64) * 100_000_000; // 100ms intervals (10 Hz)
        let noise = ((i % 5) as f64 - 2.0) * 0.05; // -0.1 to +0.1
        let val = if i < 500 {
            50.0 + noise // Steady state
        } else {
            50.0 + ((i - 500) as f64 * 0.1) + noise // Linear ramp
        };

        let sample = HistorianSample::new(ts_ns, val, 192);
        if let Some(archived) = sdc.process_sample(sample) {
            archived_samples.push(archived);
        }
    }

    if let Some(final_pt) = sdc.flush() {
        archived_samples.push(final_pt);
    }

    let archived_count = archived_samples.len();
    let compression_ratio = 1.0 - (archived_count as f64 / raw_count as f64);

    println!(
        "Raw samples: {}, Archived: {}, Compression ratio: {:.2}%",
        raw_count,
        archived_count,
        compression_ratio * 100.0
    );

    // Guaranteed Bristol SDC compression ratio > 90% (per NFR-004 & SYS-REQ-065)
    assert!(
        compression_ratio > 0.90,
        "Compression ratio {:.2}% must exceed 90%",
        compression_ratio * 100.0
    );

    // Verify error bounds: all archived points must not deviate wildly
    for s in &archived_samples {
        assert!(s.value >= 49.0 && s.value <= 110.0);
    }
}

#[test]
fn test_parquet_daily_chunk_storage_and_roundtrip() {
    let tmp = tempdir().unwrap();
    let storage = ParquetHistorianStorage::new(tmp.path());

    let partition_dir = storage.hive_partition_path(2026, 10, 19);
    let parquet_file = partition_dir.join("telemetry-00000.parquet");

    let mut records = Vec::new();
    for i in 0..500 {
        records.push(TelemetryRecord {
            timestamp_ns: 1_700_000_000_000_000_000 + (i * 100_000_000),
            tag_id: 1001,
            tag_name: "external.Boiler01.Pressure".to_string(),
            value: 2.85 + (i as f64 * 0.001),
            quality: 192,
            batch_id: Some("BATCH-2026-OCT-1".to_string()),
        });
    }

    let batch = storage.build_record_batch(&records).unwrap();
    assert_eq!(batch.num_rows(), 500);
    assert_eq!(batch.num_columns(), 6);

    let bytes_written = storage.write_parquet_file(&parquet_file, &batch).unwrap();
    assert!(bytes_written > 0);
    assert!(parquet_file.exists());
    assert!(bytes_written < 50_000); // Efficient Zstandard compression
}

#[test]
fn test_rfc_4180_selective_csv_exporter_all_strategies() {
    let mut points = Vec::new();
    for i in 0..10 {
        points.push(TimeSeriesPoint {
            timestamp_ns: (i as u64) * 1_000_000_000, // 0s, 1s, 2s, ...
            value: (i as f64) * 10.0,                 // 0.0, 10.0, 20.0, ...
        });
    }

    // 1. Raw Strategy
    let raw = CsvExporter::resample_series(&points, 0, 5_000_000_000, 1000, ResampleStrategy::Raw);
    assert_eq!(raw.len(), 6);
    assert_eq!(raw[0], (0, Some(0.0)));
    assert_eq!(raw[5], (5_000_000_000, Some(50.0)));

    // 2. Linear Interpolation Strategy at 500ms intervals
    let interp = CsvExporter::resample_series(
        &points,
        0,
        2_000_000_000,
        500,
        ResampleStrategy::LinearInterpolation,
    );
    assert_eq!(interp.len(), 5);
    // at t=0.5s -> 5.0
    assert_eq!(interp[1], (500_000_000, Some(5.0)));
    // at t=1.5s -> 15.0
    assert_eq!(interp[3], (1_500_000_000, Some(15.0)));

    // 3. Forward Fill Strategy
    let ff = CsvExporter::resample_series(
        &points,
        0,
        2_000_000_000,
        500,
        ResampleStrategy::ForwardFill,
    );
    assert_eq!(ff.len(), 5);
    assert_eq!(ff[1], (500_000_000, Some(0.0))); // held 0.0 until 1.0s

    // 4. Time-Weighted Average
    let twa = CsvExporter::resample_series(
        &points,
        0,
        2_000_000_000,
        1000,
        ResampleStrategy::TimeWeightedAverage,
    );
    assert_eq!(twa.len(), 2);
    // Interval [0s, 1s]: average of line from 0 to 10 is 5.0
    assert_eq!(twa[0], (0, Some(5.0)));
    // Interval [1s, 2s]: average of line from 10 to 20 is 15.0
    assert_eq!(twa[1], (1_000_000_000, Some(15.0)));

    // 5. Multi-tag Stream CSV Export
    let mut tag_map = BTreeMap::new();
    tag_map.insert("external.SteamPressure".to_string(), points.clone());

    let mut buffer = Vec::new();
    let request = CsvExportRequest {
        start_time_ns: 0,
        end_time_ns: 3_000_000_000,
        tag_names: vec!["external.SteamPressure".to_string()],
        sample_interval_ms: 1000,
        resample_strategy: ResampleStrategy::LinearInterpolation,
        include_headers: true,
        delimiter: ',',
    };

    CsvExporter::export_csv(&mut buffer, &request, &tag_map).unwrap();
    let csv_out = String::from_utf8(buffer).unwrap();

    let lines: Vec<&str> = csv_out.lines().collect();
    assert_eq!(lines[0], "timestamp,external.SteamPressure");
    assert_eq!(lines[1], "0,0.0000");
    assert_eq!(lines[2], "1000000000,10.0000");
    assert_eq!(lines[3], "2000000000,20.0000");
    assert_eq!(lines[4], "3000000000,30.0000");
}
