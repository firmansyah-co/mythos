//! Append-Only Merkle-Chained JSONL Audit Ledger.
//!
//! Complies with TSK-505, SYS-REQ-067, and ADR-005.
//! Provides an immutable record of all setpoint writes, alarm actions, logic deploys,
//! and supervisory operations. Every entry forms an unbroken cryptographic SHA-256 hash chain.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Audit ledger errors.
#[derive(Debug, Error)]
pub enum AuditIntegrityError {
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error at index {index}: {reason}")]
    SerializationError { index: u64, reason: String },

    #[error("Tampered record detected at index {index}: expected prev_hash '{expected_prev_hash}', got '{actual_prev_hash}'")]
    TamperedPrevHash {
        index: u64,
        expected_prev_hash: String,
        actual_prev_hash: String,
    },

    #[error("Tampered record detected at index {index}: computed hash '{computed_hash}' does not match recorded hash '{recorded_hash}'")]
    TamperedRecordDetected {
        index: u64,
        computed_hash: String,
        recorded_hash: String,
    },

    #[error("Audit ledger sequence broken: expected index {expected}, got {actual}")]
    SequenceBroken { expected: u64, actual: u64 },
}

/// A single immutable audit entry in the Merkle-chained JSONL ledger.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditEntry {
    /// Monotonically increasing entry index (0 for genesis).
    pub index: u64,
    /// Nanoseconds since UNIX epoch when this action was committed.
    pub timestamp_ns: u64,
    /// Identifier of the caller (e.g. "agent:claude-01", "operator:dewa_bagus").
    pub caller_id: String,
    /// Role of the caller (e.g. "Engineer", "Operator", "Supervisor", "Admin").
    pub caller_role: String,
    /// Action verb (e.g. "write_tag", "ack_alarm", "shelve_alarm", "deploy_logic").
    pub action: String,
    /// Target resource URI or ID (e.g. "ns=1;s=Boiler.Temperature.SP", "ALM-BOILER-01").
    pub target: String,
    /// Action details, previous/new values, and operator_intent.
    pub payload: serde_json::Value,
    /// SHA-256 hex string of the previous entry (or 64 zeroes for genesis).
    pub prev_hash: String,
    /// SHA-256 hex string of this entry's canonical content + prev_hash.
    pub hash: String,
}

impl AuditEntry {
    pub const GENESIS_PREV_HASH: &'static str =
        "0000000000000000000000000000000000000000000000000000000000000000";

    /// Compute the deterministic SHA-256 hash for an entry given its data fields and prev_hash.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_hash(
        index: u64,
        timestamp_ns: u64,
        caller_id: &str,
        caller_role: &str,
        action: &str,
        target: &str,
        payload: &serde_json::Value,
        prev_hash: &str,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(index.to_be_bytes());
        hasher.update(timestamp_ns.to_be_bytes());
        hasher.update(caller_id.as_bytes());
        hasher.update(caller_role.as_bytes());
        hasher.update(action.as_bytes());
        hasher.update(target.as_bytes());
        // Deterministic JSON string for payload
        let payload_str = serde_json::to_string(payload).unwrap_or_default();
        hasher.update(payload_str.as_bytes());
        hasher.update(prev_hash.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Verify this entry's internal hash integrity.
    pub fn verify_hash(&self) -> bool {
        let expected = Self::compute_hash(
            self.index,
            self.timestamp_ns,
            &self.caller_id,
            &self.caller_role,
            &self.action,
            &self.target,
            &self.payload,
            &self.prev_hash,
        );
        self.hash == expected
    }
}

/// Thread-safe append-only Merkle-chained JSONL audit ledger.
pub struct MerkleAuditLedger {
    file_path: PathBuf,
    next_index: AtomicU64,
    last_hash: Mutex<String>,
    write_lock: Mutex<()>,
}

impl MerkleAuditLedger {
    /// Open or create an audit ledger at the specified file path.
    /// If the file already exists, it verifies integrity and restores chain state.
    pub fn open_or_create(path: impl Into<PathBuf>) -> Result<Self, AuditIntegrityError> {
        let file_path = path.into();

        if let Some(parent) = file_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let mut next_idx = 0u64;
        let mut last_h = AuditEntry::GENESIS_PREV_HASH.to_string();

        if file_path.exists() {
            let file = File::open(&file_path)?;
            let reader = BufReader::new(file);

            for (i, line) in reader.lines().enumerate() {
                let line_str = line?;
                if line_str.trim().is_empty() {
                    continue;
                }
                let entry: AuditEntry = serde_json::from_str(&line_str).map_err(|e| {
                    AuditIntegrityError::SerializationError {
                        index: i as u64,
                        reason: e.to_string(),
                    }
                })?;

                if entry.index != next_idx {
                    return Err(AuditIntegrityError::SequenceBroken {
                        expected: next_idx,
                        actual: entry.index,
                    });
                }

                if entry.prev_hash != last_h {
                    return Err(AuditIntegrityError::TamperedPrevHash {
                        index: entry.index,
                        expected_prev_hash: last_h,
                        actual_prev_hash: entry.prev_hash,
                    });
                }

                let computed = AuditEntry::compute_hash(
                    entry.index,
                    entry.timestamp_ns,
                    &entry.caller_id,
                    &entry.caller_role,
                    &entry.action,
                    &entry.target,
                    &entry.payload,
                    &entry.prev_hash,
                );

                if entry.hash != computed {
                    return Err(AuditIntegrityError::TamperedRecordDetected {
                        index: entry.index,
                        computed_hash: computed,
                        recorded_hash: entry.hash,
                    });
                }

                last_h = entry.hash;
                next_idx += 1;
            }
        }

        Ok(Self {
            file_path,
            next_index: AtomicU64::new(next_idx),
            last_hash: Mutex::new(last_h),
            write_lock: Mutex::new(()),
        })
    }

