//! Atomic update engine caching releases and updating symlinks.

use crate::config::{CliConfig, CliConfigError};
use crate::versions::VersionManager;
use std::fs;
use std::path::Path;
use std::time::Instant;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum UpdateError {
    #[error("Configuration error: {0}")]
    Config(#[from] CliConfigError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Version resolution failed for channel '{0}'")]
    VersionResolutionFailed(String),

    #[error("Target version '{0}' is already installed")]
    AlreadyInstalled(String),
}

pub struct UpdateManager;

impl UpdateManager {
    /// Perform or simulate an update to a target version or latest channel version.
    pub fn update(
        mythos_dir: &Path,
        target_version: Option<String>,
    ) -> Result<UpdateResult, UpdateError> {
        let start = Instant::now();
        let mut config = CliConfig::load_from(mythos_dir)?;

        let catalog =
            VersionManager::fetch_remote_versions(&config.remote_url, Some(config.channel));
        let version_to_install = match target_version {
            Some(v) => v,
            None => VersionManager::resolve_latest(&catalog, config.channel)
                .ok_or_else(|| UpdateError::VersionResolutionFailed(config.channel.to_string()))?,
        };

        // Cache directory: ~/.mythos/versions/<version>/bin/mythos
        let version_dir = mythos_dir.join("versions").join(&version_to_install);
        let bin_dir = version_dir.join("bin");
        fs::create_dir_all(&bin_dir)?;

        let target_bin = bin_dir.join(binary_filename());
        if !target_bin.exists() {
            // Write payload (binary)
            let current_exe = std::env::current_exe().ok();
            if let Some(exe_path) = current_exe {
                if exe_path.exists() && exe_path.is_file() {
                    let _ = fs::copy(&exe_path, &target_bin);
                }
            }
            if !target_bin.exists() {
                // If current_exe couldn't be copied, create simulated executable binary
                fs::write(
                    &target_bin,
                    format!("#!/bin/sh\necho 'MythOS v{}'\n", version_to_install),
                )?;
            }
            set_executable_permission(&target_bin)?;
        }

        // Atomic symlink switch:
        // 1. ~/.mythos/current -> ~/.mythos/versions/<version>
        let current_link = mythos_dir.join("current");
        atomic_symlink(&version_dir, &current_link)?;

        // 2. ~/.mythos/bin/mythos -> ~/.mythos/current/bin/mythos
        let mythos_bin_dir = mythos_dir.join("bin");
        fs::create_dir_all(&mythos_bin_dir)?;
        let mythos_bin_link = mythos_bin_dir.join(binary_filename());
        atomic_symlink(&target_bin, &mythos_bin_link)?;

        // Update persistent config
        let prev_version = config.current_version.clone();
        if prev_version.as_deref() != Some(&version_to_install) {
            config.previous_version = prev_version;
        }
        config.current_version = Some(version_to_install.clone());
        config.save_to(mythos_dir)?;

        let duration = start.elapsed();
        Ok(UpdateResult {
            version: version_to_install,
            duration_ms: duration.as_secs_f64() * 1000.0,
        })
    }
}

#[derive(Debug, Clone)]
pub struct UpdateResult {
    pub version: String,
    pub duration_ms: f64,
}

pub fn binary_filename() -> &'static str {
    if cfg!(windows) {
        "mythos.exe"
    } else {
        "mythos"
    }
}

pub fn atomic_symlink(target: &Path, link_path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        if let Some(parent) = link_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp_link = link_path.with_extension(format!("tmp.{}", std::process::id()));
        if tmp_link.exists() || fs::symlink_metadata(&tmp_link).is_ok() {
            let _ = fs::remove_file(&tmp_link);
        }
        symlink(target, &tmp_link)?;
        fs::rename(&tmp_link, link_path)?;
        Ok(())
    }

    #[cfg(not(unix))]
    {
        if let Some(parent) = link_path.parent() {
            fs::create_dir_all(parent)?;
        }
        if target.is_dir() {
            let _ = std::os::windows::fs::symlink_dir(target, link_path);
        } else {
            let _ = fs::copy(target, link_path);
        }
        Ok(())
    }
}

fn set_executable_permission(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms)?;
    }
    let _ = path;
    Ok(())
}
