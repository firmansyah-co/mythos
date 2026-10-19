//! # 1-Click Encrypted Disaster Recovery Engine
//!
//! Provides automated packing of project configuration, IEC 61131-3 logic programs,
//! screens, faceplates, ISA-88 recipes, and user credentials into encrypted `.mythos-pkg` archives.
//! Supports hot restoration of the complete operational state in under 25 seconds.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::time::Instant;
use thiserror::Error;
use zip::write::FileOptions;
use zip::{ZipArchive, ZipWriter};

use super::container::{PackageError, PackageHeader, PackageManifest, HEADER_LEN};
use super::crypto::{decrypt_payload, encrypt_payload, CryptoError};
use crate::recipe::MasterRecipe;
use crate::storage::{ControllerStorage, ProjectConfig, StorageError, TaskConfig};
use sha2::Digest;

#[derive(Error, Debug)]
pub enum RecoveryError {
    #[error("Package formatting error: {0}")]
    Package(#[from] PackageError),

    #[error("Cryptography error: {0}")]
    Crypto(#[from] CryptoError),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("ZIP archive error: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("JSON serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Restore failed: {0}")]
    RestoreFailed(String),
}

/// User credentials and access scopes stored in disaster recovery package.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserRecord {
    pub username: String,
    pub role: String,
    pub scopes: Vec<String>,
}

/// Unpacked disaster recovery container holding all state artifacts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisasterRecoveryBundle {
    pub manifest: PackageManifest,
    pub project: ProjectConfig,
    pub tasks: Vec<TaskConfig>,
    pub logic_files: HashMap<String, String>,
    pub screens: HashMap<String, serde_json::Value>,
    pub faceplates: HashMap<String, serde_json::Value>,
    pub recipes: Vec<MasterRecipe>,
    pub users: Vec<UserRecord>,
}

/// Outcome report returned after hot-restoration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreReport {
    pub project_id: String,
    pub project_name: String,
    pub restored_files_count: usize,
    pub recipes_count: usize,
    pub users_count: usize,
    pub duration_ms: u64,
    pub meets_sla_25s: bool,
    pub target_directory: String,
}

/// Core Disaster Recovery Packager and Hot-Restore Engine.
pub struct RecoveryEngine;

impl RecoveryEngine {
    /// Pack a complete project from ControllerStorage into an encrypted `.mythos-pkg` byte buffer.
    pub fn pack(
        project_name: &str,
        storage: &ControllerStorage,
        passphrase: &str,
        recipes: &[MasterRecipe],
        users: &[UserRecord],
    ) -> Result<Vec<u8>, RecoveryError> {
        let pdir = storage.project_dir(project_name);
        if !pdir.exists() {
            return Err(StorageError::ProjectNotFound(project_name.to_string()).into());
        }

        let project_config = storage.get_project(project_name)?;

        // Collect tasks configuration
        let tasks_file = pdir.join("tasks.active.json");
        let tasks: Vec<TaskConfig> = if tasks_file.exists() {
            let data = fs::read_to_string(&tasks_file)?;
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Vec::new()
        };

        // Collect logic files
        let mut logic_files = HashMap::new();
        let logic_dir = pdir.join("logic/active");
        if logic_dir.exists() {
            for entry in fs::read_dir(logic_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    let fname = entry.file_name().to_string_lossy().to_string();
                    let content = fs::read_to_string(&path)?;
                    logic_files.insert(fname, content);
                }
            }
        }

