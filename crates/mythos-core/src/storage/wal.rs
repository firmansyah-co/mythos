//! Append-only crash-proof Write-Ahead Log (WAL) for project modifications.

use crc32fast::Hasher;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

pub const WAL_MAGIC: u32 = 0x4D595741; // "MYWA"

#[derive(Error, Debug)]
pub enum WalError {
    #[error("I/O error during WAL operation: {0}")]
    Io(#[from] std::io::Error),

    #[error("Corrupt WAL record! Checksum mismatch: expected {expected:#x}, got {actual:#x}")]
    ChecksumMismatch { expected: u32, actual: u32 },

    #[error("Invalid WAL magic: {0:#x}")]
    InvalidMagic(u32),

    #[error("Invalid UTF-8 in document path")]
    InvalidUtf8,
}

/// A parsed record from the project WAL.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectWalEntry {
    pub timestamp_us: u64,
    pub doc_path: String,
    pub payload: Vec<u8>,
}

pub struct ProjectWal {
    file_path: PathBuf,
}

impl ProjectWal {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            file_path: path.as_ref().to_path_buf(),
        }
    }

    /// Append a document update delta to the WAL and immediately sync to storage media.
    pub fn append(
        &self,
        doc_path: &str,
        payload: &[u8],
        timestamp_us: u64,
    ) -> Result<(), WalError> {
        if let Some(parent) = self.file_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)?;

        let doc_bytes = doc_path.as_bytes();
        let doc_len = doc_bytes.len() as u16;
        let payload_len = payload.len() as u32;

        // Calculate CRC32 over header and payload
        let mut hasher = Hasher::new();
        hasher.update(&timestamp_us.to_le_bytes());
        hasher.update(&doc_len.to_le_bytes());
        hasher.update(&payload_len.to_le_bytes());
        hasher.update(doc_bytes);
        hasher.update(payload);
        let crc = hasher.finalize();

        // Write binary record
        file.write_all(&WAL_MAGIC.to_le_bytes())?;
        file.write_all(&crc.to_le_bytes())?;
        file.write_all(&timestamp_us.to_le_bytes())?;
        file.write_all(&doc_len.to_le_bytes())?;
        file.write_all(&payload_len.to_le_bytes())?;
        file.write_all(doc_bytes)?;
        file.write_all(payload)?;

        // Ensure durability
        file.sync_data()?;

        Ok(())
    }

    /// Replay all valid records from the WAL.
    /// Trailing corrupt or incomplete records from power failure are automatically discarded.
    pub fn replay(&self) -> Result<Vec<ProjectWalEntry>, WalError> {
        if !self.file_path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&self.file_path)?;
        let mut reader = BufReader::new(file);
        let mut entries = Vec::new();

        loop {
            // Read magic and CRC (8 bytes)
            let mut magic_crc = [0u8; 8];
            match reader.read_exact(&mut magic_crc) {
                Ok(()) => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    break;
                }
                Err(e) => return Err(WalError::Io(e)),
            }

            let magic = u32::from_le_bytes(magic_crc[0..4].try_into().unwrap());
            let expected_crc = u32::from_le_bytes(magic_crc[4..8].try_into().unwrap());

            if magic != WAL_MAGIC {
                // Invalid record encountered, stop replay cleanly at boundary
                break;
            }

            // Read fixed body header: timestamp_us (8), doc_len (2), payload_len (4) = 14 bytes
            let mut fixed_header = [0u8; 14];
            if reader.read_exact(&mut fixed_header).is_err() {
                // Incomplete record header at EOF, discard
                break;
            }

            let timestamp_us = u64::from_le_bytes(fixed_header[0..8].try_into().unwrap());
            let doc_len = u16::from_le_bytes(fixed_header[8..10].try_into().unwrap()) as usize;
            let payload_len = u32::from_le_bytes(fixed_header[10..14].try_into().unwrap()) as usize;

            let mut doc_buf = vec![0u8; doc_len];
            if reader.read_exact(&mut doc_buf).is_err() {
                // Incomplete doc path at EOF, discard
                break;
            }

            let mut payload_buf = vec![0u8; payload_len];
            if reader.read_exact(&mut payload_buf).is_err() {
                // Incomplete payload at EOF, discard
                break;
            }

            // Verify checksum
            let mut hasher = Hasher::new();
            hasher.update(&fixed_header);
            hasher.update(&doc_buf);
            hasher.update(&payload_buf);
            let calculated_crc = hasher.finalize();

            if calculated_crc != expected_crc {
                // Checksum failure: mid-write power loss corrupted this trailing record
                break;
            }

            let doc_path = String::from_utf8(doc_buf).map_err(|_| WalError::InvalidUtf8)?;

            entries.push(ProjectWalEntry {
                timestamp_us,
                doc_path,
                payload: payload_buf,
            });
        }

        Ok(entries)
    }

    /// Truncate WAL file to zero bytes after successful checkpoint flush.
    pub fn truncate(&self) -> Result<(), WalError> {
        if self.file_path.exists() {
            let file = OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&self.file_path)?;
            file.sync_all()?;
        }
        Ok(())
    }

    /// Check if WAL file exists and has size > 0.
    pub fn has_pending_entries(&self) -> bool {
        self.file_path
            .metadata()
            .map(|m| m.len() > 0)
            .unwrap_or(false)
    }

    /// Returns path to WAL file.
    pub fn path(&self) -> &Path {
        &self.file_path
    }
}
