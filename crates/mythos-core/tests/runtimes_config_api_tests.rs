//! The precondition on a save of a project's runtimes configuration, over the API.
//!
//! Run against the real router and a real I/O manager on a real shared-memory segment, in a
//! temporary directory, with no port. Saves of a configuration with no instance need no runtime
//! module. The properties that matter:
//!
//! - the read answers the revision of the stored bytes, and a byte changed by hand changes it;
//! - a save with the revision it read is saved and answers the revision of what it wrote;
//! - a save with a revision the configuration no longer has is refused with words, and nothing
//!   is written;
//! - a misspelt query key is refused, never taken as a save without a precondition;
//! - a save without a revision is saved as before.

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    Router,
};
use mythos_core::{
    api::{create_router, ApiState},
    io::{DriverRegistry, IoManager},
    storage::{ControllerStorage, ProjectConfig},
    tag::TagCatalog,
};
use mythos_pal::DefaultShm;
use mythos_shm::ShmReaderHandle;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

const CAPACITY: usize = 64;
static SEGMENT_SEQ: AtomicU32 = AtomicU32::new(0);

/// The revision of no file: SHA-256 of no bytes.
const NO_FILE: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
/// The revision of the file a save of no instance writes, `{\n  "instances": []\n}`.
const NO_INSTANCE: &str = "74dd955ba792a2a80447cfe83aa25e231b7188d0314bb0a22c1610f18bf7e39a";

struct Rig {
    router: Router,
    io: Arc<IoManager<DefaultShm>>,
    segment: String,
    dir: tempfile::TempDir,
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = DefaultShm::unlink(&self.segment);
    }
}

/// A server with two projects, `plant_a` active, no runtime module and no configuration.
async fn rig() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(dir.path()).unwrap());
    for (id, name) in [("plant_a", "Plant A"), ("plant_b", "Plant B")] {
        storage
            .create_project(&ProjectConfig::new(id, name))
            .unwrap();
    }
    storage.activate_project("plant_a").unwrap();

    let segment = format!(
        "mythos_runtimes_config_test_{}_{}",
        std::process::id(),
        SEGMENT_SEQ.fetch_add(1, Ordering::SeqCst)
    );
    let _ = DefaultShm::unlink(&segment);
    let catalog = Arc::new(RwLock::new(TagCatalog::new()));
    let io = IoManager::<DefaultShm>::new(
        Arc::clone(&storage),
        Arc::clone(&catalog),
        segment.clone(),
        CAPACITY,
        DriverRegistry::empty(),
    );
    io.reload().await;
    let reader = ShmReaderHandle::<DefaultShm>::new(&segment, CAPACITY).unwrap();
    let state = ApiState::<DefaultShm>::new(storage, catalog, Some(reader))
        .with_io_manager(Arc::clone(&io));
    Rig {
        router: create_router(state),
        io,
        segment,
        dir,
    }
}

