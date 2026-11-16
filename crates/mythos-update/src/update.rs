//! `mythos update`: install a published release and make it current.

use crate::config::{CliConfig, CliConfigError, ReleaseChannel};
use crate::install::{self, InstallError};
use crate::release::{self, FeedError, Release};
use std::path::Path;
use std::time::Instant;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum UpdateError {
    #[error("Configuration error: {0}")]
    Config(#[from] CliConfigError),

    #[error(transparent)]
    Feed(#[from] FeedError),

    #[error(transparent)]
    Install(#[from] InstallError),

    #[error("nothing has been published on the {channel} channel at {feed}")]
    NothingPublished {
        channel: ReleaseChannel,
        feed: String,
    },

    #[error("version {version} has not been published at {feed}")]
    VersionNotPublished { version: String, feed: String },

    #[error("'{0}' is not a version (expected something like 1.2.0 or 1.2.0-rc.1)")]
    NotAVersion(String),
}

#[derive(Debug, Clone)]
pub struct UpdateResult {
    pub version: String,
    /// What the installed `mythos --version` printed, or `None` when the version was already
    /// installed and nothing was downloaded.
    pub reported: Option<String>,
    pub duration_ms: f64,
}

pub struct UpdateManager;

impl UpdateManager {
    /// Install `version`, or the pinned version, or the newest release on the configured
    /// channel, and make it current.
    pub fn update(mythos_dir: &Path, version: Option<String>) -> Result<UpdateResult, UpdateError> {
        let config = CliConfig::load_from(mythos_dir)?;
        let releases = release::fetch(&config.feed_url())?;
        Self::update_from(mythos_dir, &releases, version)
    }

    /// `update`, choosing among `releases` already read from the feed.
    pub fn update_from(
        mythos_dir: &Path,
        releases: &[Release],
        version: Option<String>,
    ) -> Result<UpdateResult, UpdateError> {
        let start = Instant::now();
        let mut config = CliConfig::load_from(mythos_dir)?;
        let feed = config.feed_url();
        let wanted = version.or_else(|| config.pinned_version.clone());
        let chosen = choose(releases, wanted.as_deref(), config.channel, &feed)?;
        let version = chosen.version.to_string();

        let reported = if install::is_installed(mythos_dir, &version) {
            None
        } else {
            Some(install::install(mythos_dir, chosen)?)
        };
        install::point_current(mythos_dir, &version)?;

        if config.current_version.as_deref() != Some(version.as_str()) {
            config.previous_version = config.current_version.take();
            config.current_version = Some(version.clone());
        }
        config.save_to(mythos_dir)?;

        Ok(UpdateResult {
            version,
            reported,
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        })
    }
}

/// The release to install: the one asked for, or the newest the channel follows.
fn choose<'a>(
    releases: &'a [Release],
    version: Option<&str>,
    channel: ReleaseChannel,
    feed: &str,
) -> Result<&'a Release, UpdateError> {
    match version {
        Some(asked) => {
            let wanted = semver::Version::parse(asked.trim().trim_start_matches('v'))
                .map_err(|_| UpdateError::NotAVersion(asked.to_string()))?;
            releases
                .iter()
                .find(|r| r.version == wanted)
                .ok_or_else(|| UpdateError::VersionNotPublished {
                    version: wanted.to_string(),
                    feed: feed.to_string(),
                })
        }
        None => release::on_channel(releases, channel)
            .into_iter()
            .next()
            .ok_or_else(|| UpdateError::NothingPublished {
                channel,
                feed: feed.to_string(),
            }),
    }
}

/// Whether what `mythos --version` printed names the release it was installed from. The
/// executable carries the base version of its tag (`1.2.0` for `1.2.0-rc.3`).
pub fn reports_release(reported: &str, version: &str) -> bool {
    match semver::Version::parse(version) {
        Ok(v) => reported
            .split_whitespace()
            .any(|word| word == format!("{}.{}.{}", v.major, v.minor, v.patch) || word == version),
        Err(_) => false,
    }
}

/// The file name of the `mythos` executable on this platform.
pub fn binary_filename() -> &'static str {
    if cfg!(windows) {
        "mythos.exe"
    } else {
        "mythos"
    }
}

#[cfg(test)]
mod tests {
    use super::reports_release;

    #[test]
    fn the_reported_version_is_checked_against_the_release() {
        assert!(reports_release("mythos 1.2.0", "1.2.0"));
        assert!(reports_release("mythos 1.2.0", "1.2.0-rc.3"));
        assert!(!reports_release("mythos 0.1.0", "1.2.0"));
        assert!(!reports_release("mythos 11.2.0", "1.2.0"));
    }
}
