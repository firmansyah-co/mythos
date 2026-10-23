//! Plugin packager, capability verification, and package extraction engine.
//!
//! Handles single-file `.mythos-plugin` archives containing manifest, payload,
//! and optional resources.

use mythos_plugin_sdk::{PluginError, PluginMetadata};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::write::FileOptions;
use zip::{ZipArchive, ZipWriter};

/// Manifest metadata stored inside `.mythos-plugin` package.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginPackageManifest {
    pub metadata: PluginMetadata,
    pub entrypoint: String,
    pub checksum: String,
    pub min_core_version: String,
}

/// A verified and extracted plugin package.
#[derive(Debug, Clone)]
pub struct VerifiedPluginPackage {
    pub manifest: PluginPackageManifest,
    pub extracted_dir: PathBuf,
    pub payload_path: PathBuf,
}

/// Engine for creating, extracting, and verifying `.mythos-plugin` packages.
pub struct PluginPackager;

impl PluginPackager {
    /// Compute SHA-256 checksum hex string for given bytes.
    pub fn compute_sha256(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        format!("{:x}", hasher.finalize())
    }

    /// Build a single-file `.mythos-plugin` package.
    pub fn build_package(
        manifest: &PluginPackageManifest,
        payload_bytes: &[u8],
        output_path: &Path,
    ) -> Result<(), PluginError> {
        let actual_checksum = Self::compute_sha256(payload_bytes);
        if !manifest.checksum.is_empty() && manifest.checksum != actual_checksum {
            return Err(PluginError::InitializationFailed(format!(
                "Checksum mismatch during build: declared '{}', computed '{}'",
                manifest.checksum, actual_checksum
            )));
        }

        let mut manifest_to_write = manifest.clone();
        if manifest_to_write.checksum.is_empty() {
            manifest_to_write.checksum = actual_checksum;
        }

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                PluginError::InitializationFailed(format!(
                    "Failed to create output directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }

        let file = File::create(output_path).map_err(|e| {
            PluginError::InitializationFailed(format!(
                "Failed to create package file {}: {}",
                output_path.display(),
                e
            ))
        })?;

        let mut zip = ZipWriter::new(file);
        let options = FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o755);

        // 1. Write manifest.json
        zip.start_file("manifest.json", options).map_err(|e| {
            PluginError::InitializationFailed(format!("Failed to add manifest.json to zip: {}", e))
        })?;
        let manifest_bytes = serde_json::to_vec_pretty(&manifest_to_write).map_err(|e| {
            PluginError::InitializationFailed(format!("Failed to serialize manifest: {}", e))
        })?;
        zip.write_all(&manifest_bytes).map_err(|e| {
            PluginError::InitializationFailed(format!("Failed to write manifest.json: {}", e))
        })?;

        // 2. Write payload
        zip.start_file(&manifest_to_write.entrypoint, options)
            .map_err(|e| {
                PluginError::InitializationFailed(format!(
                    "Failed to add payload entrypoint '{}' to zip: {}",
                    manifest_to_write.entrypoint, e
                ))
            })?;
        zip.write_all(payload_bytes).map_err(|e| {
            PluginError::InitializationFailed(format!("Failed to write payload bytes: {}", e))
        })?;

        zip.finish().map_err(|e| {
            PluginError::InitializationFailed(format!("Failed to finalize zip package: {}", e))
        })?;

        Ok(())
    }

    /// Extract and verify a `.mythos-plugin` package.
    pub fn extract_and_verify(
        package_path: &Path,
        target_dir: &Path,
    ) -> Result<VerifiedPluginPackage, PluginError> {
        let file = File::open(package_path).map_err(|e| {
            PluginError::InitializationFailed(format!(
                "Failed to open package file {}: {}",
                package_path.display(),
                e
            ))
        })?;

        let mut archive = ZipArchive::new(file).map_err(|e| {
            PluginError::InitializationFailed(format!(
                "Failed to read zip archive from {}: {}",
                package_path.display(),
                e
            ))
        })?;

        // 1. Read manifest.json
        let manifest_bytes = {
            let mut manifest_file = archive.by_name("manifest.json").map_err(|e| {
                PluginError::InitializationFailed(format!(
                    "Package missing required 'manifest.json': {}",
                    e
                ))
            })?;
            let mut bytes = Vec::new();
            manifest_file.read_to_end(&mut bytes).map_err(|e| {
                PluginError::InitializationFailed(format!("Failed to read 'manifest.json': {}", e))
            })?;
            bytes
        };

        let manifest: PluginPackageManifest =
            serde_json::from_slice(&manifest_bytes).map_err(|e| {
                PluginError::InitializationFailed(format!("Invalid manifest.json: {}", e))
            })?;

        // 2. Create destination directory: target_dir/<plugin_id>/<version>
        let dest_dir = target_dir
            .join(&manifest.metadata.id)
            .join(&manifest.metadata.version);
        fs::create_dir_all(&dest_dir).map_err(|e| {
            PluginError::InitializationFailed(format!(
                "Failed to create target directory {}: {}",
                dest_dir.display(),
                e
            ))
        })?;

        // 3. Extract all files
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| {
                PluginError::InitializationFailed(format!(
                    "Failed to read entry {} from archive: {}",
                    i, e
                ))
            })?;

            let enclosed_name = match file.enclosed_name() {
                Some(path) => path.to_owned(),
                None => continue,
            };

            let outpath = dest_dir.join(enclosed_name);
            if file.name().ends_with('/') {
                fs::create_dir_all(&outpath).map_err(|e| {
                    PluginError::InitializationFailed(format!(
                        "Failed to create directory {}: {}",
                        outpath.display(),
                        e
                    ))
                })?;
            } else {
                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        fs::create_dir_all(p).map_err(|e| {
                            PluginError::InitializationFailed(format!(
                                "Failed to create parent directory {}: {}",
                                p.display(),
                                e
                            ))
                        })?;
                    }
                }
                let mut outfile = File::create(&outpath).map_err(|e| {
                    PluginError::InitializationFailed(format!(
                        "Failed to create extracted file {}: {}",
                        outpath.display(),
                        e
                    ))
                })?;
                std::io::copy(&mut file, &mut outfile).map_err(|e| {
                    PluginError::InitializationFailed(format!(
                        "Failed to extract file {}: {}",
                        outpath.display(),
                        e
                    ))
                })?;
            }
        }

        // 4. Verify payload existence and SHA-256 checksum
        let payload_path = dest_dir.join(&manifest.entrypoint);
        if !payload_path.exists() {
            return Err(PluginError::InitializationFailed(format!(
                "Package entrypoint payload '{}' not found in extracted files",
                manifest.entrypoint
            )));
        }

        let payload_bytes = fs::read(&payload_path).map_err(|e| {
            PluginError::InitializationFailed(format!(
                "Failed to read payload file {}: {}",
                payload_path.display(),
                e
            ))
        })?;

        let computed_checksum = Self::compute_sha256(&payload_bytes);
        if manifest.checksum != computed_checksum {
            return Err(PluginError::InitializationFailed(format!(
                "Payload SHA-256 checksum mismatch for '{}': expected '{}', computed '{}'",
                manifest.metadata.id, manifest.checksum, computed_checksum
            )));
        }

        Ok(VerifiedPluginPackage {
            manifest,
            extracted_dir: dest_dir,
            payload_path,
        })
    }

    /// Verify capabilities against an allowed policy set.
    pub fn verify_capabilities(
        manifest: &PluginPackageManifest,
        allowed_caps: &HashSet<String>,
    ) -> Result<(), PluginError> {
        for cap in &manifest.metadata.capabilities {
            if !allowed_caps.contains(cap) {
                return Err(PluginError::CapabilityViolation(format!(
                    "Plugin '{}' requests forbidden capability '{}'",
                    manifest.metadata.id, cap
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mythos_plugin_sdk::PluginType;

    fn sample_manifest() -> PluginPackageManifest {
        PluginPackageManifest {
            metadata: PluginMetadata {
                id: "com.vendor.testdriver".to_string(),
                name: "Test Driver".to_string(),
                version: "1.2.0".to_string(),
                author: "MythOS Team".to_string(),
                description: "Test plugin package".to_string(),
                plugin_type: PluginType::ProtocolDriver,
                capabilities: vec!["tagbus:read".to_string(), "net:listen".to_string()],
            },
            entrypoint: "bin/driver.so".to_string(),
            checksum: String::new(),
            min_core_version: "0.1.0".to_string(),
        }
    }

    #[test]
    fn test_package_build_extract_and_verify_success() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg_path = tmp.path().join("testdriver-1.2.0.mythos-plugin");
        let extract_target = tmp.path().join("installed_plugins");

        let manifest = sample_manifest();
        let dummy_payload = b"FAKE_ELF_SHARED_OBJECT_BINARY_CONTENT_12345";

        // Build
        PluginPackager::build_package(&manifest, dummy_payload, &pkg_path).unwrap();
        assert!(pkg_path.exists());

        // Extract and verify
        let verified = PluginPackager::extract_and_verify(&pkg_path, &extract_target).unwrap();
        assert_eq!(verified.manifest.metadata.id, "com.vendor.testdriver");
        assert_eq!(verified.manifest.metadata.version, "1.2.0");
        assert_eq!(
            verified.manifest.checksum,
            PluginPackager::compute_sha256(dummy_payload)
        );
        assert!(verified.payload_path.exists());
        let read_back = fs::read(&verified.payload_path).unwrap();
        assert_eq!(read_back, dummy_payload);

        // Verify capabilities success
        let mut allowed = HashSet::new();
        allowed.insert("tagbus:read".to_string());
        allowed.insert("net:listen".to_string());
        allowed.insert("serial:rw".to_string());
        assert!(PluginPackager::verify_capabilities(&verified.manifest, &allowed).is_ok());

        // Verify capability violation rejection
        let mut restricted = HashSet::new();
        restricted.insert("tagbus:read".to_string());
        let cap_err = PluginPackager::verify_capabilities(&verified.manifest, &restricted);
        assert!(matches!(cap_err, Err(PluginError::CapabilityViolation(_))));
    }

    #[test]
    fn test_corrupted_checksum_fails_verification() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg_path = tmp.path().join("tampered.mythos-plugin");
        let extract_target = tmp.path().join("installed_plugins");

        let mut manifest = sample_manifest();
        manifest.checksum =
            "0000000000000000000000000000000000000000000000000000000000000000".to_string();
        let dummy_payload = b"INTEGRITY_CHECK_DATA";

        // Building with explicitly wrong checksum returns error
        let err = PluginPackager::build_package(&manifest, dummy_payload, &pkg_path);
        assert!(err.is_err());

        // If built with auto checksum and then payload replaced in zip
        let mut valid_manifest = sample_manifest();
        PluginPackager::build_package(&valid_manifest, dummy_payload, &pkg_path).unwrap();

        // Tamper zip manually
        let tampered_pkg_path = tmp.path().join("tampered_archive.mythos-plugin");
        {
            valid_manifest.checksum =
                "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".to_string();
            let file = File::create(&tampered_pkg_path).unwrap();
            let mut zip = ZipWriter::new(file);
            let options = FileOptions::default();
            zip.start_file("manifest.json", options).unwrap();
            let mb = serde_json::to_vec(&valid_manifest).unwrap();
            zip.write_all(&mb).unwrap();
            zip.start_file("bin/driver.so", options).unwrap();
            zip.write_all(b"TAMPERED_BYTES").unwrap();
            zip.finish().unwrap();
        }

        let verify_res = PluginPackager::extract_and_verify(&tampered_pkg_path, &extract_target);
        assert!(matches!(
            verify_res,
            Err(PluginError::InitializationFailed(_))
        ));
    }
}
