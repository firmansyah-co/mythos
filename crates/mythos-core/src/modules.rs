//! Settings → Modules (ADR-011 §2.4): switching modules off and on, and installing and removing
//! them from packages.
//!
//! A module is **disabled** by its manifest id. It stays where it is; the server simply does not
//! use it: a disabled driver's connections stop with a `CONFIG_ERROR` that names the reason, and a
//! disabled service answers nothing. The list is kept in `<system dir>/modules.json`.
//!
//! A **module package** is a zip:
//!
//! ```text
//! module.json                    the manifest, as the executable prints it with --manifest
//! checksums.json                 {"<path in the zip>": "<sha-256>", …} for every other file
//! bin/<target>/mythos-module-<provides>[.exe]   one executable per platform (Rust target triple)
//! ui/…                           the module's workbench UI, when it has one (ADR-011 §2.3)
//! ```
//!
//! Installing checks every file against `checksums.json`, refuses an entry that would land outside
//! its directory, takes the executable for this server's platform, and requires that it answers
//! `--manifest` with the manifest of the package before it is put in place. Packages are installed
//! into `<system dir>/modules/`, which is always one of the module directories, so an update of
//! MythOS itself (a new version directory) keeps them. Their UI goes to
//! `<system dir>/modules-ui/<id>/`.

use mythos_module::protocol::ModuleManifest;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use thiserror::Error;

/// The file a module's UI directory starts from: an ES module whose default export is the
/// module's UI (see `@mythos/workbench-sdk`).
pub const UI_ENTRY: &str = "entry.js";

/// The platform this server was built for, which picks the executable out of a package.
pub const TARGET: &str = mythos_update::release::TARGET;

#[derive(Error, Debug)]
pub enum ModuleError {
    #[error("{0}")]
    BadPackage(String),
    #[error("the package has no build for this platform ({TARGET}); it has: {0}")]
    NoBuildForPlatform(String),
    #[error("{0}")]
    Conflict(String),
    #[error("there is no module '{0}'")]
    NotFound(String),
    #[error("{0}")]
    Io(String),
}

