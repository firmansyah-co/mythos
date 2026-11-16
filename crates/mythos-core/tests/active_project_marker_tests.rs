//! The marker that names the active project: a plain file, so it works where symlinks need
//! privileges (Windows), and still read from the symlink earlier versions left behind.

use mythos_core::storage::{ControllerStorage, ProjectConfig};

fn storage_with(projects: &[&str]) -> (tempfile::TempDir, ControllerStorage) {
    let dir = tempfile::tempdir().unwrap();
    let storage = ControllerStorage::new(dir.path()).unwrap();
    for id in projects {
        storage
            .create_project(&ProjectConfig::new(*id, *id))
            .unwrap();
    }
    (dir, storage)
}

#[test]
fn activating_a_project_writes_a_file_naming_it() {
    let (_dir, storage) = storage_with(&["plant_a", "plant_b"]);
    assert_eq!(storage.get_active_project().unwrap(), None);

    storage.activate_project("plant_a").unwrap();
    let marker = storage.active_marker_path();
    assert!(std::fs::symlink_metadata(&marker).unwrap().is_file());
    assert_eq!(std::fs::read_to_string(&marker).unwrap(), "plant_a");
    assert_eq!(
        storage.get_active_project().unwrap().as_deref(),
        Some("plant_a")
    );

    storage.activate_project("plant_b").unwrap();
    assert_eq!(
        storage.get_active_project().unwrap().as_deref(),
        Some("plant_b")
    );
    // The marker is not a project, and no temporary file is left beside it.
    assert_eq!(storage.list_projects().unwrap(), vec!["plant_a", "plant_b"]);
    let leftovers: Vec<_> = std::fs::read_dir(storage.root_dir())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn a_damaged_marker_is_an_error_not_no_active_project() {
    let (_dir, storage) = storage_with(&["plant_a"]);
    std::fs::write(storage.active_marker_path(), "../elsewhere").unwrap();
    let err = storage.get_active_project().unwrap_err();
    assert!(
        err.to_string().contains("does not name a valid project"),
        "{err}"
    );

    std::fs::write(storage.active_marker_path(), "").unwrap();
    assert!(storage.get_active_project().is_err());
}

#[cfg(unix)]
#[test]
fn a_symlink_left_by_an_earlier_version_is_read_and_then_replaced() {
    let (_dir, storage) = storage_with(&["plant_a", "plant_b"]);
    let marker = storage.active_marker_path();
    std::os::unix::fs::symlink(storage.project_dir("plant_a").unwrap(), &marker).unwrap();
    assert_eq!(
        storage.get_active_project().unwrap().as_deref(),
        Some("plant_a")
    );

    storage.activate_project("plant_b").unwrap();
    assert!(std::fs::symlink_metadata(&marker).unwrap().is_file());
    assert_eq!(
        storage.get_active_project().unwrap().as_deref(),
        Some("plant_b")
    );
    assert!(
        storage.project_dir("plant_a").unwrap().is_dir(),
        "the old target is untouched"
    );
}
