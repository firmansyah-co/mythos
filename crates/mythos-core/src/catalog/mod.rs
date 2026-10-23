//! Open Hardware Catalog Ingestion & Registry Engine.
//!
//! Provides parser and query functions for loading standard device packages
//! conforming to MYTHOS-CAT-001.

pub mod fingerprint;
pub mod schema;

pub use fingerprint::{CatalogFingerprintEngine, FingerprintMatch};
pub use schema::*;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Catalog management and validation errors.
#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("I/O error reading catalog: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON deserialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Invalid manifest at {path}: {reason}")]
    InvalidManifest { path: PathBuf, reason: String },

    #[error("Device manifest with ID '{0}' not found")]
    NotFound(String),
}

/// In-memory catalog repository indexing device manifests by ID and vendor.
#[derive(Debug, Clone, Default)]
pub struct DeviceCatalog {
    manifests: HashMap<String, DeviceManifest>,
}

impl DeviceCatalog {
    /// Create a new empty device catalog.
    pub fn new() -> Self {
        Self {
            manifests: HashMap::new(),
        }
    }

    /// Register a single device manifest after validation.
    pub fn register(&mut self, manifest: DeviceManifest) -> Result<(), CatalogError> {
        Self::validate_manifest(&manifest)?;
        self.manifests
            .insert(manifest.metadata.id.clone(), manifest);
        Ok(())
    }

    /// Retrieve a manifest by device ID.
    pub fn get(&self, device_id: &str) -> Option<&DeviceManifest> {
        self.manifests.get(device_id)
    }

    /// List all registered device manifests.
    pub fn list(&self) -> Vec<&DeviceManifest> {
        self.manifests.values().collect()
    }

    /// Number of registered device manifests.
    pub fn len(&self) -> usize {
        self.manifests.len()
    }

    /// Returns true if catalog is empty.
    pub fn is_empty(&self) -> bool {
        self.manifests.is_empty()
    }

    /// Parse a manifest from a JSON string.
    pub fn from_json_str(json_str: &str) -> Result<DeviceManifest, CatalogError> {
        let manifest: DeviceManifest = serde_json::from_str(json_str)?;
        Self::validate_manifest(&manifest)?;
        Ok(manifest)
    }

    /// Load a manifest from a JSON file.
    pub fn load_manifest_file<P: AsRef<Path>>(path: P) -> Result<DeviceManifest, CatalogError> {
        let path_ref = path.as_ref();
        let contents = fs::read_to_string(path_ref)?;
        let manifest: DeviceManifest =
            serde_json::from_str(&contents).map_err(|e| CatalogError::InvalidManifest {
                path: path_ref.to_path_buf(),
                reason: e.to_string(),
            })?;
        Self::validate_manifest(&manifest).map_err(|e| match e {
            CatalogError::InvalidManifest { reason, .. } => CatalogError::InvalidManifest {
                path: path_ref.to_path_buf(),
                reason,
            },
            other => other,
        })?;
        Ok(manifest)
    }

    /// Recursively search and load all `manifest.json` files in a directory tree.
    pub fn load_directory<P: AsRef<Path>>(&mut self, dir_path: P) -> Result<usize, CatalogError> {
        let mut loaded = 0;
        let root = dir_path.as_ref();
        if !root.exists() {
            return Ok(0);
        }

        Self::walk_and_load(root, self, &mut loaded)?;
        Ok(loaded)
    }

    fn walk_and_load(
        dir: &Path,
        catalog: &mut DeviceCatalog,
        loaded: &mut usize,
    ) -> Result<(), CatalogError> {
        if dir.is_dir() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    Self::walk_and_load(&path, catalog, loaded)?;
                } else if path.file_name().and_then(|n| n.to_str()) == Some("manifest.json") {
                    let manifest = Self::load_manifest_file(&path)?;
                    catalog.register(manifest)?;
                    *loaded += 1;
                }
            }
        }
        Ok(())
    }

    /// Validate manifest fields according to MYTHOS-CAT-001 invariants.
    pub fn validate_manifest(manifest: &DeviceManifest) -> Result<(), CatalogError> {
        if manifest.schema_version.is_empty() {
            return Err(CatalogError::InvalidManifest {
                path: PathBuf::new(),
                reason: "schemaVersion cannot be empty".to_string(),
            });
        }
        if manifest.metadata.id.is_empty() {
            return Err(CatalogError::InvalidManifest {
                path: PathBuf::new(),
                reason: "metadata.id cannot be empty".to_string(),
            });
        }
        if manifest.registers.is_empty() {
            return Err(CatalogError::InvalidManifest {
                path: PathBuf::new(),
                reason: "registers list cannot be empty".to_string(),
            });
        }

        // Validate register addresses and hex consistency
        for reg in &manifest.registers {
            // Verify address_dec parses cleanly from hex or matches bounds
            if let Some(stripped) = reg
                .address_hex
                .strip_prefix("0x")
                .or_else(|| reg.address_hex.strip_prefix("0X"))
            {
                if let Ok(val) = u16::from_str_radix(stripped, 16) {
                    if val != reg.address_dec {
                        return Err(CatalogError::InvalidManifest {
                            path: PathBuf::new(),
                            reason: format!(
                                "Register '{}': addressHex ({}) does not match addressDec ({})",
                                reg.name, reg.address_hex, reg.address_dec
                            ),
                        });
                    }
                }
            }

            // Verify scaling values
            if let ScalingConfig::Divisor { divisor, .. } = reg.scaling {
                if divisor == 0.0 {
                    return Err(CatalogError::InvalidManifest {
                        path: PathBuf::new(),
                        reason: format!("Register '{}': scaling divisor cannot be zero", reg.name),
                    });
                }
            }
        }

        // Validate protocol configuration
        if let Some(ref rtu) = manifest.protocol.modbus_rtu {
            if rtu.default_slave_id == 0 || rtu.default_slave_id > 247 {
                return Err(CatalogError::InvalidManifest {
                    path: PathBuf::new(),
                    reason: format!(
                        "Invalid defaultSlaveId {}: must be between 1 and 247",
                        rtu.default_slave_id
                    ),
                });
            }
        }

        Ok(())
    }
}
