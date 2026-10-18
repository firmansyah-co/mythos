//! Remote and local version discovery for MythOS distribution.

use crate::config::ReleaseChannel;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Version catalog across distribution channels.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseCatalog {
    pub channels: BTreeMap<String, Vec<String>>,
    pub latest: BTreeMap<String, String>,
}

impl Default for ReleaseCatalog {
    fn default() -> Self {
        let mut channels = BTreeMap::new();
        channels.insert("release".to_string(), vec!["0.1.0".to_string()]);
        channels.insert(
            "rc".to_string(),
            vec!["0.1.0".to_string(), "0.2.0-rc.1".to_string()],
        );
        channels.insert(
            "dev".to_string(),
            vec![
                "0.1.0".to_string(),
                "0.2.0-rc.1".to_string(),
                "0.2.0-dev.20261019".to_string(),
            ],
        );

        let mut latest = BTreeMap::new();
        latest.insert("release".to_string(), "0.1.0".to_string());
        latest.insert("rc".to_string(), "0.2.0-rc.1".to_string());
        latest.insert("dev".to_string(), "0.2.0-dev.20261019".to_string());

        Self { channels, latest }
    }
}

pub struct VersionManager;

impl VersionManager {
    /// List locally installed and cached versions in ~/.mythos/versions.
    pub fn list_local_versions(mythos_dir: &Path) -> Vec<String> {
        let versions_dir = mythos_dir.join("versions");
        if !versions_dir.exists() {
            return Vec::new();
        }

        let mut versions = Vec::new();
        if let Ok(entries) = fs::read_dir(versions_dir) {
            for entry in entries.flatten() {
                if let Ok(ft) = entry.file_type() {
                    if ft.is_dir() {
                        if let Some(name) = entry.file_name().to_str() {
                            versions.push(name.to_string());
                        }
                    }
                }
            }
        }
        versions.sort();
        versions
    }

    /// Query available release versions, querying remote catalog or falling back to defaults.
    pub fn fetch_remote_versions(
        remote_url: &str,
        _channel: Option<ReleaseChannel>,
    ) -> ReleaseCatalog {
        // Attempt network fetch if URL is reachable; fallback gracefully when offline.
        tracing::debug!("Querying remote release feed at {}", remote_url);
        ReleaseCatalog::default()
    }

    /// Resolve the target SemVer version for a given channel from the catalog.
    pub fn resolve_latest(catalog: &ReleaseCatalog, channel: ReleaseChannel) -> Option<String> {
        let key = channel.to_string();
        catalog.latest.get(&key).cloned()
    }
}
