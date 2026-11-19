//! What the server knows about its own version, its installation, and the published releases;
//! and installing one of them (Settings → Updates).
//!
//! An installation is the layout `mythos update` makes (`mythos_update::install`):
//! `<home>/versions/<version>/bin/mythos-server` and `<home>/current`. The server manages the
//! installation it runs from, or the one `MYTHOS_HOME` names. A server built from source runs
//! from no installation: it can show what has been published, but has nowhere to install it.
//!
//! Installing makes the new version current; the running server keeps running the version it
//! started as until it is restarted from `<home>/current/bin`. Nothing here restarts it.

use mythos_update::config::{CliConfig, ReleaseChannel};
use mythos_update::release::{self, Release, TARGET};
use mythos_update::{install, versions::VersionManager};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// How long a reading of the release feed is reused before it is asked again.
pub const FEED_CACHE: Duration = Duration::from_secs(300);

/// The environment variable naming the installation, as for the CLI.
pub const HOME_ENV: &str = "MYTHOS_HOME";

#[derive(Debug, Clone, Serialize)]
pub struct Running {
    /// The version this server was built as.
    pub version: String,
    /// The installed version it runs from, when it runs from an installation.
    pub installed_version: Option<String>,
    /// The platform it was built for, which picks the bundle to install.
    pub target: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Installation {
    pub home: String,
    pub current_version: Option<String>,
    pub previous_version: Option<String>,
    pub installed: Vec<String>,
    pub channel: ReleaseChannel,
    pub pinned_version: Option<String>,
    pub feed: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateStatus {
    pub running: Running,
    /// `None` when the server runs from no installation it can manage.
    pub installation: Option<Installation>,
    /// Why there is no installation, when there is none.
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReleaseView {
    pub version: String,
    pub tag: String,
    pub channel: ReleaseChannel,
    pub name: Option<String>,
    pub notes: Option<String>,
    pub published_at: Option<String>,
    pub url: Option<String>,
    /// Whether it publishes a bundle for the platform this server runs on.
    pub has_bundle: bool,
    pub installed: bool,
    pub current: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReleaseList {
    pub feed: String,
    pub fetched_unix_s: u64,
    /// The version the configured channel (or pin) would install, if any.
    pub target_version: Option<String>,
    pub releases: Vec<ReleaseView>,
}

struct Cached {
    feed: String,
    at: Instant,
    fetched_unix_s: u64,
    releases: Vec<Release>,
}

/// Updates for one server.
pub struct Updates {
    home: Option<PathBuf>,
    exe: Option<PathBuf>,
    /// The feed to read when there is no installation whose configuration names one.
    feed: Option<String>,
    cache: Mutex<Option<Cached>>,
    /// One install or rollback at a time.
    busy: tokio::sync::Mutex<()>,
}

fn now_unix_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `<home>` and the version when `exe` is `<home>/versions/<version>/bin/<name>`, or
/// `<home>/current/bin/<name>` (a path through the link, as macOS and Windows report it).
fn home_of(exe: &Path) -> Option<(PathBuf, String)> {
    let bin = exe.parent()?;
    if bin.file_name()? != "bin" {
        return None;
    }
    let version_dir = bin.parent()?;
    if version_dir.file_name()? == "current" {
        let home = version_dir.parent()?.to_path_buf();
        let version = install::current_version(&home)?;
        return Some((home, version));
    }
    let versions = version_dir.parent()?;
    if versions.file_name()? != "versions" {
        return None;
    }
    let version = version_dir.file_name()?.to_str()?.to_string();
    Some((versions.parent()?.to_path_buf(), version))
}

impl Updates {
    /// For this process: the installation `MYTHOS_HOME` names, else the one it runs from.
    pub fn detect() -> Self {
        let exe = std::env::current_exe().ok();
        let home = std::env::var_os(HOME_ENV)
            .map(PathBuf::from)
            .or_else(|| exe.as_deref().and_then(home_of).map(|(home, _)| home));
        Self::new(home, exe)
    }

    pub fn new(home: Option<PathBuf>, exe: Option<PathBuf>) -> Self {
        Self {
            home,
            exe,
            feed: None,
            cache: Mutex::new(None),
            busy: tokio::sync::Mutex::new(()),
        }
    }

    /// Read `feed` when there is no installation (whose configuration names its own feed).
    pub fn with_feed(mut self, feed: impl Into<String>) -> Self {
        self.feed = Some(feed.into());
        self
    }

    pub fn home(&self) -> Option<&Path> {
        self.home.as_deref()
    }

    fn config(&self) -> CliConfig {
        match self.home.as_deref() {
            Some(home) => CliConfig::load_from(home).unwrap_or_default(),
            None => CliConfig {
                remote_url: self
                    .feed
                    .clone()
                    .unwrap_or_else(|| CliConfig::default().remote_url),
                ..CliConfig::default()
            },
        }
    }

    pub fn status(&self) -> UpdateStatus {
        let running = Running {
            version: env!("CARGO_PKG_VERSION").to_string(),
            installed_version: self.exe.as_deref().and_then(home_of).map(|(_, v)| v),
            target: TARGET.to_string(),
        };
        match &self.home {
            None => UpdateStatus {
                running,
                installation: None,
                note: Some(format!(
                    "This server does not run from an installation ({}), so it has nowhere to \
                     install a release. Install one with the mythos CLI, or set {HOME_ENV}.",
                    self.exe
                        .as_deref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "its own path is unknown".into())
                )),
            },
            Some(home) => {
                let config = self.config();
                UpdateStatus {
                    running,
                    installation: Some(Installation {
                        home: home.display().to_string(),
                        current_version: install::current_version(home),
                        previous_version: config.previous_version.clone(),
                        installed: VersionManager::list_local_versions(home),
                        channel: config.channel,
                        pinned_version: config.pinned_version.clone(),
                        feed: config.feed_url(),
                    }),
                    note: None,
                }
            }
        }
    }

    /// The releases the feed publishes, read again when `refresh` or when the last reading is
    /// older than [`FEED_CACHE`].
    pub async fn releases(&self, refresh: bool) -> Result<ReleaseList, String> {
        let config = self.config();
        let feed = config.feed_url();
        let (releases, fetched_unix_s) = self.read_feed(&feed, refresh).await?;
        let home = self.home.clone();
        let current = home.as_deref().and_then(install::current_version);
        let installed = home
            .as_deref()
            .map(VersionManager::list_local_versions)
            .unwrap_or_default();
        let target_version = match &config.pinned_version {
            Some(pin) => Some(pin.clone()),
            None => release::on_channel(&releases, config.channel)
                .first()
                .map(|r| r.version.to_string()),
        };
        let mut views: Vec<ReleaseView> = releases
            .iter()
            .map(|r| {
                let version = r.version.to_string();
                ReleaseView {
                    has_bundle: r.bundle_for(TARGET).is_some(),
                    installed: installed.contains(&version),
                    current: current.as_deref() == Some(version.as_str()),
                    version,
                    tag: r.tag.clone(),
                    channel: r.channel,
                    name: r.name.clone(),
                    notes: r.notes.clone(),
                    published_at: r.published_at.clone(),
                    url: r.url.clone(),
                }
            })
            .collect();
        views.sort_by(|a, b| {
            semver::Version::parse(&b.version)
                .ok()
                .cmp(&semver::Version::parse(&a.version).ok())
        });
        Ok(ReleaseList {
            feed,
            fetched_unix_s,
            target_version,
            releases: views,
        })
    }

    async fn read_feed(&self, feed: &str, refresh: bool) -> Result<(Vec<Release>, u64), String> {
        if !refresh {
            let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(c) = cache.as_ref() {
                if c.feed == feed && c.at.elapsed() < FEED_CACHE {
                    return Ok((c.releases.clone(), c.fetched_unix_s));
                }
            }
        }
        let url = feed.to_string();
        let releases = tokio::task::spawn_blocking(move || release::fetch(&url))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let fetched_unix_s = now_unix_s();
        *self.cache.lock().unwrap_or_else(|e| e.into_inner()) = Some(Cached {
            feed: feed.to_string(),
            at: Instant::now(),
            fetched_unix_s,
            releases: releases.clone(),
        });
        Ok((releases, fetched_unix_s))
    }

    fn require_home(&self) -> Result<PathBuf, UpdatesError> {
        self.home
            .clone()
            .ok_or_else(|| UpdatesError::NoInstallation(self.status().note.unwrap_or_default()))
    }

    /// Follow `channel`, or stay on `pinned` when it is given.
    pub fn follow(
        &self,
        channel: ReleaseChannel,
        pinned: Option<&str>,
    ) -> Result<UpdateStatus, UpdatesError> {
        let home = self.require_home()?;
        let pinned = pinned
            .map(|v| {
                semver::Version::parse(v.trim().trim_start_matches('v'))
                    .map(|v| v.to_string())
                    .map_err(|_| UpdatesError::Refused(format!("'{v}' is not a version")))
            })
            .transpose()?;
        let mut config =
            CliConfig::load_from(&home).map_err(|e| UpdatesError::Failed(e.to_string()))?;
        config.channel = channel;
        config.pinned_version = pinned;
        config
            .save_to(&home)
            .map_err(|e| UpdatesError::Failed(e.to_string()))?;
        Ok(self.status())
    }

    /// Install `version` (or what the channel or pin says) and make it current.
    pub async fn install(&self, version: Option<String>) -> Result<Installed, UpdatesError> {
        let home = self.require_home()?;
        let _one_at_a_time = self.busy.try_lock().map_err(|_| {
            UpdatesError::Refused("an install or rollback is already running".into())
        })?;
        let config = self.config();
        let (releases, _) = self
            .read_feed(&config.feed_url(), true)
            .await
            .map_err(UpdatesError::Failed)?;
        let result = tokio::task::spawn_blocking(move || {
            mythos_update::update::UpdateManager::update_from(&home, &releases, version)
        })
        .await
        .map_err(|e| UpdatesError::Failed(e.to_string()))?
        .map_err(|e| UpdatesError::Failed(e.to_string()))?;
        Ok(self.installed(result.version, result.reported))
    }

    /// Make the previous version current again.
    pub async fn rollback(&self) -> Result<Installed, UpdatesError> {
        let home = self.require_home()?;
        let _one_at_a_time = self.busy.try_lock().map_err(|_| {
            UpdatesError::Refused("an install or rollback is already running".into())
        })?;
        let result = tokio::task::spawn_blocking(move || {
            mythos_update::rollback::RollbackManager::rollback(&home)
        })
        .await
        .map_err(|e| UpdatesError::Failed(e.to_string()))?
        .map_err(|e| UpdatesError::Refused(e.to_string()))?;
        Ok(self.installed(result.to_version, None))
    }

    fn installed(&self, version: String, reported: Option<String>) -> Installed {
        let running = self.exe.as_deref().and_then(home_of).map(|(_, v)| v);
        Installed {
            restart_required: running.as_deref() != Some(version.as_str()),
            version,
            reported,
            status: self.status(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Installed {
    pub version: String,
    /// What the installed `mythos --version` printed, when it was downloaded now.
    pub reported: Option<String>,
    /// Whether the running server is another version: it runs the new one once restarted.
    pub restart_required: bool,
    pub status: UpdateStatus,
}

#[derive(Debug)]
pub enum UpdatesError {
    /// The server has no installation to change.
    NoInstallation(String),
    /// The request cannot be done as asked.
    Refused(String),
    /// It was tried and did not work.
    Failed(String),
}

impl std::fmt::Display for UpdatesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdatesError::NoInstallation(m)
            | UpdatesError::Refused(m)
            | UpdatesError::Failed(m) => f.write_str(m),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_server_in_an_installation_knows_its_home_and_version() {
        let exe = Path::new("/opt/mythos/versions/0.4.0/bin/mythos-server");
        assert_eq!(
            home_of(exe),
            Some((PathBuf::from("/opt/mythos"), "0.4.0".to_string()))
        );
        assert_eq!(home_of(Path::new("/repo/target/debug/mythos-server")), None);
        assert_eq!(
            home_of(Path::new("/opt/mythos/current/bin/mythos-server")),
            None
        );
    }
}