async fn call(
    router: Router,
    method: &'static str,
    uri: String,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(json) => {
            request = request.header(header::CONTENT_TYPE, "application/json");
            Body::from(serde_json::to_vec(&json).unwrap())
        }
        None => Body::empty(),
    };
    let response = router.oneshot(request.body(body).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

impl Rig {
    async fn request(
        &self,
        method: &'static str,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        call(self.router.clone(), method, uri.to_string(), body).await
    }

    /// The revision the read answers, after checking the read answered.
    async fn revision(&self, project: &str) -> String {
        let (status, body) = self
            .request("GET", &format!("/api/v1/projects/{project}/runtimes"), None)
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["revision"]
            .as_str()
            .unwrap_or_else(|| panic!("no revision in {body}"))
            .to_string()
    }

    async fn save(&self, project: &str, query: &str, config: Value) -> (StatusCode, Value) {
        self.request(
            "PUT",
            &format!("/api/v1/projects/{project}/runtimes{query}"),
            Some(config),
        )
        .await
    }

    fn file(&self, project: &str) -> PathBuf {
        self.dir.path().join(project).join("runtimes.json")
    }

    /// The stored bytes, `None` when there is no file.
    fn stored(&self, project: &str) -> Option<Vec<u8>> {
        std::fs::read(self.file(project)).ok()
    }
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn none() -> Value {
    json!({"instances": []})
}

/// An instance whose module no runtime module provides: the module checks refuse it.
fn unknown_module() -> Value {
    json!({"instances": [{
        "id": "tank", "module": "plc-9000", "run": false,
        "program": {"name": "P", "version": "1", "interval_ms": 20, "code": "00"}
    }]})
}

fn changed_words(expected: &str, now: &str) -> String {
    format!(
        "the project's runtimes have changed since they were read (revision {expected}, now {now}), so nothing was saved: read them again"
    )
}

#[tokio::test]
async fn the_read_answers_the_revision_of_the_stored_bytes_and_of_no_bytes_without_a_file() {
    let rig = rig().await;
    let (status, body) = rig
        .request("GET", "/api/v1/projects/plant_b/runtimes", None)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!({"config": null, "revision": NO_FILE}));

    let text = br#"{"instances": []}"#;
    std::fs::write(rig.file("plant_b"), text).unwrap();
    let (_, body) = rig
        .request("GET", "/api/v1/projects/plant_b/runtimes", None)
        .await;
    assert_eq!(body["config"], none(), "{body}");
    assert_eq!(
        body["revision"], "b21a191107a138ca112d977949f546a9db58f4a0a9d883b3cb9a5ad3a2cbdc7a",
        "the SHA-256 of the bytes on disk, as they are"
    );
    rig.io.shutdown().await;
}

#[tokio::test]
async fn a_save_with_the_revision_it_read_is_saved_and_answers_the_revision_of_what_it_wrote() {
    let rig = rig().await;
    let read = rig.revision("plant_a").await;
    assert_eq!(read, NO_FILE);
    let (status, body) = rig
        .save("plant_a", &format!("?revision={read}"), none())
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["applied"], true, "{body}");
    let written = rig.stored("plant_a").expect("the file is written");
    assert_eq!(body["revision"], sha256(&written), "{body}");
    assert_eq!(body["revision"], NO_INSTANCE, "{body}");
    assert_eq!(rig.revision("plant_a").await, NO_INSTANCE);
    rig.io.shutdown().await;
}

#[tokio::test]
async fn a_save_with_a_revision_the_configuration_no_longer_has_is_refused_and_nothing_is_written()
{
    let rig = rig().await;
    // Two clients read the same configuration: none.
    let first = rig.revision("plant_a").await;
    let second = rig.revision("plant_a").await;
    assert_eq!(first, second);
    // One saves (its bytes are not those this server would write, so a write would show).
    let (status, body) = rig
        .save("plant_a", &format!("?revision={first}"), none())
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let compact = br#"{"instances":[]}"#;
    std::fs::write(rig.file("plant_a"), compact).unwrap();
    let now = sha256(compact);
    // The other sends what it planned from its read.
    let (status, body) = rig
        .save("plant_a", &format!("?revision={second}"), none())
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body, json!({"error": changed_words(&second, &now)}));
    let on_disk = compact.to_vec();
    assert_eq!(
        rig.stored("plant_a").unwrap(),
        on_disk,
        "nothing was written"
    );
    rig.io.shutdown().await;
}

#[tokio::test]
async fn a_configuration_changed_by_hand_changes_its_revision_and_a_save_of_the_old_one_is_refused()
{
    let rig = rig().await;
    let (status, body) = rig.save("plant_b", "", none()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let before = rig.revision("plant_b").await;
    // One space more: the same configuration, other bytes.
    let mut bytes = rig.stored("plant_b").unwrap();
    bytes.push(b' ');
    std::fs::write(rig.file("plant_b"), &bytes).unwrap();
    let after = rig.revision("plant_b").await;
    assert_ne!(after, before, "a byte changed by hand changes the revision");
    assert_eq!(after, sha256(&bytes));

    let (status, body) = rig
        .save("plant_b", &format!("?revision={before}"), none())
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"], changed_words(&before, &after));
    assert_eq!(rig.stored("plant_b").unwrap(), bytes, "nothing was written");
    rig.io.shutdown().await;
}

/// What the read's description says of an empty file, as it is: no bytes, so the revision of no
/// file. The read refuses it, and a save with the revision of no file replaces it.
#[tokio::test]
async fn an_empty_file_has_the_revision_of_no_file_and_a_save_with_that_revision_replaces_it() {
    let rig = rig().await;
    assert_eq!(rig.revision("plant_b").await, NO_FILE);
    std::fs::write(rig.file("plant_b"), b"").unwrap();
    let (status, body) = rig
        .request("GET", "/api/v1/projects/plant_b/runtimes", None)
        .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|e| e.contains("is not a runtimes configuration")),
        "{body}"
    );

    let (status, body) = rig
        .save("plant_b", &format!("?revision={NO_FILE}"), none())
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["revision"], NO_INSTANCE, "{body}");
    assert_eq!(rig.revision("plant_b").await, NO_INSTANCE);
    rig.io.shutdown().await;
}

