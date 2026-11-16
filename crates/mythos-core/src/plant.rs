//! Plant projects (ADR-012 §2.4, §2.5): one project engineers a whole plant of MythOS devices,
//! each downloaded to its node and compared with what that node runs.
//!
//! A **device** is a slot in the plant: a name, the project that holds its configuration (its
//! I/O, the HMI module's screens, later its logic: a project like any other, so every editor and
//! module works on it unchanged), and the node it is **assigned** to, explicitly, from the nodes
//! this one is paired with (or this node itself). The plant's devices are kept in the plant
//! project's `plant.json`. A project without one is a plant of one device, "this controller",
//! on this node: nothing of an earlier project is lost or moved.
//!
//! A device's **slice** is its project's files. Downloading sends the slice; the node stages it,
//! keeps what it ran before, and makes the new one current, or returns to the old one. Its
//! **hash** (SHA-256 over every file's path and content, in order) is how a plant compares what
//! a node runs with what the project says.

use crate::storage::{is_valid_project_id, ControllerStorage, StorageError};
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};

pub const PLANT_FILE: &str = "plant.json";
/// Where a node keeps the slice it ran before the last download, per project.
const PREVIOUS_DIR: &str = "deploy-previous";

/// One device of a plant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    /// Unique in the plant: letters, digits, `-`, `_`.
    pub id: String,
    pub name: String,
    /// The project holding its configuration.
    pub project: String,
    /// The node it is assigned to: this node's id, a paired node's, or none yet.
    #[serde(default)]
    pub node: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plant {
    pub devices: Vec<Device>,
}

#[derive(Debug, thiserror::Error)]
pub enum PlantError {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    NotFound(String),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// The device a project without `plant.json` is: itself, on this node.
pub fn this_controller(project: &str, own_node: &str) -> Device {
    Device {
        id: "this".into(),
        name: "This controller".into(),
        project: project.into(),
        node: Some(own_node.into()),
    }
}

fn valid_device_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The project a new device of `plant` gets: `<plant>--<device>`.
pub fn device_project(plant: &str, device: &str) -> String {
    format!("{plant}--{device}")
}

/// Read a plant: its `plant.json`, or the one device it is. Returns whether it was implicit.
pub fn read(
    storage: &ControllerStorage,
    project: &str,
    own_node: &str,
) -> Result<(Plant, bool), PlantError> {
    let dir = storage.project_dir(project)?;
    match std::fs::read(dir.join(PLANT_FILE)) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(|plant| (plant, false))
            .map_err(|e| PlantError::Invalid(format!("{PLANT_FILE} is not readable: {e}"))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((
            Plant {
                devices: vec![this_controller(project, own_node)],
            },
            true,
        )),
        Err(e) => Err(e.into()),
    }
}

pub fn write(storage: &ControllerStorage, project: &str, plant: &Plant) -> Result<(), PlantError> {
    let dir = storage.project_dir(project)?;
    let bytes = serde_json::to_vec_pretty(plant).map_err(|e| PlantError::Invalid(e.to_string()))?;
    storage.atomic_write_file(&dir.join(PLANT_FILE), &bytes)?;
    Ok(())
}

/// Add a device to the plant, or rename or reassign one. A new device gets a project of its own.
pub fn set_device(
    storage: &ControllerStorage,
    plant_id: &str,
    own_node: &str,
    device_id: &str,
    name: &str,
    node: Option<String>,
) -> Result<Device, PlantError> {
    if !valid_device_id(device_id) {
        return Err(PlantError::Invalid(format!(
            "'{device_id}' is not a device id: 1 to 32 letters, digits, '-' or '_'"
        )));
    }
    let name = name.trim();
    if name.is_empty() || name.chars().any(char::is_control) {
        return Err(PlantError::Invalid("a device needs a name".into()));
    }
    let (mut plant, _) = read(storage, plant_id, own_node)?;
    let device = match plant.devices.iter_mut().find(|d| d.id == device_id) {
        Some(device) => {
            device.name = name.to_string();
            device.node = node;
            device.clone()
        }
        None => {
            let project = device_project(plant_id, device_id);
            if !is_valid_project_id(&project) {
                return Err(PlantError::Invalid(format!(
                    "'{project}' cannot be a project id"
                )));
            }
            if storage.get_project(&project).is_err() {
                let base = storage.get_project(plant_id)?;
                let mut config = base.clone();
                config.id = project.clone();
                config.name = format!("{} — {name}", base.name);
                storage.create_project(&config)?;
            }
            let device = Device {
                id: device_id.to_string(),
                name: name.to_string(),
                project,
                node,
            };
            plant.devices.push(device.clone());
            device
        }
    };
    write(storage, plant_id, &plant)?;
    Ok(device)
}

/// Take a device out of the plant. Its project is kept.
pub fn remove_device(
    storage: &ControllerStorage,
    plant_id: &str,
    own_node: &str,
    device_id: &str,
) -> Result<Device, PlantError> {
    let (mut plant, _) = read(storage, plant_id, own_node)?;
    let at = plant
        .devices
        .iter()
        .position(|d| d.id == device_id)
        .ok_or_else(|| PlantError::NotFound(format!("no device '{device_id}' in this plant")))?;
    let device = plant.devices.remove(at);
    write(storage, plant_id, &plant)?;
    Ok(device)
}

/// A file of a slice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SliceFile {
    /// `/`-separated, inside the project.
    pub path: String,
    /// Its bytes, base64.
    pub data: String,
}

