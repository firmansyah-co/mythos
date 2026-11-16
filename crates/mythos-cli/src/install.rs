//! Installed versions, and which one is current.
//!
//! ```text
//! <mythos dir>/
//!   versions/<version>/bin/mythos[.exe], mythos-server[.exe], mythos-module-*[.exe]
//!   versions/<version>/web/            the workbench
//!   current  ->  versions/<version>    a symlink on Unix, a junction on Windows
//! ```
//!
//! Put `<mythos dir>/current/bin` on `PATH`. Switching version moves `current` and touches no
//! executable, so the running CLI (or server) is never overwritten, which Windows forbids.
//!
//! A bundle is installed only after its SHA-256 matches the release's `SHA256SUMS`, every entry
//! of its archive stays inside its own directory, and its `mythos` runs on this machine. Until
//! then it lives in a staging directory that is never `current`.

use crate::release::{bundle_name, Release, CHECKSUMS_ASSET, TARGET};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use thiserror::Error;

/// How long a freshly installed `mythos --version` may take to answer.
const RUN_CHECK_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Error, Debug)]
pub enum InstallError {
    #[error("release {tag} publishes no bundle for this platform ({target}); expected {name}")]
    NoBundleForPlatform {
        tag: String,
        target: String,
        name: String,
    },

    #[error("release {tag} publishes no {CHECKSUMS_ASSET}, so {name} cannot be checked; refusing to install it")]
    NoChecksums { tag: String, name: String },

    #[error("{CHECKSUMS_ASSET} of release {tag} does not list {name}; refusing to install it")]
    NotInChecksums { tag: String, name: String },

    #[error("{name} does not match its published checksum (expected {expected}, got {actual}); refusing to install it")]
    ChecksumMismatch {
        name: String,
        expected: String,
        actual: String,
    },

    #[error("downloading {url} failed: {reason}")]
    Download { url: String, reason: String },

    #[error("{name} is not a usable bundle: {reason}")]
    BadBundle { name: String, reason: String },

    #[error("the installed {path} does not run on this machine: {reason}")]
    DoesNotRun { path: String, reason: String },

    #[error("version {0} is not installed in this directory")]
    NotInstalled(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// The file name of an executable on this platform.
pub fn exe(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

pub fn versions_dir(mythos_dir: &Path) -> PathBuf {
    mythos_dir.join("versions")
}

pub fn version_dir(mythos_dir: &Path, version: &str) -> PathBuf {
    versions_dir(mythos_dir).join(version)
}

/// Whether `version` is installed: its directory holds a `mythos` executable.
pub fn is_installed(mythos_dir: &Path, version: &str) -> bool {
    version_dir(mythos_dir, version)
        .join("bin")
        .join(exe("mythos"))
        .is_file()
}

/// Download, check and unpack `release`'s bundle for this platform into `versions/<version>`.
/// Returns what the installed `mythos --version` printed.
pub fn install(mythos_dir: &Path, release: &Release) -> Result<String, InstallError> {
    let name = bundle_name(&release.version, TARGET);
    let bundle = release
        .bundle_for(TARGET)
        .ok_or_else(|| InstallError::NoBundleForPlatform {
            tag: release.tag.clone(),
            target: TARGET.to_string(),
            name: name.clone(),
        })?;
    let sums = release
        .asset(CHECKSUMS_ASSET)
        .ok_or_else(|| InstallError::NoChecksums {
            tag: release.tag.clone(),
            name: name.clone(),
        })?;

    let mut listing = Vec::new();
    download(&sums.url, &mut listing)?;
    let expected = checksum_in(&String::from_utf8_lossy(&listing), &name).ok_or_else(|| {
        InstallError::NotInChecksums {
            tag: release.tag.clone(),
            name: name.clone(),
        }
    })?;

    let versions = versions_dir(mythos_dir);
    fs::create_dir_all(&versions)?;
    let staging = versions.join(format!(
        ".staging-{}-{}",
        release.version,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;
    let result = (|| {
        let archive = staging.join(".bundle.zip");
        let actual = {
            let mut file = fs::File::create(&archive)?;
            let mut hashing = HashingWriter::new(&mut file);
            download(&bundle.url, &mut hashing)?;
            hashing.finish()
        };
        if !actual.eq_ignore_ascii_case(&expected) {
            return Err(InstallError::ChecksumMismatch {
                name: name.clone(),
                expected: expected.clone(),
                actual,
            });
        }
        unpack(&archive, &staging, &name)?;
        fs::remove_file(&archive)?;
        let reported = check_runs(&staging.join("bin").join(exe("mythos")))?;
        let target = version_dir(mythos_dir, &release.version.to_string());
        if target.exists() {
            // A directory left by an install that did not finish: it was never current.
            fs::remove_dir_all(&target)?;
        }
        fs::rename(&staging, &target)?;
        Ok(reported)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

/// The checksum `SHA256SUMS` gives for `name` (`<hex>  <name>`, or `<hex> *<name>`).
pub fn checksum_in(listing: &str, name: &str) -> Option<String> {
    listing.lines().find_map(|line| {
        let (hash, file) = line.trim().split_once(char::is_whitespace)?;
        let file = file.trim_start().trim_start_matches('*');
        (file == name).then(|| hash.to_ascii_lowercase())
    })
}

fn download(url: &str, out: &mut dyn Write) -> Result<(), InstallError> {
    let failed = |reason: String| InstallError::Download {
        url: url.to_string(),
        reason,
    };
    let response = crate::release::agent()
        .get(url)
        .header("Accept", "application/octet-stream")
        .call()
        .map_err(|e| failed(e.to_string()))?;
    let mut body = response.into_body();
    let mut reader = body.as_reader();
    std::io::copy(&mut reader, out).map_err(|e| failed(e.to_string()))?;
    Ok(())
}

struct HashingWriter<'a> {
    inner: &'a mut dyn Write,
    hasher: Sha256,
}

impl<'a> HashingWriter<'a> {
    fn new(inner: &'a mut dyn Write) -> Self {
        Self {
            inner,
            hasher: Sha256::new(),
        }
    }

    fn finish(self) -> String {
        self.hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
}

impl Write for HashingWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Unpack a bundle into `into`. Every entry must stay inside `into`: an archive naming
/// `../anything` or an absolute path is refused before anything is written.
fn unpack(archive: &Path, into: &Path, name: &str) -> Result<(), InstallError> {
    let bad = |reason: String| InstallError::BadBundle {
        name: name.to_string(),
        reason,
    };
    let mut zip = zip::ZipArchive::new(fs::File::open(archive)?).map_err(|e| bad(e.to_string()))?;

    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|e| bad(e.to_string()))?;
        if entry.enclosed_name().is_none() {
            return Err(bad(format!(
                "entry {:?} would be written outside the install directory",
                entry.name()
            )));
        }
    }

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| bad(e.to_string()))?;
        let relative = entry.enclosed_name().expect("checked above").to_path_buf();
        let path = into.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&path)?;
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::File::create(&path)?;
        let mut buffer = Vec::new();
        entry
            .read_to_end(&mut buffer)
            .map_err(|e| bad(e.to_string()))?;
        file.write_all(&buffer)?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(mode & 0o7777))?;
        }
    }

    if !into.join("bin").join(exe("mythos")).is_file() {
        return Err(bad(format!("it has no bin/{}", exe("mythos"))));
    }
    Ok(())
}

/// Run `mythos --version` from the new install. A bundle built for another platform, or one
/// that is damaged, fails here and is never made current. Returns what it printed.
fn check_runs(mythos: &Path) -> Result<String, InstallError> {
    let failed = |reason: String| InstallError::DoesNotRun {
        path: mythos.display().to_string(),
        reason,
    };
    let mut child = Command::new(mythos)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| failed(e.to_string()))?;
    let deadline = Instant::now() + RUN_CHECK_TIMEOUT;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(failed(format!(
                "no answer to --version within {RUN_CHECK_TIMEOUT:?}"
            )));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_string(&mut stdout);
    }
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_string(&mut stderr);
    }
    if !status.success() {
        return Err(failed(format!(
            "--version exited with {status}: {}",
            stderr.trim()
        )));
    }
    let reported = stdout.trim().to_string();
    if reported.is_empty() {
        return Err(failed("--version printed nothing".into()));
    }
    Ok(reported)
}

