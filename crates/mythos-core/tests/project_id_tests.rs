//! A project id becomes a directory name, so it must never be able to name anything outside
//! the storage root.
//!
//! The id arrives from two places an unauthenticated client controls: the body of
//! `POST /api/v1/projects`, and the `:id` segment of every project route (where `%2F` decodes
//! to a slash). Before the check in `ControllerStorage::project_dir`, `../x` and absolute paths
//! created directories outside the root, and a project planted outside it could be read and
//! made the active project.

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    Router,
};
use mythos_core::{
    api::{create_router, ApiState},
    storage::{is_valid_project_id, ControllerStorage, ProjectConfig, StorageError},
    tag::TagCatalog,
};
use serde_json::json;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

/// Ids that must be refused. Each is a way to leave the root, or to collide with it.
fn hostile_ids(outside: &Path) -> Vec<String> {
    vec![
        "../escaped".into(),
        "..".into(),
        ".".into(),
        "a/b".into(),
        "a/../../escaped".into(),
        "..\\escaped".into(),
        outside.join("absolute").display().to_string(),
        "/".into(),
        "".into(),
        " ".into(),
        "name with space".into(),
        "nul\0byte".into(),
        ".hidden".into(),
        "active".into(),
        "x".repeat(129),
        "ünïcode".into(),
    ]
}

/// Every entry directly under `dir`, by name.
fn entries(dir: &Path) -> BTreeSet<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

struct Sandbox {
    /// Parent of the storage root. Anything that appears here other than `root` escaped.
    outer: tempfile::TempDir,
    storage: Arc<ControllerStorage>,
    router: Router,
}

impl Sandbox {
    fn new() -> Self {
        let outer = tempfile::tempdir().unwrap();
        let root = outer.path().join("root");
        std::fs::create_dir(&root).unwrap();
        let storage = Arc::new(ControllerStorage::new(&root).unwrap());
        let state = ApiState::<mythos_pal::MemoryShm>::new(
            Arc::clone(&storage),
            Arc::new(RwLock::new(TagCatalog::new())),
            None,
        );
        Self {
            outer,
            storage,
            router: create_router(state),
        }
    }

    fn root(&self) -> std::path::PathBuf {
        self.outer.path().join("root")
    }

    async fn send(&self, method: &str, uri: &str, body: Option<serde_json::Value>) -> StatusCode {
        let mut request = Request::builder().method(method).uri(uri);
        let body = match body {
            Some(json) => {
                request = request.header(header::CONTENT_TYPE, "application/json");
                Body::from(serde_json::to_vec(&json).unwrap())
            }
            None => Body::empty(),
        };
        self.router
            .clone()
            .oneshot(request.body(body).unwrap())
            .await
            .unwrap()
            .status()
    }
}

/// Percent-encode everything, so the id survives as one path segment whatever it contains.
fn encode(id: &str) -> String {
    id.bytes().map(|b| format!("%{b:02X}")).collect()
}

#[test]
fn accepts_the_ids_projects_actually_have() {
    for id in [
        "plant_a",
        "boiler-house",
        "proj_new_plant_station_01_6472",
        "A",
        "0",
        "_leading",
        &"x".repeat(128),
    ] {
        assert!(is_valid_project_id(id), "must accept {id:?}");
    }
}

#[test]
fn storage_refuses_every_hostile_id_and_touches_nothing() {
    let sandbox = Sandbox::new();
    let before_outer = entries(sandbox.outer.path());
    let before_root = entries(&sandbox.root());

    for id in hostile_ids(sandbox.outer.path()) {
        assert!(!is_valid_project_id(&id), "must reject {id:?}");

        let config = ProjectConfig::new(id.clone(), "x");
        assert!(
            matches!(
                sandbox.storage.create_project(&config),
                Err(StorageError::InvalidProjectId(_))
            ),
            "create_project must refuse {id:?}"
        );
        assert!(
            matches!(
                sandbox.storage.project_dir(&id),
                Err(StorageError::InvalidProjectId(_))
            ),
            "project_dir must refuse {id:?}"
        );
        assert!(sandbox.storage.get_project(&id).is_err(), "get {id:?}");
        assert!(
            sandbox.storage.activate_project(&id).is_err(),
            "activate {id:?}"
        );
        assert!(sandbox.storage.get_wal(&id).is_err(), "get_wal {id:?}");
        assert!(
            sandbox.storage.commit_draft_to_active(&id).is_err(),
            "commit {id:?}"
        );
    }

    assert_eq!(
        entries(sandbox.outer.path()),
        before_outer,
        "something was created outside the storage root"
    );
    assert_eq!(
        entries(&sandbox.root()),
        before_root,
        "something was created inside the storage root"
    );
}

#[tokio::test]
async fn the_api_refuses_to_create_a_project_outside_the_root() {
    let sandbox = Sandbox::new();
    let before_outer = entries(sandbox.outer.path());

    for id in hostile_ids(sandbox.outer.path()) {
        let status = sandbox
            .send(
                "POST",
                "/api/v1/projects",
                Some(json!({"id": id, "name": "x", "description": "", "author": ""})),
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "create with id {id:?}");
    }

    assert_eq!(entries(sandbox.outer.path()), before_outer);
    assert!(entries(&sandbox.root()).is_empty());

    // The route still works for an ordinary id.
    let status = sandbox
        .send(
            "POST",
            "/api/v1/projects",
            Some(json!({"id": "plant_a", "name": "Plant A", "description": "", "author": ""})),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        entries(&sandbox.root()),
        BTreeSet::from(["plant_a".to_string()])
    );
}

#[tokio::test]
async fn a_project_planted_outside_the_root_can_be_neither_read_nor_activated() {
    let sandbox = Sandbox::new();

    // A complete, readable project directory sitting beside the storage root. Before the fix
    // `GET /api/v1/projects/..%2Fplanted` answered 200 with this descriptor, and activating it
    // pointed the `active` symlink out of the root.
    let planted = sandbox.outer.path().join("planted");
    std::fs::create_dir(&planted).unwrap();
    std::fs::write(
        planted.join("project.active.json"),
        serde_json::to_vec(&ProjectConfig::new("planted", "Planted")).unwrap(),
    )
    .unwrap();
    assert!(
        sandbox
            .root()
            .join("../planted/project.active.json")
            .exists(),
        "the test is only meaningful if the traversal path really resolves to the planted project"
    );

    sandbox
        .storage
        .create_project(&ProjectConfig::new("plant_a", "Plant A"))
        .unwrap();
    sandbox.storage.activate_project("plant_a").unwrap();

    let traversal = encode("../planted");
    for (method, uri) in [
        ("GET", format!("/api/v1/projects/{traversal}")),
        ("POST", format!("/api/v1/projects/{traversal}/activate")),
        ("GET", format!("/api/v1/projects/{traversal}/hmi/screens")),
        ("GET", format!("/api/v1/projects/{traversal}/datatypes")),
        (
            "GET",
            format!("/api/v1/projects/{traversal}/faceplates/types"),
        ),
        (
            "GET",
            format!("/api/v1/projects/{traversal}/faceplates/instances"),
        ),
    ] {
        let status = sandbox.send(method, &uri, None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{method} {uri}");
    }

    assert_eq!(
        sandbox.storage.get_active_project().unwrap().as_deref(),
        Some("plant_a"),
        "the active project must not have moved"
    );
    let marker = std::fs::read_to_string(sandbox.storage.active_marker_path()).unwrap();
    assert_eq!(marker, "plant_a", "the active-project marker was rewritten");
}
