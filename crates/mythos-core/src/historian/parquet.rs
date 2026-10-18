//! Apache Parquet Chunking & Hive-Partitioned Storage for Process Telemetry.
//!
//! Complies with SYS-REQ-065 and ADR-007.
//! Writes daily Parquet chunks using Zstandard compression.

use arrow::array::{ArrayRef, Float64Array, Int16Array, Int32Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;
use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, File};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParquetHistorianError {
    #[error("Arrow error: {0}")]
    Arrow(#[from] arrow::error::ArrowError),
    #[error("Parquet error: {0}")]
    Parquet(#[from] parquet::errors::ParquetError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryRecord {
    pub timestamp_ns: i64,
    pub tag_id: i32,
    pub tag_name: String,
    pub value: f64,
    pub quality: i16,
    pub batch_id: Option<String>,
}

pub struct ParquetHistorianStorage {
    base_dir: PathBuf,
    schema: Arc<Schema>,
}

impl ParquetHistorianStorage {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        let schema = Arc::new(Schema::new(vec![
            Field::new(
                "timestamp_ns",
                DataType::Timestamp(TimeUnit::Nanosecond, Some("UTC".into())),
                false,
            ),
            Field::new("tag_id", DataType::Int32, false),
            Field::new("tag_name", DataType::Utf8, false),
            Field::new("value_double", DataType::Float64, false),
            Field::new("quality_code", DataType::Int16, false),
            Field::new("batch_id", DataType::Utf8, true),
        ]));

        Self {
            base_dir: base_dir.into(),
            schema,
        }
    }

    pub fn schema(&self) -> Arc<Schema> {
        self.schema.clone()
    }

    /// Converts a slice of telemetry records into an Arrow RecordBatch.
    pub fn build_record_batch(
        &self,
        records: &[TelemetryRecord],
    ) -> Result<RecordBatch, ParquetHistorianError> {
        let mut timestamps = Vec::with_capacity(records.len());
        let mut tag_ids = Vec::with_capacity(records.len());
        let mut tag_names = Vec::with_capacity(records.len());
        let mut values = Vec::with_capacity(records.len());
        let mut qualities = Vec::with_capacity(records.len());
        let mut batch_ids = Vec::with_capacity(records.len());

        for r in records {
            timestamps.push(r.timestamp_ns);
            tag_ids.push(r.tag_id);
            tag_names.push(r.tag_name.clone());
            values.push(r.value);
            qualities.push(r.quality);
            batch_ids.push(r.batch_id.clone());
        }

        let columns: Vec<ArrayRef> = vec![
            Arc::new(arrow::array::TimestampNanosecondArray::from(timestamps).with_timezone("UTC")),
            Arc::new(Int32Array::from(tag_ids)),
            Arc::new(StringArray::from(tag_names)),
            Arc::new(Float64Array::from(values)),
            Arc::new(Int16Array::from(qualities)),
            Arc::new(StringArray::from(batch_ids)),
        ];

        let batch = RecordBatch::try_new(self.schema.clone(), columns)?;
        Ok(batch)
    }

    /// Writes an Arrow RecordBatch to a Parquet file with Zstandard Level 3 compression.
    pub fn write_parquet_file(
        &self,
        output_path: &Path,
        batch: &RecordBatch,
    ) -> Result<u64, ParquetHistorianError> {
        if let Some(parent) = output_path.parent() {
            create_dir_all(parent)?;
        }

        let file = File::create(output_path)?;
        let props = WriterProperties::builder()
            .set_compression(Compression::ZSTD(
                parquet::basic::ZstdLevel::try_new(3).unwrap_or_default(),
            ))
            .build();

        let mut writer = ArrowWriter::try_new(file, batch.schema(), Some(props))?;
        writer.write(batch)?;
        writer.close()?;

        let metadata = std::fs::metadata(output_path)?;
        Ok(metadata.len())
    }

    /// Formats the Hive-partitioned directory path `year=YYYY/month=MM/day=DD/`.
    pub fn hive_partition_path(&self, year: u32, month: u32, day: u32) -> PathBuf {
        self.base_dir
            .join(format!("year={:04}", year))
            .join(format!("month={:02}", month))
            .join(format!("day={:02}", day))
    }
}