impl From<std::io::Error> for ModuleError {
    fn from(e: std::io::Error) -> Self {
        ModuleError::Io(e.to_string())
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Stored {
    #[serde(default)]
    disabled: BTreeSet<String>,
}

/// Which modules are switched off, and where packages are installed.
pub struct ModuleSettings {
    system_dir: PathBuf,
    disabled: RwLock<BTreeSet<String>>,
}

impl ModuleSettings {
    /// The settings kept in `system_dir`. A missing file is no module disabled.
    pub fn open(system_dir: &Path) -> Result<Self, ModuleError> {
        let path = system_dir.join("modules.json");
        let stored: Stored = match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| {
                ModuleError::Io(format!("{} is not module settings: {e}", path.display()))
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Stored::default(),
            Err(e) => return Err(e.into()),
        };
        std::fs::create_dir_all(system_dir.join("modules"))?;
        Ok(Self {
            system_dir: system_dir.to_path_buf(),
            disabled: RwLock::new(stored.disabled),
        })
    }

    /// Where packages are installed. The server always looks here for modules.
    pub fn install_dir(&self) -> PathBuf {
        self.system_dir.join("modules")
    }

    /// Where the UI of the module `id` is kept.
    pub fn ui_dir(&self, id: &str) -> PathBuf {
        self.system_dir.join("modules-ui").join(id)
    }

    /// The directory holding the workbench UI of `module` (ADR-011 §2.3), when it has one: the
    /// one installed with its package, else `modules-ui/<id>/` beside the directory of its
    /// executable (where a MythOS release bundle puts the UI of the modules it ships).
    pub fn ui_dir_of(&self, module: &mythos_module::ModuleBinary) -> Option<PathBuf> {
        let id = &module.manifest.id;
        let beside = module
            .path
            .parent()
            .and_then(Path::parent)
            .map(|d| d.join("modules-ui").join(id));
        [Some(self.ui_dir(id)), beside]
            .into_iter()
            .flatten()
            .find(|dir| dir.join(UI_ENTRY).is_file())
    }

    pub fn disabled(&self) -> BTreeSet<String> {
        self.disabled
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn is_disabled(&self, id: &str) -> bool {
        self.disabled
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .contains(id)
    }

    /// Switch the module `id` on or off, and keep it so.
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), ModuleError> {
        let mut disabled = self.disabled.write().unwrap_or_else(|e| e.into_inner());
        let mut next = disabled.clone();
        if enabled {
            next.remove(id);
        } else {
            next.insert(id.to_string());
        }
        let text = serde_json::to_vec_pretty(&Stored {
            disabled: next.clone(),
        })
        .map_err(|e| ModuleError::Io(e.to_string()))?;
        let path = self.system_dir.join("modules.json");
        let tmp = self
            .system_dir
            .join(format!(".modules.json.{}.tmp", std::process::id()));
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &path)?;
        *disabled = next;
        Ok(())
    }

    /// Install a package. Returns the manifest of what was installed.
    ///
    /// `others` are the modules already found, by `(kind, provides)`, so a package cannot take
    /// over a name another module provides.
    pub async fn install(
        &self,
        package: &[u8],
        others: &[(ModuleManifest, PathBuf)],
    ) -> Result<ModuleManifest, ModuleError> {
        let unpacked = unpack(package)?;
        let manifest = unpacked.manifest.clone();
        if let Some((other, path)) = others.iter().find(|(m, _)| {
            m.kind == manifest.kind && m.provides == manifest.provides && m.id != manifest.id
        }) {
            return Err(ModuleError::Conflict(format!(
                "'{}' is already provided by module '{}' ({})",
                manifest.provides,
                other.id,
                path.display()
            )));
        }

        let install_dir = self.install_dir();
        std::fs::create_dir_all(&install_dir)?;
        let file_name = format!(
            "{}{}{}",
            mythos_module::MODULE_FILE_PREFIX,
            manifest.provides,
            std::env::consts::EXE_SUFFIX
        );
        let staged = install_dir.join(format!(".staging-{}-{}", std::process::id(), file_name));
        std::fs::write(&staged, &unpacked.executable)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))?;
        }
        // On Windows a file is only executable with the .exe name; stage it under one.
        let probe = if cfg!(windows) {
            let probe = install_dir.join(format!(".probe-{}-{}", std::process::id(), file_name));
            std::fs::rename(&staged, &probe)?;
            probe
        } else {
            staged.clone()
        };
        let said = mythos_module::read_manifest(&probe).await;
        let said = match said {
            Ok(said) => said,
            Err(e) => {
                let _ = std::fs::remove_file(&probe);
                return Err(ModuleError::BadPackage(format!(
                    "its executable for {TARGET} does not run here: {e}"
                )));
            }
        };
        if said.id != manifest.id
            || said.version != manifest.version
            || said.kind != manifest.kind
            || said.provides != manifest.provides
        {
            let _ = std::fs::remove_file(&probe);
            return Err(ModuleError::BadPackage(format!(
                "its executable says it is '{}' {} providing '{}', but module.json says '{}' {} providing '{}'",
                said.id, said.version, said.provides, manifest.id, manifest.version, manifest.provides
            )));
        }

        // The UI first: a module whose executable is in place but whose UI is not would be a
        // half-installed module.
        let ui_dir = self.ui_dir(&manifest.id);
        if !unpacked.ui.is_empty() {
            let staging = self.system_dir.join("modules-ui").join(format!(
                ".staging-{}-{}",
                manifest.id,
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&staging);
            for (relative, bytes) in &unpacked.ui {
                let path = staging.join(relative);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(path, bytes)?;
            }
            let _ = std::fs::remove_dir_all(&ui_dir);
            std::fs::rename(&staging, &ui_dir)?;
        } else {
            let _ = std::fs::remove_dir_all(&ui_dir);
        }

        // Then the executable, the way an installer does on every platform: a running one is
        // renamed away (Windows refuses to replace it), then the new one is moved in.
        let target = install_dir.join(&file_name);
        if target.exists() {
            let old = install_dir.join(format!(
                "{}.replaced-{}.old",
                file_name,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            ));
            std::fs::rename(&target, old)?;
        }
        std::fs::rename(&probe, &target)?;
        Ok(manifest)
    }

    /// Remove the installed module `manifest` (its executable and UI). The caller has stopped it.
    pub fn remove(&self, manifest: &ModuleManifest, path: &Path) -> Result<(), ModuleError> {
        if path.parent() != Some(self.install_dir().as_path()) {
            return Err(ModuleError::Conflict(format!(
                "module '{}' is part of this MythOS installation ({}), not an installed package; disable it instead",
                manifest.id,
                path.display()
            )));
        }
        // A process that is still exiting can hold the file on Windows for a moment.
        let mut attempt = 0;
        loop {
            match std::fs::remove_file(path) {
                Ok(()) => break,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
                Err(e) if attempt < 20 => {
                    let _ = e;
                    attempt += 1;
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(e) => return Err(e.into()),
            }
        }
        let _ = std::fs::remove_dir_all(self.ui_dir(&manifest.id));
        // A removed module is not remembered as disabled.
        self.set_enabled(&manifest.id, true)?;
        Ok(())
    }
}

