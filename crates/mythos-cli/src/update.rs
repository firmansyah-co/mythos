//! Atomic update engine caching releases and updating symlinks.
//!
//! # What is implemented, and what is not
//!
//! **Nothing installs a release yet.** [`UpdateManager::update`] refuses for every
//! input and returns [`UpdateError::NotImplemented`].
//!
//! This module previously *fabricated* an install: it copied the currently running
//! executable into `~/.mythos/versions/<target>/bin/`, recorded `<target>` as the
//! current version, and the CLI printed "Successfully updated to version <target>".
//! The installed file was byte-identical to the running binary, so `mythos --version`
//! still reported the *old* version. If the copy failed it wrote a shell script that
//! echoed the target version string. Both paths are deleted.
//!
//! The structural guarantee that replaces them: **no code path in this module writes
//! `CliConfig::current_version`.** There is no `Ok` arm, so there is nothing the CLI
//! could report as successfully installed. The only things left here are the two
//! filesystem primitives that [`crate::rollback`] genuinely uses.

use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum UpdateError {
    /// No release feed is published and this build has no network client.
    #[error(
        "NOT IMPLEMENTED: `mythos update` cannot install a release. No release feed is \
         published yet and this build contains no network client, so there is no binary \
         to download. This command refuses rather than copying the running binary and \
         relabelling it as the target version. Run `mythos versions` to see what is \
         installed locally."
    )]
    NotImplemented,
}

/// The outcome of a successful update.
///
/// Retained as the shape a real implementation will return. No code constructs one
/// today, because [`UpdateManager::update`] has no success path.
#[derive(Debug, Clone)]
pub struct UpdateResult {
    pub version: String,
    pub duration_ms: f64,
}

pub struct UpdateManager;

impl UpdateManager {
    /// Install `target_version`, or the latest version in the active channel when
    /// `target_version` is `None`.
    ///
    /// # Not implemented
    ///
    /// This returns [`UpdateError::NotImplemented`] for **every** input, including the
    /// bare `mythos update` invocation that reaches it with `None`. That path is the one
    /// an operator reaches without flags, so it is the one the tests exercise directly.
    pub fn update(
        mythos_dir: &Path,
        target_version: Option<String>,
    ) -> Result<UpdateResult, UpdateError> {
        let _ = (mythos_dir, target_version);
        Err(UpdateError::NotImplemented)
    }
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
