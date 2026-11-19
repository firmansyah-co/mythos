//! Configuration and channel management for MythOS CLI.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Release channels supported by MythOS distribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ReleaseChannel {
    #[default]
    Release,
    Rc,
    Dev,
}

impl fmt::Display for ReleaseChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Release => write!(f, "release"),
            Self::Rc => write!(f, "rc"),
            Self::Dev => write!(f, "dev"),
        }
    }
}

impl std::str::FromStr for ReleaseChannel {
    type Err = CliConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "release" | "stable" => Ok(Self::Release),
            "rc" | "candidate" => Ok(Self::Rc),
            "dev" | "nightly" => Ok(Self::Dev),
            other => Err(CliConfigError::InvalidChannel(other.to_string())),
        }
    }
}

/// Errors related to CLI configuration and storage.
#[derive(Error, Debug)]
pub enum CliConfigError {
    #[error("Invalid channel '{0}'. Supported channels: release, rc, dev")]
    InvalidChannel(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Persistent configuration stored at `~/.mythos/config.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CliConfig {
    pub channel: ReleaseChannel,
    #[serde(default = "default_remote_url")]
    pub remote_url: String,
    pub current_version: Option<String>,
    pub previous_version: Option<String>,
    /// A version to stay on instead of following the channel. `mythos update` without a
    /// version installs this one.
    #[serde(default)]
    pub pinned_version: Option<String>,
}

/// Where releases are published: this repository's GitHub Releases.
pub const DEFAULT_RELEASE_FEED: &str =
    "https://api.github.com/repos/firmansyah-co/mythos/releases?per_page=100";

/// A feed earlier versions of this CLI wrote into `config.json`. Nothing was ever served there.
const NEVER_SERVED_FEED: &str = "https://releases.mythos.firmansyah.co";

/// Overrides the feed for one invocation, without touching `config.json`.
pub const RELEASE_FEED_ENV: &str = "MYTHOS_RELEASE_FEED";

fn default_remote_url() -> String {
    DEFAULT_RELEASE_FEED.to_string()
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            channel: ReleaseChannel::default(),
            remote_url: default_remote_url(),
            current_version: None,
            previous_version: None,
            pinned_version: None,
        }
    }
}

impl CliConfig {
    /// Return the default base MythOS directory: `~/.mythos`.
    pub fn default_mythos_dir() -> PathBuf {
        if let Ok(override_dir) = std::env::var("MYTHOS_HOME") {
            return PathBuf::from(override_dir);
        }
        if let Some(home) = dirs_fallback() {
            home.join(".mythos")
        } else {
            PathBuf::from(".mythos")
        }
    }

    /// The release feed to ask: `MYTHOS_RELEASE_FEED` if set, else the configured one. A
    /// configuration still naming the address earlier versions invented gets the real feed.
    pub fn feed_url(&self) -> String {
        if let Ok(url) = std::env::var(RELEASE_FEED_ENV) {
            if !url.trim().is_empty() {
                return url;
            }
        }
        if self.remote_url.trim_end_matches('/') == NEVER_SERVED_FEED {
            return default_remote_url();
        }
        self.remote_url.clone()
    }

    /// Load config from given mythos directory.
    pub fn load_from(mythos_dir: &Path) -> Result<Self, CliConfigError> {
        let config_file = mythos_dir.join("config.json");
        if !config_file.exists() {
            return Ok(Self::default());
        }
        let data = fs::read_to_string(&config_file)?;
        let cfg: Self = serde_json::from_str(&data)?;
        Ok(cfg)
    }

    /// Save config to given mythos directory.
    pub fn save_to(&self, mythos_dir: &Path) -> Result<(), CliConfigError> {
        fs::create_dir_all(mythos_dir)?;
        let config_file = mythos_dir.join("config.json");
        let json = serde_json::to_string_pretty(self)?;
        fs::write(config_file, json)?;
        Ok(())
    }
}

fn dirs_fallback() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}
