//! Remote and local version discovery for MythOS distribution.
//!
//! # What is implemented, and what is not
//!
//! Only *local* discovery is implemented: [`VersionManager::list_local_versions`] reads
//! `~/.mythos/versions` from disk and reports what is actually there.
//!
//! *Remote* discovery is **not implemented**. There is no published release feed, this
//! crate contains no network client, and [`VersionManager::fetch_remote_versions`]
//! therefore returns an error rather than a catalogue. It used to return a hardcoded
//! list of version strings (`0.1.0`, `0.2.0-rc.1`, `0.2.0-dev.20261019`) while its
//! caller printed "Remote: <url>" to the operator — a remote that was never contacted.
//! Those literals are gone. `ReleaseCatalog` and [`VersionManager::resolve_latest`]
//! remain as the shape a real fetch will return and the selection rule it will use;
//! **no production code in this crate constructs a `ReleaseCatalog`** — only a test
//! does, over a catalogue it declares itself — so nothing can present one as fetched.

use crate::config::ReleaseChannel;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use thiserror::Error;

/// Version catalog across distribution channels.
///
/// This is the shape a real release feed will deserialize into. No code in this crate
/// produces one today — see the module docs. It deliberately has no `Default`
/// implementation: a default catalogue would be a fabricated one.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseCatalog {
    pub channels: BTreeMap<String, Vec<String>>,
    pub latest: BTreeMap<String, String>,
}

/// Why a remote catalogue could not be obtained.
#[derive(Error, Debug, PartialEq, Eq)]
pub enum VersionFetchError {
    /// No release feed is published and this build has no network client.
    #[error(
        "NOT IMPLEMENTED: `mythos versions --remote` cannot query a release feed. \
         No feed is published yet and this build contains no network client, so there \
         is nothing to fetch. This command refuses rather than printing a hardcoded \
         catalogue under a remote URL it never contacted. Run `mythos versions` \
         (without --remote) to list versions installed locally."
    )]
    NotImplemented,
}

pub struct VersionManager;

impl VersionManager {
    /// List locally installed and cached versions in ~/.mythos/versions.
    ///
    /// This is a real measurement: it enumerates directories that exist on disk.
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

    /// Query the remote release catalogue.
    ///
    /// # Not implemented
    ///
    /// This returns [`VersionFetchError::NotImplemented`] for every input. The
    /// `remote_url` parameter is accepted because the call sites have a configured URL
    /// to offer, but **it is not contacted, and its value does not affect the result**.
    /// An earlier version of this function logged the URL at `debug` level and returned
    /// a hardcoded catalogue, which made a caller believe a fetch had happened; the log
    /// was invisible at the default `ERROR` filter, so nothing ever contradicted that
    /// belief.
    ///
    /// Implementing this requires an HTTP client, SHA-256 checksum verification and
    /// signature verification before any downloaded binary may be installed. That is
    /// its own slice; shipping a fetch without verification would replace a fabricated
    /// catalogue with a fabricated binary.
    pub fn fetch_remote_versions(
        remote_url: &str,
        _channel: Option<ReleaseChannel>,
    ) -> Result<ReleaseCatalog, VersionFetchError> {
        let _ = remote_url;
        Err(VersionFetchError::NotImplemented)
    }

    /// Resolve the target SemVer version for a given channel from the catalog.
    ///
    /// Pure lookup over a caller-supplied catalogue. It is correct, and it is the rule a
    /// real fetch will use — but no production path supplies it a catalogue today.
    pub fn resolve_latest(catalog: &ReleaseCatalog, channel: ReleaseChannel) -> Option<String> {
        let key = channel.to_string();
        catalog.latest.get(&key).cloned()
    }
}
