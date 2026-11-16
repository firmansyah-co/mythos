//! Installed versions, and published ones.

use crate::install;
use crate::release::{self, FeedError, Release};
use std::fs;
use std::path::Path;

pub struct VersionManager;

impl VersionManager {
    /// Versions installed in `<mythos dir>/versions`, oldest first. A directory left by an
    /// install that did not finish is not a version.
    pub fn list_local_versions(mythos_dir: &Path) -> Vec<String> {
        let Ok(entries) = fs::read_dir(install::versions_dir(mythos_dir)) else {
            return Vec::new();
        };
        let mut versions: Vec<String> = entries
            .flatten()
            .filter_map(|e| e.file_name().to_str().map(str::to_string))
            .filter(|name| !name.starts_with('.'))
            .filter(|name| install::is_installed(mythos_dir, name))
            .collect();
        versions.sort_by(
            |a, b| match (semver::Version::parse(a), semver::Version::parse(b)) {
                (Ok(a), Ok(b)) => a.cmp(&b),
                _ => a.cmp(b),
            },
        );
        versions
    }

    /// What the feed has published.
    pub fn fetch_remote_versions(feed_url: &str) -> Result<Vec<Release>, FeedError> {
        release::fetch(feed_url)
    }
}