    /// Append a new audit entry into the ledger and atomically sync to disk.
    pub fn append(
        &self,
        caller_id: impl Into<String>,
        caller_role: impl Into<String>,
        action: impl Into<String>,
        target: impl Into<String>,
        payload: serde_json::Value,
    ) -> Result<AuditEntry, AuditIntegrityError> {
        let _guard = self.write_lock.lock().unwrap();
        let mut last_h_guard = self.last_hash.lock().unwrap();

        let index = self.next_index.fetch_add(1, Ordering::SeqCst);
        let timestamp_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        let caller_id = caller_id.into();
        let caller_role = caller_role.into();
        let action = action.into();
        let target = target.into();
        let prev_hash = last_h_guard.clone();

        let hash = AuditEntry::compute_hash(
            index,
            timestamp_ns,
            &caller_id,
            &caller_role,
            &action,
            &target,
            &payload,
            &prev_hash,
        );

        let entry = AuditEntry {
            index,
            timestamp_ns,
            caller_id,
            caller_role,
            action,
            target,
            payload,
            prev_hash,
            hash: hash.clone(),
        };

        // Append to file
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)?;

        let line =
            serde_json::to_string(&entry).map_err(|e| AuditIntegrityError::SerializationError {
                index,
                reason: e.to_string(),
            })?;
        writeln!(file, "{}", line)?;
        file.sync_data()?;

        *last_h_guard = hash;

        Ok(entry)
    }

    /// Mathematically verify the integrity of the entire JSONL hash chain from genesis to end.
    /// Returns Ok(()) if the chain is unbroken and zero records have been tampered with.
    pub fn verify_integrity(&self) -> Result<(), AuditIntegrityError> {
        Self::verify_file_integrity(&self.file_path)
    }

    /// Static validator to verify integrity of any JSONL audit ledger on disk.
    pub fn verify_file_integrity(path: &Path) -> Result<(), AuditIntegrityError> {
        if !path.exists() {
            return Ok(());
        }

        let file = File::open(path)?;
        let reader = BufReader::new(file);

        let mut expected_index = 0u64;
        let mut expected_prev_hash = AuditEntry::GENESIS_PREV_HASH.to_string();

        for (line_num, line) in reader.lines().enumerate() {
            let line_str = line?;
            if line_str.trim().is_empty() {
                continue;
            }

            let entry: AuditEntry = serde_json::from_str(&line_str).map_err(|e| {
                AuditIntegrityError::SerializationError {
                    index: line_num as u64,
                    reason: e.to_string(),
                }
            })?;

            if entry.index != expected_index {
                return Err(AuditIntegrityError::SequenceBroken {
                    expected: expected_index,
                    actual: entry.index,
                });
            }

            if entry.prev_hash != expected_prev_hash {
                return Err(AuditIntegrityError::TamperedPrevHash {
                    index: entry.index,
                    expected_prev_hash,
                    actual_prev_hash: entry.prev_hash,
                });
            }

            let computed = AuditEntry::compute_hash(
                entry.index,
                entry.timestamp_ns,
                &entry.caller_id,
                &entry.caller_role,
                &entry.action,
                &entry.target,
                &entry.payload,
                &entry.prev_hash,
            );

            if entry.hash != computed {
                return Err(AuditIntegrityError::TamperedRecordDetected {
                    index: entry.index,
                    computed_hash: computed,
                    recorded_hash: entry.hash,
                });
            }

            expected_prev_hash = entry.hash;
            expected_index += 1;
        }

        Ok(())
    }

    /// Get current total entry count.
    pub fn entry_count(&self) -> u64 {
        self.next_index.load(Ordering::SeqCst)
    }

    /// Read all entries currently in the ledger.
    pub fn read_entries(&self) -> Result<Vec<AuditEntry>, AuditIntegrityError> {
        if !self.file_path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&self.file_path)?;
        let reader = BufReader::new(file);
        let mut entries = Vec::new();

        for (i, line) in reader.lines().enumerate() {
            let line_str = line?;
            if line_str.trim().is_empty() {
                continue;
            }
            let entry: AuditEntry = serde_json::from_str(&line_str).map_err(|e| {
                AuditIntegrityError::SerializationError {
                    index: i as u64,
                    reason: e.to_string(),
                }
            })?;
            entries.push(entry);
        }

        Ok(entries)
    }
}
