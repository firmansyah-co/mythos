//! Field I/O configuration: which drivers run, against what, and how often.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use thiserror::Error;

/// File name of the I/O configuration inside a project directory.
pub const IO_CONFIG_FILE: &str = "io.json";

/// Smallest accepted poll interval.
pub const MIN_POLL_INTERVAL_MS: u64 = 10;

#[derive(Debug, Error)]
pub enum IoConfigError {
    #[error("cannot read {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("{path} is not a valid I/O configuration: {source}")]
    Parse {
        path: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("invalid I/O configuration: {0}")]
    Invalid(String),
}

/// One connection to a field device or gateway, served by one driver instance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionConfig {
    /// Name of this connection, unique within the project.
    pub id: String,
    /// Registered driver name, for example `"modbus"`.
    pub driver: String,
    /// How often the driver is polled. Required: there is no interval that is right for a
    /// device nobody described.
    pub poll_interval_ms: u64,
    /// Longest a single poll cycle may run before every tag of the connection is marked as a
    /// communication failure and the driver is restarted. Defaults to five poll intervals, and
    /// never less than two seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle_timeout_ms: Option<u64>,
    /// Driver-specific configuration, validated by the driver.
    pub config: serde_json::Value,
}

impl ConnectionConfig {
    /// The cycle deadline in effect for this connection.
    pub fn effective_cycle_timeout_ms(&self) -> u64 {
        self.cycle_timeout_ms
            .unwrap_or_else(|| self.poll_interval_ms.saturating_mul(5).max(2_000))
    }
}

/// The I/O configuration of a project (`io.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct IoConfig {
    #[serde(default)]
    pub connections: Vec<ConnectionConfig>,
}

impl IoConfig {
    /// Load and validate a configuration file. `Ok(None)` when the file does not exist.
    pub fn load(path: &Path) -> Result<Option<Self>, IoConfigError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(IoConfigError::Read {
                    path: path.display().to_string(),
                    source,
                })
            }
        };

        let config: Self = serde_json::from_str(&text).map_err(|source| IoConfigError::Parse {
            path: path.display().to_string(),
            source,
        })?;
        config.validate()?;
        Ok(Some(config))
    }

    /// Check the parts of the configuration the host owns. Each driver validates its own
    /// `config` object when its connection is started.
    pub fn validate(&self) -> Result<(), IoConfigError> {
        let mut ids = HashSet::new();
        for connection in &self.connections {
            if connection.id.trim().is_empty() {
                return Err(IoConfigError::Invalid(
                    "a connection has an empty id".into(),
                ));
            }
            if crate::runtimes::is_runtime_connection(&connection.id) {
                return Err(IoConfigError::Invalid(format!(
                    "connection id '{}' starts with '{}', which names a runtime",
                    connection.id,
                    crate::runtimes::CONNECTION_PREFIX
                )));
            }
            if !ids.insert(connection.id.as_str()) {
                return Err(IoConfigError::Invalid(format!(
                    "connection id '{}' is used more than once",
                    connection.id
                )));
            }
            if connection.poll_interval_ms < MIN_POLL_INTERVAL_MS {
                return Err(IoConfigError::Invalid(format!(
                    "connection '{}': poll_interval_ms {} is below the minimum of {}",
                    connection.id, connection.poll_interval_ms, MIN_POLL_INTERVAL_MS
                )));
            }
            if connection.cycle_timeout_ms == Some(0) {
                return Err(IoConfigError::Invalid(format!(
                    "connection '{}': cycle_timeout_ms must be greater than zero",
                    connection.id
                )));
            }
        }
        Ok(())
    }
}