/// What a device runs: its project's files, and their hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slice {
    pub files: Vec<SliceFile>,
    pub hash: String,
}

/// Files that are not configuration: temporary files, and the plant's own device list.
fn excluded(relative: &str) -> bool {
    relative == PLANT_FILE
        || relative.ends_with(".tmp")
        || relative.split('/').any(|s| s.starts_with(".deploy"))
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_dir() {
            collect(root, &path, out)?;
        } else if kind.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|e| std::io::Error::other(e.to_string()))?
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            if !excluded(&relative) {
                out.push((relative, std::fs::read(&path)?));
            }
        }
    }
    Ok(())
}

/// The hash of a slice: SHA-256 over each file's path and the SHA-256 of its content, in path
/// order. Equal on two nodes exactly when they hold the same files.
pub fn hash_of(files: &[(String, Vec<u8>)]) -> String {
    let mut sorted: Vec<&(String, Vec<u8>)> = files.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = Sha256::new();
    for (path, data) in sorted {
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update(Sha256::digest(data));
    }
    hex::encode(hasher.finalize())
}

/// The slice of a project on this node.
pub fn pack(storage: &ControllerStorage, project: &str) -> Result<Slice, PlantError> {
    storage.get_project(project)?;
    let dir = storage.project_dir(project)?;
    let mut files = Vec::new();
    collect(&dir, &dir, &mut files)?;
    let hash = hash_of(&files);
    let engine = base64::engine::general_purpose::STANDARD;
    Ok(Slice {
        files: files
            .into_iter()
            .map(|(path, data)| SliceFile {
                path,
                data: engine.encode(data),
            })
            .collect(),
        hash,
    })
}

/// The hash of what this node holds for `project`, or `None` when it holds no such project.
pub fn hash_here(storage: &ControllerStorage, project: &str) -> Result<Option<String>, PlantError> {
    if storage.get_project(project).is_err() {
        return Ok(None);
    }
    let dir = storage.project_dir(project)?;
    let mut files = Vec::new();
    collect(&dir, &dir, &mut files)?;
    Ok(Some(hash_of(&files)))
}

