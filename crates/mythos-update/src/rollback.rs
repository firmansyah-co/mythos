//! `mythos rollback`: make the previously current version current again.
//!
//! Nothing is downloaded: the previous version is still installed beside the current one, so
//! rolling back moves the `current` link and nothing else.

use crate::config::{CliConfig, CliConfigError};
use crate::install::{self, InstallError};
use std::path::Path;
use std::time::Instant;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RollbackError {
    #[error("Configuration error: {0}")]
    Config(#[from] CliConfigError),

    #[error(transparent)]
    Install(#[from] InstallError),

    #[error("No previous version available to roll back to")]
    NoPreviousVersion,

    #[error("No version is current, so there is nothing to roll back from")]
    NoCurrentVersion,
}

#[derive(Debug, Clone)]
pub struct RollbackResult {
    pub from_version: String,
    pub to_version: String,
    pub duration_ms: f64,
}

pub struct RollbackManager;

impl RollbackManager {
    pub fn rollback(mythos_dir: &Path) -> Result<RollbackResult, RollbackError> {
        let start = Instant::now();
        let mut config = CliConfig::load_from(mythos_dir)?;
        let previous = config
            .previous_version
            .clone()
            .ok_or(RollbackError::NoPreviousVersion)?;
        let current = config
            .current_version
            .clone()
            .ok_or(RollbackError::NoCurrentVersion)?;

        install::point_current(mythos_dir, &previous)?;
        config.previous_version = Some(current.clone());
        config.current_version = Some(previous.clone());
        config.save_to(mythos_dir)?;

        Ok(RollbackResult {
            from_version: current,
            to_version: previous,
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        })
    }
}