        // Collect screens
        let mut screens: HashMap<String, serde_json::Value> = HashMap::new();
        let screens_dir = pdir.join("screens/active");
        if screens_dir.exists() {
            for entry in fs::read_dir(screens_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    let fname = entry.file_name().to_string_lossy().to_string();
                    let content = fs::read_to_string(&path)?;
                    if let Ok(val) = serde_json::from_str(&content) {
                        screens.insert(fname, val);
                    }
                }
            }
        }

        // Collect faceplates
        let mut faceplates: HashMap<String, serde_json::Value> = HashMap::new();
        let faceplates_dir = pdir.join("faceplates/active");
        if faceplates_dir.exists() {
            for entry in fs::read_dir(faceplates_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    let fname = entry.file_name().to_string_lossy().to_string();
                    let content = fs::read_to_string(&path)?;
                    if let Ok(val) = serde_json::from_str(&content) {
                        faceplates.insert(fname, val);
                    }
                }
            }
        }

        // Build list of all file entries for manifest
        let mut file_list = Vec::new();
        for k in logic_files.keys() {
            file_list.push(format!("logic/{}", k));
        }
        for k in screens.keys() {
            file_list.push(format!("screens/{}", k));
        }
        for k in faceplates.keys() {
            file_list.push(format!("faceplates/{}", k));
        }
        file_list.push("project.json".to_string());
        file_list.push("tasks.json".to_string());
        file_list.push("recipes.json".to_string());
        file_list.push("users.json".to_string());
        file_list.sort();

        // Create in-memory zip payload
        let mut zip_buffer = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut zip_buffer);
            let options =
                FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

            // Write project.json
            zip.start_file("project.json", options)?;
            zip.write_all(serde_json::to_string_pretty(&project_config)?.as_bytes())?;

            // Write tasks.json
            zip.start_file("tasks.json", options)?;
            zip.write_all(serde_json::to_string_pretty(&tasks)?.as_bytes())?;

            // Write logic files
            for (fname, content) in &logic_files {
                zip.start_file(format!("logic/{}", fname), options)?;
                zip.write_all(content.as_bytes())?;
            }

            // Write screens
            for (fname, content) in &screens {
                zip.start_file(format!("screens/{}", fname), options)?;
                zip.write_all(serde_json::to_string_pretty(content)?.as_bytes())?;
            }

            // Write faceplates
            for (fname, content) in &faceplates {
                zip.start_file(format!("faceplates/{}", fname), options)?;
                zip.write_all(serde_json::to_string_pretty(content)?.as_bytes())?;
            }

            // Write recipes
            zip.start_file("recipes.json", options)?;
            zip.write_all(serde_json::to_string_pretty(recipes)?.as_bytes())?;

            // Write users
            zip.start_file("users.json", options)?;
            zip.write_all(serde_json::to_string_pretty(users)?.as_bytes())?;

            // Compute content checksum across bundle components
            let mut hasher = sha2::Sha256::new();
            hasher.update(project_config.id.as_bytes());
            hasher.update(project_config.version.as_bytes());
            for (k, v) in &logic_files {
                hasher.update(k.as_bytes());
                hasher.update(v.as_bytes());
            }
            let checksum = format!("{:x}", sha2::Digest::finalize(hasher));

            let manifest = PackageManifest {
                project_id: project_config.id.clone(),
                project_name: project_config.name.clone(),
                project_version: project_config.version.clone(),
                created_at_utc: chrono_stub_utc(),
                content_sha256: checksum,
                files: file_list,
                recipe_count: recipes.len(),
                user_count: users.len(),
                screen_count: screens.len(),
                logic_file_count: logic_files.len(),
            };

            zip.start_file("manifest.json", options)?;
            zip.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes())?;

            zip.finish()?;
        }

        let unencrypted_payload = zip_buffer.into_inner();

        // Encrypt with ChaCha20-Poly1305
        let (salt, nonce, ciphertext) = encrypt_payload(&unencrypted_payload, passphrase)?;

        // Build 64-byte header
        let header = PackageHeader::new(salt, nonce, ciphertext.len() as u64);

        // Assemble final container: Header + Ciphertext (with Poly1305 tag)
        let mut final_package = Vec::with_capacity(HEADER_LEN + ciphertext.len());
        final_package.extend_from_slice(&header.to_bytes());
        final_package.extend_from_slice(&ciphertext);

        Ok(final_package)
    }

    /// Pack a project directly to a file on disk (e.g. `/path/to/archive.mythos-pkg`).
    pub fn pack_to_file<P: AsRef<Path>>(
        project_name: &str,
        storage: &ControllerStorage,
        passphrase: &str,
        recipes: &[MasterRecipe],
        users: &[UserRecord],
        output_path: P,
    ) -> Result<(), RecoveryError> {
        let pkg_bytes = Self::pack(project_name, storage, passphrase, recipes, users)?;
        let mut file = File::create(output_path)?;
        file.write_all(&pkg_bytes)?;
        file.sync_all()?;
        Ok(())
    }

    /// Verify package integrity and return manifest without extracting files.
    pub fn inspect_manifest(
        package_bytes: &[u8],
        passphrase: &str,
    ) -> Result<PackageManifest, RecoveryError> {
        let bundle = Self::unpack(package_bytes, passphrase)?;
        Ok(bundle.manifest)
    }

    /// Unpack and decrypt a `.mythos-pkg` byte buffer into a DisasterRecoveryBundle.
    pub fn unpack(
        package_bytes: &[u8],
        passphrase: &str,
    ) -> Result<DisasterRecoveryBundle, RecoveryError> {
        if package_bytes.len() < HEADER_LEN {
            return Err(PackageError::UnexpectedEof.into());
        }

        // Parse header
        let header = PackageHeader::from_bytes(&package_bytes[0..HEADER_LEN])?;
        let ciphertext = &package_bytes[HEADER_LEN..];

        if ciphertext.len() != header.payload_len as usize {
            return Err(PackageError::PayloadLengthMismatch {
                expected: header.payload_len as usize,
                actual: ciphertext.len(),
            }
            .into());
        }

        // Decrypt using ChaCha20-Poly1305
        let decrypted_payload =
            decrypt_payload(ciphertext, passphrase, &header.salt, &header.nonce)?;

        // Open zip archive
        let cursor = Cursor::new(decrypted_payload);
        let mut zip = ZipArchive::new(cursor)?;

        // Read and parse manifest
        let manifest: PackageManifest = {
            let mut file = zip.by_name("manifest.json")?;
            let mut content = String::new();
            file.read_to_string(&mut content)?;
            serde_json::from_str(&content)?
        };

        // Read project.json
        let project: ProjectConfig = {
            let mut file = zip.by_name("project.json")?;
            let mut content = String::new();
            file.read_to_string(&mut content)?;
            serde_json::from_str(&content)?
        };

        // Read tasks.json
        let tasks: Vec<TaskConfig> = if let Ok(mut file) = zip.by_name("tasks.json") {
            let mut content = String::new();
            file.read_to_string(&mut content)?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Vec::new()
        };

        // Read recipes.json
        let recipes: Vec<MasterRecipe> = if let Ok(mut file) = zip.by_name("recipes.json") {
            let mut content = String::new();
            file.read_to_string(&mut content)?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Vec::new()
        };

        // Read users.json
        let users: Vec<UserRecord> = if let Ok(mut file) = zip.by_name("users.json") {
            let mut content = String::new();
            file.read_to_string(&mut content)?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Vec::new()
        };

        // Read logic, screens, faceplates
        let mut logic_files = HashMap::new();
        let mut screens = HashMap::new();
        let mut faceplates = HashMap::new();

        for i in 0..zip.len() {
            let mut file = zip.by_index(i)?;
            let name = file.name().to_string();

            if name.starts_with("logic/") {
                let fname = name.trim_start_matches("logic/").to_string();
                if !fname.is_empty() {
                    let mut content = String::new();
                    file.read_to_string(&mut content)?;
                    logic_files.insert(fname, content);
                }
            } else if name.starts_with("screens/") {
                let fname = name.trim_start_matches("screens/").to_string();
                if !fname.is_empty() {
                    let mut content = String::new();
                    file.read_to_string(&mut content)?;
                    if let Ok(val) = serde_json::from_str(&content) {
                        screens.insert(fname, val);
                    }
                }
            } else if name.starts_with("faceplates/") {
                let fname = name.trim_start_matches("faceplates/").to_string();
                if !fname.is_empty() {
                    let mut content = String::new();
                    file.read_to_string(&mut content)?;
                    if let Ok(val) = serde_json::from_str(&content) {
                        faceplates.insert(fname, val);
                    }
                }
            }
        }

        Ok(DisasterRecoveryBundle {
            manifest,
            project,
            tasks,
            logic_files,
            screens,
            faceplates,
            recipes,
            users,
        })
    }

    /// Hot-Restore a DisasterRecoveryBundle into target ControllerStorage,
    /// atomically activating the project and verifying MTTR < 25 seconds.
    pub fn hot_restore(
        bundle: &DisasterRecoveryBundle,
        storage: &ControllerStorage,
        target_name_override: Option<&str>,
    ) -> Result<RestoreReport, RecoveryError> {
        let start_time = Instant::now();

        let project_name = target_name_override.unwrap_or(&bundle.project.id);
        let pdir = storage.project_dir(project_name);

        // Ensure clean directory layout
        fs::create_dir_all(pdir.join("logic/active"))?;
        fs::create_dir_all(pdir.join("logic/draft"))?;
        fs::create_dir_all(pdir.join("screens/active"))?;
        fs::create_dir_all(pdir.join("screens/draft"))?;
        fs::create_dir_all(pdir.join("faceplates/active"))?;
        fs::create_dir_all(pdir.join("faceplates/draft"))?;
        fs::create_dir_all(pdir.join("recipes/active"))?;
        fs::create_dir_all(pdir.join("users"))?;

        let mut files_restored = 0;

        // Write project configuration
        let mut proj_cfg = bundle.project.clone();
        if let Some(target) = target_name_override {
            proj_cfg.id = target.to_string();
            proj_cfg.name = target.to_string();
        }
        let proj_json = serde_json::to_string_pretty(&proj_cfg)?;
        storage.atomic_write_file(&pdir.join("project.active.json"), proj_json.as_bytes())?;
        storage.atomic_write_file(&pdir.join("project.draft.json"), proj_json.as_bytes())?;
        files_restored += 2;

        // Write tasks configuration
        let tasks_json = serde_json::to_string_pretty(&bundle.tasks)?;
        storage.atomic_write_file(&pdir.join("tasks.active.json"), tasks_json.as_bytes())?;
        files_restored += 1;

        // Restore logic files
        for (fname, content) in &bundle.logic_files {
            let active_path = pdir.join("logic/active").join(fname);
            let draft_path = pdir.join("logic/draft").join(fname);
            storage.atomic_write_file(&active_path, content.as_bytes())?;
            storage.atomic_write_file(&draft_path, content.as_bytes())?;
            files_restored += 2;
        }

        // Restore screens
        for (fname, content) in &bundle.screens {
            let active_path = pdir.join("screens/active").join(fname);
            let draft_path = pdir.join("screens/draft").join(fname);
            let json_str = serde_json::to_string_pretty(content)?;
            storage.atomic_write_file(&active_path, json_str.as_bytes())?;
            storage.atomic_write_file(&draft_path, json_str.as_bytes())?;
            files_restored += 2;
        }

        // Restore faceplates
        for (fname, content) in &bundle.faceplates {
            let active_path = pdir.join("faceplates/active").join(fname);
            let draft_path = pdir.join("faceplates/draft").join(fname);
            let json_str = serde_json::to_string_pretty(content)?;
            storage.atomic_write_file(&active_path, json_str.as_bytes())?;
            storage.atomic_write_file(&draft_path, json_str.as_bytes())?;
            files_restored += 2;
        }

        // Restore recipes
        let recipes_json = serde_json::to_string_pretty(&bundle.recipes)?;
        storage.atomic_write_file(
            &pdir.join("recipes/active/recipes.json"),
            recipes_json.as_bytes(),
        )?;
        files_restored += 1;

        // Restore users
        let users_json = serde_json::to_string_pretty(&bundle.users)?;
        storage.atomic_write_file(&pdir.join("users/users.json"), users_json.as_bytes())?;
        files_restored += 1;

        // Atomically activate project
        storage.switch_active_project(project_name)?;

        let duration = start_time.elapsed();
        let duration_ms = duration.as_millis() as u64;
        let meets_sla_25s = duration_ms <= 25_000;

        Ok(RestoreReport {
            project_id: project_name.to_string(),
            project_name: bundle.project.name.clone(),
            restored_files_count: files_restored,
            recipes_count: bundle.recipes.len(),
            users_count: bundle.users.len(),
            duration_ms,
            meets_sla_25s,
            target_directory: pdir.to_string_lossy().to_string(),
        })
    }
}

fn chrono_stub_utc() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}-01-01T00:00:00Z", 1970 + now / 31536000)
}
