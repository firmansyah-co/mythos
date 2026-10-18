//! Sub-second (<1.0s) atomic binary rollback engine.

use crate::config::{CliConfig, CliConfigError};
use crate::update::{atomic_symlink, binary_filename};
use std::path::Path;
use std::time::Instant;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RollbackError {
    #[error("Configuration error: {0}")]
    Config(#[from] CliConfigError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("No previous version available to roll back to")]
    NoPreviousVersion,

    #[error("Target rollback version '{0}' directory does not exist")]
    VersionDirectoryMissing(String),
}

#[derive(Debug, Clone)]
pub struct RollbackResult {
    pub from_version: String,
    pub to_version: String,
    pub duration_ms: f64,
}

pub struct RollbackManager;

impl RollbackManager {
    /// Instantaneous atomic rollback (< 1.0s) to the previous cached binary version.
    pub fn rollback(mythos_dir: &Path) -> Result<RollbackResult, RollbackError> {
        let start = Instant::now();
        let mut config = CliConfig::load_from(mythos_dir)?;

        let prev = config
            .previous_version
            .clone()
            .ok_or(RollbackError::NoPreviousVersion)?;
        let curr = config
            .current_version
            .clone()
            .unwrap_or_else(|| "unknown".to_string());

        let target_version_dir = mythos_dir.join("versions").join(&prev);
        if !target_version_dir.exists() {
            return Err(RollbackError::VersionDirectoryMissing(prev));
        }

        let target_bin = target_version_dir.join("bin").join(binary_filename());

        // 1. Swap ~/.mythos/current symlink to previous version dir
        let current_link = mythos_dir.join("current");
        atomic_symlink(&target_version_dir, &current_link)?;

        // 2. Swap ~/.mythos/bin/mythos symlink to previous binary
        if target_bin.exists() {
            let mythos_bin_link = mythos_dir.join("bin").join(binary_filename());
            atomic_symlink(&target_bin, &mythos_bin_link)?;
        }

        // 3. Update configuration
        config.previous_version = Some(curr.clone());
        config.current_version = Some(prev.clone());
        config.save_to(mythos_dir)?;

        let duration = start.elapsed();
        let duration_ms = duration.as_secs_f64() * 1000.0;

        // Invariant check: MUST be sub-second (< 1000ms)
        if duration_ms >= 1000.0 {
            tracing::warn!(
                "Rollback duration exceeded sub-second SLA: {:.2}ms",
                duration_ms
            );
        }

        Ok(RollbackResult {
            from_version: curr,
            to_version: prev,
            duration_ms,
        })
    }
}