/// A package, checked and taken apart.
pub struct Unpacked {
    pub manifest: ModuleManifest,
    pub executable: Vec<u8>,
    /// `ui/…` files, by their path below `ui/`.
    pub ui: Vec<(PathBuf, Vec<u8>)>,
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Check a package and take out what this platform needs. Nothing is written.
pub fn unpack(package: &[u8]) -> Result<Unpacked, ModuleError> {
    let bad = |m: String| ModuleError::BadPackage(m);
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(package))
        .map_err(|e| bad(format!("it is not a zip file: {e}")))?;

    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| bad(e.to_string()))?;
        if entry.enclosed_name().is_none() {
            return Err(bad(format!(
                "entry {:?} would be written outside the module's directory",
                entry.name()
            )));
        }
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().replace('\\', "/");
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|e| bad(format!("{name}: {e}")))?;
        files.insert(name, bytes);
    }

    let manifest: ModuleManifest = serde_json::from_slice(
        files
            .get("module.json")
            .ok_or_else(|| bad("it has no module.json".into()))?,
    )
    .map_err(|e| bad(format!("module.json is not a module manifest: {e}")))?;
    let sums: BTreeMap<String, String> = serde_json::from_slice(
        files
            .get("checksums.json")
            .ok_or_else(|| bad("it has no checksums.json".into()))?,
    )
    .map_err(|e| {
        bad(format!(
            "checksums.json is not a map of file to SHA-256: {e}"
        ))
    })?;

    for (name, bytes) in &files {
        if name == "checksums.json" {
            continue;
        }
        let listed = sums
            .get(name)
            .ok_or_else(|| bad(format!("{name} is not listed in checksums.json")))?;
        if !listed.eq_ignore_ascii_case(&sha256_hex(bytes)) {
            return Err(bad(format!("{name} does not match its checksum")));
        }
    }
    if let Some(missing) = sums.keys().find(|name| !files.contains_key(*name)) {
        return Err(bad(format!(
            "checksums.json lists {missing}, which is not in the package"
        )));
    }

    let exe = format!(
        "bin/{TARGET}/{}{}{}",
        mythos_module::MODULE_FILE_PREFIX,
        manifest.provides,
        if TARGET.contains("windows") {
            ".exe"
        } else {
            ""
        }
    );
    let executable = match files.get(&exe) {
        Some(bytes) => bytes.clone(),
        None => {
            let targets: BTreeSet<&str> = files
                .keys()
                .filter_map(|n| n.strip_prefix("bin/")?.split('/').next())
                .collect();
            return Err(ModuleError::NoBuildForPlatform(if targets.is_empty() {
                "none".into()
            } else {
                targets.into_iter().collect::<Vec<_>>().join(", ")
            }));
        }
    };
    let ui = files
        .iter()
        .filter_map(|(name, bytes)| {
            name.strip_prefix("ui/")
                .filter(|rest| !rest.is_empty())
                .map(|rest| (PathBuf::from(rest), bytes.clone()))
        })
        .collect();
    Ok(Unpacked {
        manifest,
        executable,
        ui,
    })
}

/// Build a package from a directory laid out as one (`module.json`, `bin/…`, `ui/…`), writing
/// its `checksums.json`. Used by `mythos module pack` and the tests.
pub fn pack(dir: &Path) -> Result<Vec<u8>, ModuleError> {
    let manifest_text = std::fs::read(dir.join("module.json"))
        .map_err(|e| ModuleError::BadPackage(format!("module.json: {e}")))?;
    serde_json::from_slice::<ModuleManifest>(&manifest_text).map_err(|e| {
        ModuleError::BadPackage(format!("module.json is not a module manifest: {e}"))
    })?;

    let mut files: Vec<(String, PathBuf)> = Vec::new();
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                walk(root, &path, out)?;
            } else {
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                if relative != "checksums.json" {
                    out.push((relative, path));
                }
            }
        }
        Ok(())
    }
    walk(dir, dir, &mut files)?;
    files.sort();

    let mut sums = BTreeMap::new();
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut out);
        for (name, path) in &files {
            let bytes = std::fs::read(path)?;
            sums.insert(name.clone(), sha256_hex(&bytes));
            let mode = if name.starts_with("bin/") {
                0o755
            } else {
                0o644
            };
            zip.start_file(
                name.as_str(),
                zip::write::FileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated)
                    .large_file(true)
                    .unix_permissions(mode),
            )
            .map_err(|e| ModuleError::Io(e.to_string()))?;
            std::io::Write::write_all(&mut zip, &bytes)?;
        }
        zip.start_file("checksums.json", zip::write::FileOptions::default())
            .map_err(|e| ModuleError::Io(e.to_string()))?;
        std::io::Write::write_all(
            &mut zip,
            &serde_json::to_vec_pretty(&sums).map_err(|e| ModuleError::Io(e.to_string()))?,
        )?;
        zip.finish().map_err(|e| ModuleError::Io(e.to_string()))?;
    }
    Ok(out.into_inner())
}
