//! Multi-project repository and atomic symlink manager for target controllers.

//! Multi-project repository and atomic symlink manager for target controllers.

pub mod project;
pub mod wal;

pub use project::*;
pub use wal::*;

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub const DEFAULT_PROJECTS_DIR: &str = "/var/lib/mythos/projects";

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("I/O error during storage operation: {0}")]
    Io(#[from] std::io::Error),

    #[error("WAL error: {0}")]
    Wal(#[from] WalError),

    #[error("Project '{0}' does not exist in repository")]
    ProjectNotFound(String),

    #[error("Project '{0}' already exists in repository")]
    ProjectAlreadyExists(String),

    #[error("JSON serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("No active project currently designated")]
    NoActiveProject,
}

pub struct ControllerStorage {
    root_dir: PathBuf,
}

impl ControllerStorage {
    /// Initialize controller storage manager targeting the given directory (e.g. `/var/lib/mythos/projects`).
    pub fn new<P: AsRef<Path>>(root_dir: P) -> Result<Self, StorageError> {
        let path = root_dir.as_ref().to_path_buf();
        fs::create_dir_all(&path)?;
        Ok(Self { root_dir: path })
    }

    /// Default production controller storage backed by `/var/lib/mythos/projects`.
    pub fn default_production() -> Result<Self, StorageError> {
        Self::new(DEFAULT_PROJECTS_DIR)
    }

    /// Root directory path.
    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    /// Path to the atomic 'active' symlink (`<root>/active`).
    pub fn active_symlink_path(&self) -> PathBuf {
        self.root_dir.join("active")
    }

    /// Path to a specific project directory (`<root>/<project_name>`).
    pub fn project_dir(&self, project_name: &str) -> PathBuf {
        self.root_dir.join(project_name)
    }

    /// Create a new project layout on disk with full directory hierarchy.
    pub fn create_project(&self, config: &ProjectConfig) -> Result<PathBuf, StorageError> {
        let pdir = self.project_dir(&config.id);
        if pdir.exists() {
            return Err(StorageError::ProjectAlreadyExists(config.id.clone()));
        }

        // Create standard directory tree
        fs::create_dir_all(pdir.join("logic/active"))?;
        fs::create_dir_all(pdir.join("logic/draft"))?;
        fs::create_dir_all(pdir.join("screens/active"))?;
        fs::create_dir_all(pdir.join("screens/draft"))?;
        fs::create_dir_all(pdir.join("faceplates/active"))?;
        fs::create_dir_all(pdir.join("faceplates/draft"))?;

        // Write initial project.active.json and project.draft.json atomically
        let json_data = serde_json::to_string_pretty(config)?;
        self.atomic_write_file(&pdir.join("project.active.json"), json_data.as_bytes())?;
        self.atomic_write_file(&pdir.join("project.draft.json"), json_data.as_bytes())?;

        Ok(pdir)
    }

    /// List all project IDs available in the repository.
    pub fn list_projects(&self) -> Result<Vec<String>, StorageError> {
        let mut list = Vec::new();
        for entry in fs::read_dir(&self.root_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() && entry.file_name() != "active" {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    list.push(name.to_string());
                }
            }
        }
        list.sort();
        Ok(list)
    }

    /// Resolve the name of the currently active project via the `active` symlink.
    /// Retrieve project configuration descriptor (`project.active.json` or `project.draft.json`).
    pub fn get_project(&self, project_name: &str) -> Result<ProjectConfig, StorageError> {
        let pdir = self.project_dir(project_name);
        if !pdir.exists() {
            return Err(StorageError::ProjectNotFound(project_name.to_string()));
        }

        let active_file = pdir.join("project.active.json");
        let draft_file = pdir.join("project.draft.json");

        let target_file = if active_file.exists() {
            active_file
        } else if draft_file.exists() {
            draft_file
        } else {
            return Err(StorageError::ProjectNotFound(project_name.to_string()));
        };

        let content = fs::read_to_string(&target_file)?;
        let config: ProjectConfig = serde_json::from_str(&content)?;
        Ok(config)
    }

    /// Alias for switch_active_project to activate a project.
    pub fn activate_project(&self, project_name: &str) -> Result<(), StorageError> {
        self.switch_active_project(project_name)
    }

    pub fn get_active_project(&self) -> Result<Option<String>, StorageError> {
        let symlink = self.active_symlink_path();
        if !symlink.exists() && fs::symlink_metadata(&symlink).is_err() {
            return Ok(None);
        }

        let target = fs::read_link(&symlink)?;
        let name = target
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string());
        Ok(name)
    }

    /// Atomically switch the active project using a temporary symlink and atomic rename.
    pub fn switch_active_project(&self, project_name: &str) -> Result<(), StorageError> {
        let target_dir = self.project_dir(project_name);
        if !target_dir.exists() {
            return Err(StorageError::ProjectNotFound(project_name.to_string()));
        }

        let active_link = self.active_symlink_path();
        let tmp_link = self
            .root_dir
            .join(format!(".active_tmp_{}", std::process::id()));

        // Remove any stale temporary link
        let _ = fs::remove_file(&tmp_link);

        #[cfg(unix)]
        std::os::unix::fs::symlink(&target_dir, &tmp_link)?;

        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&target_dir, &tmp_link)?;

        // Atomic rename replaces the active symlink in a single filesystem metadata swap
        fs::rename(&tmp_link, &active_link)?;

        Ok(())
    }

    /// Write file atomically using sibling temporary file and POSIX rename.
    pub fn atomic_write_file(
        &self,
        target_path: &Path,
        content: &[u8],
    ) -> Result<(), StorageError> {
        let parent = target_path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;

        let filename = target_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("tmp");
        let tmp_path = parent.join(format!(".{}_{}.tmp", filename, std::process::id()));

        {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&tmp_path)?;
            file.write_all(content)?;
            file.sync_all()?;
        }

        fs::rename(&tmp_path, target_path)?;
        Ok(())
    }

    /// Commit staged drafts to active production with automatic rollback capability.
    pub fn commit_draft_to_active(&self, project_name: &str) -> Result<(), StorageError> {
        let pdir = self.project_dir(project_name);
        if !pdir.exists() {
            return Err(StorageError::ProjectNotFound(project_name.to_string()));
        }

        let draft_project = pdir.join("project.draft.json");
        let active_project = pdir.join("project.active.json");
        let backup_project = pdir.join("project.active.json.bak");

        if draft_project.exists() {
            // Backup active for rollback
            if active_project.exists() {
                fs::copy(&active_project, &backup_project)?;
            }

            let draft_content = fs::read(&draft_project)?;
            if let Err(e) = self.atomic_write_file(&active_project, &draft_content) {
                // Rollback if active backup exists
                if backup_project.exists() {
                    let _ = fs::rename(&backup_project, &active_project);
                }
                return Err(e);
            }

            // Cleanup backup upon successful commit
            let _ = fs::remove_file(backup_project);
        }

        // Flushes and clears WAL if present
        let wal_path = pdir.join("project.wal");
        let wal = ProjectWal::new(wal_path);
        wal.truncate()?;

        Ok(())
    }

    /// Get WAL handle for the specified project.
    pub fn get_wal(&self, project_name: &str) -> ProjectWal {
        let wal_path = self.project_dir(project_name).join("project.wal");
        ProjectWal::new(wal_path)
    }

    /// Recover any uncheckpointed WAL edits into active project files.
    pub fn recover_project_wal(&self, project_name: &str) -> Result<usize, StorageError> {
        let pdir = self.project_dir(project_name);
        let wal = self.get_wal(project_name);

        let entries = wal.replay()?;
        if entries.is_empty() {
            return Ok(0);
        }

        let count = entries.len();
        for entry in entries {
            let target_file = pdir.join(&entry.doc_path);
            self.atomic_write_file(&target_file, &entry.payload)?;
        }

        wal.truncate()?;
        Ok(count)
    }
}
