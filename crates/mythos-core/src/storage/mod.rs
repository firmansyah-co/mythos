//! Multi-project repository for target controllers.

pub mod project;
pub mod wal;

pub use project::*;
pub use wal::*;

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Where a controller keeps its projects when `MYTHOS_STORAGE_ROOT` does not say: the place each
/// operating system sets aside for a service's data.
///
/// * Linux and other Unix: `/var/lib/mythos/projects`
/// * macOS: `/Library/Application Support/MythOS/projects`
/// * Windows: `%ProgramData%\MythOS\projects` (normally `C:\ProgramData\MythOS\projects`)
pub fn default_projects_dir() -> PathBuf {
    #[cfg(windows)]
    {
        let base = std::env::var_os("ProgramData")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
        base.join("MythOS").join("projects")
    }
    #[cfg(target_os = "macos")]
    {
        PathBuf::from("/Library/Application Support/MythOS/projects")
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        PathBuf::from("/var/lib/mythos/projects")
    }
}

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

    #[error(
        "'{0}' is not a valid project id: use 1 to {MAX_PROJECT_ID_LEN} ASCII letters, digits, \
         '_' or '-' (and not the reserved name 'active')"
    )]
    InvalidProjectId(String),
}

/// Longest accepted project id, in bytes.
pub const MAX_PROJECT_ID_LEN: usize = 128;

/// Name of the marker that designates the active project. It lives beside the project
/// directories, so no project may be called this.
const ACTIVE_LINK_NAME: &str = "active";

/// Whether `id` may name a project.
///
/// A project id becomes a directory name directly under the storage root, so it must be a
/// single, ordinary path component: no separator, no `.` or `..`, nothing absolute. The
/// alphabet is deliberately narrow rather than "anything but `/`", because every character
/// outside it is one somebody would later have to prove harmless.
pub fn is_valid_project_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_PROJECT_ID_LEN
        && id != ACTIVE_LINK_NAME
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

pub struct ControllerStorage {
    root_dir: PathBuf,
}

impl ControllerStorage {
    /// Initialize controller storage manager targeting the given directory.
    pub fn new<P: AsRef<Path>>(root_dir: P) -> Result<Self, StorageError> {
        let path = root_dir.as_ref().to_path_buf();
        fs::create_dir_all(&path)?;
        Ok(Self { root_dir: path })
    }

    /// Controller storage in this platform's default place (see [`default_projects_dir`]).
    pub fn default_production() -> Result<Self, StorageError> {
        Self::new(default_projects_dir())
    }

    /// Root directory path.
    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    /// Path to the marker naming the active project (`<root>/active`).
    pub fn active_marker_path(&self) -> PathBuf {
        self.root_dir.join(ACTIVE_LINK_NAME)
    }

    /// Path to a specific project directory (`<root>/<project_name>`).
    ///
    /// This is the only place a project id is turned into a path, and it refuses any id that is
    /// not a valid one. An id reaches here from request bodies and URL path segments; joined
    /// unchecked, `../x` or an absolute path would name a directory outside the storage root.
    pub fn project_dir(&self, project_name: &str) -> Result<PathBuf, StorageError> {
        if !is_valid_project_id(project_name) {
            return Err(StorageError::InvalidProjectId(project_name.to_string()));
        }
        Ok(self.root_dir.join(project_name))
    }

    /// Create a new project layout on disk with full directory hierarchy.
    pub fn create_project(&self, config: &ProjectConfig) -> Result<PathBuf, StorageError> {
        let pdir = self.project_dir(&config.id)?;
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
            if path.is_dir() && entry.file_name() != ACTIVE_LINK_NAME {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    list.push(name.to_string());
                }
            }
        }
        list.sort();
        Ok(list)
    }

    /// Retrieve project configuration descriptor (`project.active.json` or `project.draft.json`).
    pub fn get_project(&self, project_name: &str) -> Result<ProjectConfig, StorageError> {
        let pdir = self.project_dir(project_name)?;
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

    /// The active project, from the `active` marker.
    ///
    /// The marker is a file holding the project's id. Earlier versions made it a symlink to the
    /// project's directory, which Windows refuses to create without administrator rights or
    /// Developer Mode; a symlink left by one of them is still read, and the next activation
    /// replaces it with a file.
    pub fn get_active_project(&self) -> Result<Option<String>, StorageError> {
        let marker = self.active_marker_path();
        let meta = match fs::symlink_metadata(&marker) {
            Ok(meta) => meta,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };

        let id = if meta.file_type().is_symlink() {
            fs::read_link(&marker)?
                .file_name()
                .and_then(|n| n.to_str())
                .map(str::to_string)
                .unwrap_or_default()
        } else {
            fs::read_to_string(&marker)?.trim().to_string()
        };

        // A marker that does not name a valid project is damaged. Saying "no active project"
        // would hide that; so would guessing one.
        if !is_valid_project_id(&id) {
            return Err(StorageError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "the active-project marker {} does not name a valid project: {:?}",
                    marker.display(),
                    id
                ),
            )));
        }
        Ok(Some(id))
    }

    /// Make `project_name` the active project, atomically: the marker is written beside itself
    /// and renamed over the old one, so a reader sees the old project or the new one, never
    /// neither.
    pub fn switch_active_project(&self, project_name: &str) -> Result<(), StorageError> {
        let target_dir = self.project_dir(project_name)?;
        if !target_dir.exists() {
            return Err(StorageError::ProjectNotFound(project_name.to_string()));
        }
        self.atomic_write_file(&self.active_marker_path(), project_name.as_bytes())
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
        let pdir = self.project_dir(project_name)?;
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
    pub fn get_wal(&self, project_name: &str) -> Result<ProjectWal, StorageError> {
        let wal_path = self.project_dir(project_name)?.join("project.wal");
        Ok(ProjectWal::new(wal_path))
    }

    /// Recover any uncheckpointed WAL edits into active project files.
    pub fn recover_project_wal(&self, project_name: &str) -> Result<usize, StorageError> {
        let pdir = self.project_dir(project_name)?;
        let wal = self.get_wal(project_name)?;

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