/// The link that names the current version.
pub fn current_link(mythos_dir: &Path) -> PathBuf {
    mythos_dir.join("current")
}

/// The version `current` points at, if any.
pub fn current_version(mythos_dir: &Path) -> Option<String> {
    let link = current_link(mythos_dir);
    #[cfg(unix)]
    let target = fs::read_link(&link).ok()?;
    #[cfg(windows)]
    let target = junction::get_target(&link).ok()?;
    target.file_name()?.to_str().map(str::to_string)
}

/// Point `current` at an installed version.
///
/// On Unix a new symlink is made beside the old one and renamed over it, so `current` always
/// names one version or the other. Windows cannot rename a directory link over another, so the
/// old junction is removed and the new one moved in its place; for that moment `current` is
/// missing. Either way no executable is touched.
pub fn point_current(mythos_dir: &Path, version: &str) -> Result<(), InstallError> {
    if !is_installed(mythos_dir, version) {
        return Err(InstallError::NotInstalled(version.to_string()));
    }
    let link = current_link(mythos_dir);
    let staged = mythos_dir.join(format!(".current-{}", std::process::id()));
    let _ = fs::remove_file(&staged);

    #[cfg(unix)]
    {
        // Relative, so the directory can be moved as a whole.
        std::os::unix::fs::symlink(Path::new("versions").join(version), &staged)?;
        fs::rename(&staged, &link)?;
    }

    #[cfg(windows)]
    {
        let _ = fs::remove_dir(&staged);
        junction::create(
            std::path::absolute(version_dir(mythos_dir, version))?,
            &staged,
        )?;
        if fs::symlink_metadata(&link).is_ok() {
            if junction::exists(&link).unwrap_or(false) {
                junction::delete(&link)?;
            }
            fs::remove_dir(&link).or_else(|_| fs::remove_file(&link))?;
        }
        fs::rename(&staged, &link)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_checksum_is_found_by_its_exact_file_name() {
        let listing = "\
            ABC123  mythos-0.2.0-x86_64-unknown-linux-gnu.zip\n\
            def456 *mythos-0.2.0-x86_64-pc-windows-msvc.zip\n";
        assert_eq!(
            checksum_in(listing, "mythos-0.2.0-x86_64-unknown-linux-gnu.zip").as_deref(),
            Some("abc123")
        );
        assert_eq!(
            checksum_in(listing, "mythos-0.2.0-x86_64-pc-windows-msvc.zip").as_deref(),
            Some("def456")
        );
        assert_eq!(checksum_in(listing, "mythos-0.2.0.zip"), None);
    }
}