#[tokio::test]
async fn a_stale_save_is_refused_before_the_modules_check_its_programs() {
    let rig = rig().await;
    let (status, body) = rig.save("plant_b", "", none()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // The same configuration, read before that save: no file then.
    let (status, body) = rig
        .save("plant_b", &format!("?revision={NO_FILE}"), unknown_module())
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body, json!({"error": changed_words(NO_FILE, NO_INSTANCE)}));
    assert!(
        body.get("problems").is_none(),
        "the modules were not asked: {body}"
    );
    // The control: with the revision stored, the same configuration reaches the checks.
    let (status, body) = rig
        .save(
            "plant_b",
            &format!("?revision={NO_INSTANCE}"),
            unknown_module(),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        body["problems"][0]["message"], "no runtime module provides 'plc-9000' (available: none)",
        "{body}"
    );
    rig.io.shutdown().await;
}

#[tokio::test]
async fn a_save_without_a_revision_is_saved_whatever_is_stored() {
    let rig = rig().await;
    std::fs::write(rig.file("plant_b"), br#"{"instances": []}"#).unwrap();
    let (status, body) = rig.save("plant_b", "", none()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["revision"], NO_INSTANCE, "{body}");
    assert_eq!(rig.stored("plant_b").unwrap(), b"{\n  \"instances\": []\n}");
    rig.io.shutdown().await;
}

#[tokio::test]
async fn start_and_revision_combine_and_a_key_the_save_does_not_take_is_refused() {
    let rig = rig().await;
    // A misspelt revision is not a save without one.
    let (status, body) = rig.save("plant_a", "?revison=0000", none()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        body,
        json!({"error": "'revison' is not something this save takes, so nothing was saved: it takes start and revision"})
    );
    assert_eq!(rig.stored("plant_a"), None, "nothing was written");
    // A key given twice is not one of them.
    let (status, body) = rig
        .save(
            "plant_a",
            &format!("?revision={NO_FILE}&revision=0000"),
            none(),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        body,
        json!({"error": "'revision' is given more than once, so nothing was saved: give it once"})
    );
    assert_eq!(rig.stored("plant_a"), None, "nothing was written");

    let (status, body) = rig
        .save(
            "plant_a",
            &format!("?start=cold&revision={NO_FILE}"),
            none(),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["revision"], NO_INSTANCE, "{body}");
    let (status, body) = rig
        .save(
            "plant_a",
            &format!("?revision={NO_FILE}&start=cold"),
            none(),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"], changed_words(NO_FILE, NO_INSTANCE));
    // `start` keeps its own refusal.
    let (status, body) = rig.save("plant_a", "?start=hot", none()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|e| e.starts_with("start=hot is not a way to start")),
        "{body}"
    );
    rig.io.shutdown().await;
}

/// No mutation can prove the lock (removing it leaves the race to the scheduler); this shows
/// the outcome it is there for, with both saves in flight together on two threads.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_saves_of_one_revision_at_the_same_time_one_is_saved_the_other_is_refused() {
    let rig = rig().await;
    let first = tokio::spawn(call(
        rig.router.clone(),
        "PUT",
        format!("/api/v1/projects/plant_a/runtimes?revision={NO_FILE}"),
        Some(none()),
    ));
    let second = tokio::spawn(call(
        rig.router.clone(),
        "PUT",
        format!("/api/v1/projects/plant_a/runtimes?revision={NO_FILE}"),
        Some(none()),
    ));
    let (first, second) = tokio::join!(first, second);
    let mut answers = [first.unwrap(), second.unwrap()];
    answers.sort_by_key(|(status, _)| status.as_u16());
    let [(saved, saved_body), (refused, refused_body)] = answers;
    assert_eq!(saved, StatusCode::OK, "{saved_body}");
    assert_eq!(refused, StatusCode::CONFLICT, "{refused_body}");
    assert_eq!(
        refused_body["error"],
        changed_words(NO_FILE, saved_body["revision"].as_str().unwrap())
    );
    assert_eq!(
        sha256(&rig.stored("plant_a").unwrap()),
        saved_body["revision"].as_str().unwrap(),
        "the stored file is the one the saved answer says it wrote"
    );
    rig.io.shutdown().await;
}
