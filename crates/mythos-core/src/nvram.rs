//! NVRAM Write-Ahead Log (WAL) persistence engine for RETAIN and PERSISTENT memory tiers.

use bytemuck::{Pod, Zeroable};
use crc32fast::Hasher;
use mythos_shm::TagValueRaw;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

pub const WAL_MAGIC: u16 = 0x5741; // "WA"
pub const DEFAULT_NVRAM_PATH: &str = "/var/lib/mythos/nvram.dat";

#[derive(Error, Debug)]
pub enum NvramError {
    #[error("I/O error during NVRAM operation: {0}")]
    Io(#[from] std::io::Error),

    #[error("Corrupt WAL record detected! Checksum mismatch at LSN {lsn}: expected {expected:#x}, got {actual:#x}")]
    ChecksumMismatch {
        lsn: u64,
        expected: u32,
        actual: u32,
    },

    #[error("Invalid WAL record magic signature {magic:#x} at LSN {lsn}")]
    InvalidMagic { lsn: u64, magic: u16 },
}

/// Binary packed on-disk record header for retained tag entries.
#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(C, packed)]
pub struct WalRecordHeader {
    /// Magic byte signature: 0x5741 (ASCII "WA")
    pub magic: u16,

    /// Monotonic log sequence number (LSN)
    pub lsn: u64,

    /// Unique tag ID matching SHM catalog
    pub tag_id: u32,

    /// Timestamp of write in nanoseconds
    pub timestamp_ns: u64,

    /// 64-bit value payload
    pub value_raw: [u8; 8],

    /// CRC32 checksum over the preceding 30 bytes of this record
    pub record_crc32: u32,
}

unsafe impl Zeroable for WalRecordHeader {}
unsafe impl Pod for WalRecordHeader {}

const _: () = assert!(std::mem::size_of::<WalRecordHeader>() == 34);

impl WalRecordHeader {
    pub fn new(lsn: u64, tag_id: u32, timestamp_ns: u64, value_raw: [u8; 8]) -> Self {
        let mut record = Self {
            magic: WAL_MAGIC,
            lsn,
            tag_id,
            timestamp_ns,
            value_raw,
            record_crc32: 0,
        };
        record.record_crc32 = record.compute_crc32();
        record
    }

    pub fn compute_crc32(&self) -> u32 {
        let bytes = bytemuck::bytes_of(self);
        let mut hasher = Hasher::new();
        hasher.update(&bytes[..30]); // Preceding 30 bytes excluding CRC
        hasher.finalize()
    }

    pub fn is_valid_checksum(&self) -> bool {
        self.record_crc32 == self.compute_crc32()
    }
}

/// NVRAM Write-Ahead Log manager.
pub struct NvramWalEngine {
    file_path: PathBuf,
    current_lsn: u64,
}

impl NvramWalEngine {
    /// Create or attach to an NVRAM WAL file at the specified path.
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            file_path: path.as_ref().to_path_buf(),
            current_lsn: 0,
        }
    }

    /// Default production instance backed by `/var/lib/mythos/nvram.dat`.
    pub fn default_production() -> Self {
        Self::new(DEFAULT_NVRAM_PATH)
    }

    /// Append a retained tag update to the WAL file with immediate synchronous flush.
    pub fn append_record(
        &mut self,
        tag_id: u32,
        value: TagValueRaw,
        timestamp_ns: u64,
    ) -> Result<u64, NvramError> {
        if let Some(parent) = self.file_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)?;

        self.current_lsn += 1;
        let lsn = self.current_lsn;
        let record = WalRecordHeader::new(lsn, tag_id, timestamp_ns, unsafe { value.raw_bytes });
        let bytes = bytemuck::bytes_of(&record);

        file.write_all(bytes)?;
        file.sync_data()?;

        Ok(lsn)
    }

    /// Replay all valid records from the WAL file, reconstructing the latest state per tag.
    /// Incomplete records at the tail caused by ungraceful power outage are safely ignored.
    pub fn recover(&mut self) -> Result<Vec<(u32, TagValueRaw)>, NvramError> {
        if !self.file_path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&self.file_path)?;
        let mut reader = BufReader::new(file);
        let mut max_lsn = 0u64;
        let mut latest_state = std::collections::HashMap::new();
        let mut buffer = [0u8; std::mem::size_of::<WalRecordHeader>()];

        loop {
            match reader.read_exact(&mut buffer) {
                Ok(()) => {
                    let record: WalRecordHeader = bytemuck::pod_read_unaligned(&buffer);

                    if record.magic != WAL_MAGIC {
                        return Err(NvramError::InvalidMagic {
                            lsn: record.lsn,
                            magic: record.magic,
                        });
                    }

                    if !record.is_valid_checksum() {
                        return Err(NvramError::ChecksumMismatch {
                            lsn: record.lsn,
                            expected: record.compute_crc32(),
                            actual: record.record_crc32,
                        });
                    }

                    if record.lsn > max_lsn {
                        max_lsn = record.lsn;
                    }

                    let val = TagValueRaw {
                        raw_bytes: record.value_raw,
                    };
                    latest_state.insert(record.tag_id, val);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    // Clean end-of-file reached
                    break;
                }
                Err(e) => return Err(NvramError::Io(e)),
            }
        }

        self.current_lsn = max_lsn;
        let mut result: Vec<(u32, TagValueRaw)> = latest_state.into_iter().collect();
        result.sort_by_key(|&(id, _)| id);
        Ok(result)
    }

    /// Compact the WAL file by writing a compacted snapshot and atomically replacing the file.
    pub fn compact(&mut self) -> Result<usize, NvramError> {
        let state = self.recover()?;
        let parent = self.file_path.parent().unwrap_or_else(|| Path::new("."));
        let temp_path = parent.join(format!(".nvram_compact_{}.tmp", std::process::id()));

        {
            let temp_file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&temp_path)?;
            let mut writer = BufWriter::new(temp_file);

            let mut new_lsn = 0u64;
            for (tag_id, value) in &state {
                new_lsn += 1;
                let record = WalRecordHeader::new(new_lsn, *tag_id, 0, unsafe { value.raw_bytes });
                let bytes = bytemuck::bytes_of(&record);
                writer.write_all(bytes)?;
            }
            writer.flush()?;
            let file = writer.into_inner().map_err(|e| e.into_error())?;
            file.sync_all()?;
        }

        // Atomic file replacement
        std::fs::rename(&temp_path, &self.file_path)?;
        self.current_lsn = state.len() as u64;

        Ok(state.len())
    }

    /// Return current LSN.
    pub fn current_lsn(&self) -> u64 {
        self.current_lsn
    }
}