/// A path of a slice as a path inside the project: refused when it is not plainly inside.
fn inside(path: &str) -> Option<PathBuf> {
    if path.is_empty() || path.contains('\\') || path.contains('\0') {
        return None;
    }
    let candidate = Path::new(path);
    let mut out = PathBuf::new();
    for component in candidate.components() {
        match component {
            Component::Normal(part) => out.push(part),
            _ => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

/// What a node keeps of the slice it ran before a download, to return to.
pub struct Previous {
    pub dir: Option<PathBuf>,
}

/// Install `slice` as `project` on this node: checked and written beside the project first,
/// then put in its place, the project it replaces kept to return to ([`restore`]).
pub fn install(
    storage: &ControllerStorage,
    project: &str,
    slice: &Slice,
) -> Result<Previous, PlantError> {
    let target = storage.project_dir(project)?;
    let engine = base64::engine::general_purpose::STANDARD;
    let mut files = Vec::with_capacity(slice.files.len());
    for file in &slice.files {
        let relative = inside(&file.path).ok_or_else(|| {
            PlantError::Invalid(format!("'{}' is not a path inside a project", file.path))
        })?;
        let data = engine
            .decode(&file.data)
            .map_err(|e| PlantError::Invalid(format!("'{}' is not base64: {e}", file.path)))?;
        files.push((file.path.clone(), relative, data));
    }
    let received: Vec<(String, Vec<u8>)> = files
        .iter()
        .map(|(path, _, data)| (path.clone(), data.clone()))
        .collect();
    if hash_of(&received) != slice.hash {
        return Err(PlantError::Invalid(
            "the files do not have the hash sent with them; nothing was installed".into(),
        ));
    }
    if !files
        .iter()
        .any(|(path, _, _)| path == "project.active.json")
    {
        return Err(PlantError::Invalid(
            "a slice without project.active.json is not a project".into(),
        ));
    }

    let root = storage.root_dir().to_path_buf();
    let staging = root.join(format!(".deploy-staging-{project}"));
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    for (_, relative, data) in &files {
        let path = staging.join(relative);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, data)?;
    }

    let previous = if target.exists() {
        let keep = storage.system_dir().join(PREVIOUS_DIR).join(project);
        if keep.exists() {
            std::fs::remove_dir_all(&keep)?;
        }
        std::fs::create_dir_all(keep.parent().unwrap_or(&root))?;
        std::fs::rename(&target, &keep)?;
        Some(keep)
    } else {
        None
    };
    if let Err(e) = std::fs::rename(&staging, &target) {
        if let Some(keep) = &previous {
            let _ = std::fs::rename(keep, &target);
        }
        return Err(e.into());
    }
    Ok(Previous { dir: previous })
}

/// Put back what `project` was before [`install`].
pub fn restore(
    storage: &ControllerStorage,
    project: &str,
    previous: &Previous,
) -> Result<(), PlantError> {
    let target = storage.project_dir(project)?;
    if target.exists() {
        std::fs::remove_dir_all(&target)?;
    }
    if let Some(keep) = &previous.dir {
        std::fs::rename(keep, &target)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::ProjectConfig;

    fn a_plant() -> (tempfile::TempDir, ControllerStorage) {
        let dir = tempfile::tempdir().unwrap();
        let storage = ControllerStorage::new(dir.path()).unwrap();
        storage
            .create_project(&ProjectConfig::new("plant_a", "Boiler House"))
            .unwrap();
        (dir, storage)
    }

    #[test]
    fn a_project_is_a_plant_of_one_device_until_one_is_added() {
        let (_dir, storage) = a_plant();
        let (plant, implicit) = read(&storage, "plant_a", "node-1").unwrap();
        assert!(implicit);
        assert_eq!(plant.devices, vec![this_controller("plant_a", "node-1")]);

        let added = set_device(
            &storage,
            "plant_a",
            "node-1",
            "edge1",
            "Boiler edge 1",
            None,
        )
        .unwrap();
        assert_eq!(added.project, "plant_a--edge1");
        assert_eq!(
            storage.get_project("plant_a--edge1").unwrap().name,
            "Boiler House — Boiler edge 1"
        );
        let (plant, implicit) = read(&storage, "plant_a", "node-1").unwrap();
        assert!(!implicit);
        assert_eq!(
            plant.devices.len(),
            2,
            "this controller, and the device added"
        );

        let assigned = set_device(
            &storage,
            "plant_a",
            "node-1",
            "edge1",
            "Boiler edge 1",
            Some("node-2".into()),
        )
        .unwrap();
        assert_eq!(assigned.node.as_deref(), Some("node-2"));
        assert!(set_device(&storage, "plant_a", "node-1", "../x", "x", None).is_err());

        remove_device(&storage, "plant_a", "node-1", "edge1").unwrap();
        assert!(
            storage.get_project("plant_a--edge1").is_ok(),
            "its project is kept"
        );
    }

    #[test]
    fn a_slice_installs_where_it_says_and_the_old_one_can_come_back() {
        let (_dir, storage) = a_plant();
        let dir = storage.project_dir("plant_a").unwrap();
        std::fs::write(dir.join("io.json"), r#"{"connections":[]}"#).unwrap();
        std::fs::create_dir_all(dir.join("hmi/screens/draft")).unwrap();
        std::fs::write(dir.join("hmi/screens/draft/screens.json"), "[]").unwrap();
        std::fs::write(dir.join("io.json.tmp"), "half").unwrap();
        let slice = pack(&storage, "plant_a").unwrap();
        let paths: Vec<&str> = slice.files.iter().map(|f| f.path.as_str()).collect();
        assert!(
            paths.contains(&"hmi/screens/draft/screens.json"),
            "{paths:?}"
        );
        assert!(
            !paths.contains(&"io.json.tmp"),
            "temporary files are not configuration"
        );

        let (_other, target) = a_plant();
        let before = hash_here(&target, "plant_a").unwrap().unwrap();
        assert_ne!(before, slice.hash);
        let previous = install(&target, "plant_a", &slice).unwrap();
        assert_eq!(hash_here(&target, "plant_a").unwrap().unwrap(), slice.hash);
        restore(&target, "plant_a", &previous).unwrap();
        assert_eq!(hash_here(&target, "plant_a").unwrap().unwrap(), before);
        assert_eq!(hash_here(&target, "nothing").unwrap(), None);
    }

    #[test]
    fn a_slice_that_reaches_outside_its_project_or_is_not_what_it_says_installs_nothing() {
        let (_dir, storage) = a_plant();
        let good = pack(&storage, "plant_a").unwrap();
        for path in ["../escape", "/etc/passwd", "a/../../b", "a\\b", ""] {
            let mut bad = good.clone();
            bad.files.push(SliceFile {
                path: path.into(),
                data: String::new(),
            });
            assert!(
                matches!(
                    install(&storage, "plant_a", &bad),
                    Err(PlantError::Invalid(_))
                ),
                "{path:?}"
            );
        }
        let mut tampered = good.clone();
        tampered.files[0].data = base64::engine::general_purpose::STANDARD.encode(b"other");
        assert!(matches!(
            install(&storage, "plant_a", &tampered),
            Err(PlantError::Invalid(_))
        ));
        assert_eq!(
            hash_here(&storage, "plant_a").unwrap().unwrap(),
            good.hash,
            "untouched"
        );
    }
}
