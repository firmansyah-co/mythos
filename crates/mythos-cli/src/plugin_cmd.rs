//! Plugin management CLI subcommands: pack, install, list, inspect.

use clap::Subcommand;
use mythos_core::plugin::{PluginPackageManifest, PluginPackager};
use mythos_plugin_sdk::{PluginError, PluginMetadata};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PluginCliError {
    #[error("Plugin error: {0}")]
    Plugin(#[from] PluginError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Manifest file not found: {0}")]
    ManifestNotFound(PathBuf),

    #[error("Payload file not found: {0}")]
    PayloadNotFound(PathBuf),

    #[error("Installed plugin '{0}' not found")]
    PluginNotFound(String),
}

#[derive(Subcommand, Debug, Clone)]
pub enum PluginCommands {
    /// Pack a plugin directory or payload into a single .mythos-plugin archive
    Pack {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        payload: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Install a .mythos-plugin package
    Install {
        /// Path to .mythos-plugin package file
        package: PathBuf,
    },
    /// List installed plugins
    List,
    /// Inspect an installed plugin metadata and details
    Inspect {
        /// Plugin ID to inspect
        id: String,
    },
}

pub struct PluginCliManager;

impl PluginCliManager {
    /// Pack a plugin package from manifest and payload files.
    pub fn pack(
        manifest_path: &Path,
        payload_path: &Path,
        output_path: &Path,
    ) -> Result<(), PluginCliError> {
        if !manifest_path.exists() {
            return Err(PluginCliError::ManifestNotFound(
                manifest_path.to_path_buf(),
            ));
        }
        if !payload_path.exists() {
            return Err(PluginCliError::PayloadNotFound(payload_path.to_path_buf()));
        }

        let manifest_str = fs::read_to_string(manifest_path)?;
        let mut manifest: PluginPackageManifest = serde_json::from_str(&manifest_str)?;

        let payload_bytes = fs::read(payload_path)?;
        if manifest.checksum.is_empty() {
            manifest.checksum = PluginPackager::compute_sha256(&payload_bytes);
        }

        PluginPackager::build_package(&manifest, &payload_bytes, output_path)?;
        Ok(())
    }

    /// Install a .mythos-plugin package into ~/.mythos/plugins/
    pub fn install(mythos_dir: &Path, package_path: &Path) -> Result<PluginMetadata, PluginError> {
        let plugins_dir = mythos_dir.join("plugins");
        let verified = PluginPackager::extract_and_verify(package_path, &plugins_dir)?;

        // Default allowed capabilities in CLI environment
        let mut allowed = HashSet::new();
        allowed.insert("tagbus:read".to_string());
        allowed.insert("tagbus:write".to_string());
        allowed.insert("net:listen".to_string());
        allowed.insert("serial:rw".to_string());

        PluginPackager::verify_capabilities(&verified.manifest, &allowed)?;
        Ok(verified.manifest.metadata)
    }

    /// List all installed plugins across ~/.mythos/plugins/
    pub fn list(mythos_dir: &Path) -> Result<Vec<PluginMetadata>, PluginCliError> {
        let plugins_dir = mythos_dir.join("plugins");
        if !plugins_dir.exists() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        if let Ok(entries) = fs::read_dir(plugins_dir) {
            for entry in entries.flatten() {
                if let Ok(ft) = entry.file_type() {
                    if ft.is_dir() {
                        let plugin_dir = entry.path();
                        // Find versions inside plugin directory
                        if let Ok(vers) = fs::read_dir(&plugin_dir) {
                            for ver_entry in vers.flatten() {
                                let manifest_file = ver_entry.path().join("manifest.json");
                                if manifest_file.exists() {
                                    if let Ok(data) = fs::read_to_string(&manifest_file) {
                                        if let Ok(m) =
                                            serde_json::from_str::<PluginPackageManifest>(&data)
                                        {
                                            results.push(m.metadata);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(results)
    }

    /// Inspect an installed plugin by its ID.
    pub fn inspect(
        mythos_dir: &Path,
        plugin_id: &str,
    ) -> Result<PluginPackageManifest, PluginCliError> {
        let plugins_dir = mythos_dir.join("plugins").join(plugin_id);
        if !plugins_dir.exists() {
            return Err(PluginCliError::PluginNotFound(plugin_id.to_string()));
        }

        let mut latest_manifest: Option<PluginPackageManifest> = None;
        if let Ok(entries) = fs::read_dir(plugins_dir) {
            for entry in entries.flatten() {
                let manifest_file = entry.path().join("manifest.json");
                if manifest_file.exists() {
                    if let Ok(data) = fs::read_to_string(&manifest_file) {
                        if let Ok(m) = serde_json::from_str::<PluginPackageManifest>(&data) {
                            latest_manifest = Some(m);
                        }
                    }
                }
            }
        }

        latest_manifest.ok_or_else(|| PluginCliError::PluginNotFound(plugin_id.to_string()))
    }
}
