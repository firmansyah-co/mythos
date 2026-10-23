//! Integration tests for master faceplate types and release-time propagation (TSK-309 part B).
//!
//! The acceptance claim is "releasing a type propagates instantly to all instances across
//! screens". These tests hold that claim mechanically: they never edit an instance record
//! between a release and the assertion, so a passing read can only come from resolving the
//! master at read time.

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use futures_util::StreamExt;
use mythos_core::{
    api::{create_router, ApiState},
    storage::{ControllerStorage, ProjectConfig},
    tag::TagCatalog,
};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tower::ServiceExt;

const PROJECT: &str = "plant_a";

fn udt() -> Value {
    json!({
        "name": "FB_Motor_VFD",
        "kind": "STRUCT",
        "members": [
            {"name": "bRunning", "dataType": "BOOL"},
            {"name": "rSpeedFeedback", "dataType": "REAL"},
            {"name": "rCurrentAmps", "dataType": "REAL"}
        ]
    })
}

/// A master definition whose P&ID view presents the given faceplate keys.
fn type_body(id: &str, keys: &[&str]) -> Value {
    let bindings: Vec<Value> = keys
        .iter()
        .map(|key| {
            json!({
                "faceplateKey": key,
                "udtProperty": match *key {
                    "running" => "bRunning",
                    "actualSpeed" => "rSpeedFeedback",
                    _ => "rCurrentAmps",
                },
                "access": "READ_ONLY"
            })
        })
        .collect();
    json!({
        "id": id,
        "name": "Motor",
        "version": 0,
        "udt": udt(),
        "views": [{"kind": "PID_SYMBOL", "bindings": bindings}]
    })
}

fn build_app(storage: &Arc<ControllerStorage>) -> Router {
    create_router(ApiState::<mythos_pal::MemoryShm>::new(
        storage.clone(),
        Arc::new(RwLock::new(TagCatalog::new())),
        None,
    ))
}

fn seeded() -> (tempfile::TempDir, Arc<ControllerStorage>) {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(dir.path()).unwrap());
    storage
        .create_project(&ProjectConfig::new(PROJECT, PROJECT))
        .unwrap();
    (dir, storage)
}

async fn call(app: &Router, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn types_base() -> String {
    format!("/api/v1/projects/{PROJECT}/faceplates/types")
}

fn instances_base() -> String {
    format!("/api/v1/projects/{PROJECT}/faceplates/instances")
}

async fn save_draft(app: &Router, id: &str, body: Value) -> StatusCode {
    call(app, "PUT", &format!("{}/{id}/draft", types_base()), body)
        .await
        .0
}

async fn release(app: &Router, id: &str) -> (StatusCode, Value) {
    call(
        app,
        "POST",
        &format!("{}/{id}/release", types_base()),
        Value::Null,
    )
    .await
}

async fn place_instance(app: &Router, name: &str, type_id: &str) -> Value {
    let (status, created) = call(
        app,
        "POST",
        &instances_base(),
        json!({"name": name, "typeId": type_id, "bindings": []}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "instance create failed: {created}"
    );
    created
}

async fn resolved(app: &Router) -> Vec<Value> {
    let (status, body) = call(app, "GET", &instances_base(), Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    body["instances"].as_array().unwrap().clone()
}

/// The core acceptance test: two instances, a type released at v1, then a v2 that changes the
/// view. Both instances must resolve v2 with no instance-level edit in between.
#[tokio::test]
async fn release_propagates_to_every_instance_without_editing_any_of_them() {
    let (_dir, storage) = seeded();
    let app = build_app(&storage);

    assert_eq!(
        save_draft(&app, "fp_motor", type_body("fp_motor", &["running"])).await,
        StatusCode::NO_CONTENT
    );
    let (status, released) = release(&app, "fp_motor").await;
    assert_eq!(status, StatusCode::OK, "release failed: {released}");
    assert_eq!(released["version"], 1);

    let first = place_instance(&app, "P101", "fp_motor").await;
    let second = place_instance(&app, "P102", "fp_motor").await;
    let first_id = first["id"].as_str().unwrap().to_owned();
    let second_id = second["id"].as_str().unwrap().to_owned();

    // Both follow the master and initially see v1's single binding.
    for inst in resolved(&app).await {
        assert_eq!(inst["resolvedVersion"], 1);
        assert!(!inst["frozen"].as_bool().unwrap());
        let keys = view_keys(&inst);
        assert_eq!(keys, vec!["running".to_string()]);
    }

    // Release v2 with a wider view. No instance is touched.
    assert_eq!(
        save_draft(
            &app,
            "fp_motor",
            type_body("fp_motor", &["running", "actualSpeed", "currentAmps"])
        )
        .await,
        StatusCode::NO_CONTENT
    );
    let (status, released) = release(&app, "fp_motor").await;
    assert_eq!(status, StatusCode::OK, "release failed: {released}");
    assert_eq!(released["version"], 2);
    assert_eq!(
        released["instancesAffected"], 2,
        "both following instances must be reported as moved"
    );
    assert_eq!(released["strandedInstances"], json!([]));

    let after = resolved(&app).await;
    assert_eq!(after.len(), 2);
    for inst in after {
        let id = inst["id"].as_str().unwrap();
        assert!(
            id == first_id || id == second_id,
            "unexpected instance id {id}"
        );
        assert_eq!(
            inst["resolvedVersion"], 2,
            "instance {id} still resolves the old master"
        );
        assert_eq!(
            view_keys(&inst),
            vec![
                "running".to_string(),
                "actualSpeed".to_string(),
                "currentAmps".to_string()
            ],
            "instance {id} did not pick up the released view"
        );
        assert_eq!(inst["views"][0]["kind"], "PID_SYMBOL");
    }
}

fn view_keys(instance: &Value) -> Vec<String> {
    instance["views"][0]["bindings"]
        .as_array()
        .map(|b| {
            b.iter()
                .map(|binding| binding["faceplateKey"].as_str().unwrap().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// Negative behaviour, chosen explicitly: a frozen instance is NOT silently upgraded, and it
/// does NOT block the release. It stays on its own version and is reported as stranded.
#[tokio::test]
async fn frozen_instance_is_never_silently_upgraded_and_never_blocks_a_release() {
    let (_dir, storage) = seeded();
    let app = build_app(&storage);

    save_draft(&app, "fp_motor", type_body("fp_motor", &["running"])).await;
    release(&app, "fp_motor").await;

    let following = place_instance(&app, "P101", "fp_motor").await;
    let frozen = place_instance(&app, "P102", "fp_motor").await;
    // Freeze P102 at the published v1: an explicit commissioning decision.
    let (status, _) = call(
        &app,
        "PUT",
        &format!("{}/{}", instances_base(), frozen["id"].as_str().unwrap()),
        json!({"typeVersion": 1}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    save_draft(
        &app,
        "fp_motor",
        type_body("fp_motor", &["running", "actualSpeed"]),
    )
    .await;
    let (status, released) = release(&app, "fp_motor").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a frozen instance must never veto a release: {released}"
    );
    assert_eq!(released["version"], 2);
    assert_eq!(
        released["instancesAffected"], 1,
        "only the following instance moved"
    );
    assert_eq!(
        released["strandedInstances"],
        json!([{
            "id": frozen["id"],
            "name": "P102",
            "typeVersion": 1
        }])
    );

    for inst in resolved(&app).await {
        if inst["id"] == following["id"] {
            assert_eq!(inst["resolvedVersion"], 2, "following instance must move");
            assert_eq!(view_keys(&inst).len(), 2);
            assert!(inst["warnings"].as_array().unwrap().is_empty());
        } else {
            assert_eq!(
                inst["resolvedVersion"], 1,
                "frozen instance must stay on its frozen version"
            );
            assert_eq!(view_keys(&inst), vec!["running".to_string()]);
            assert!(inst["frozen"].as_bool().unwrap());
            let warning = inst["warnings"][0].as_str().unwrap();
            assert!(
                warning.contains("frozen at v1") && warning.contains("active is v2"),
                "frozen instance must explain its state: {warning}"
            );
        }
    }

    // Unfreezing is what moves it forward; no other edit is needed.
    let (status, _) = call(
        &app,
        "PUT",
        &format!("{}/{}", instances_base(), frozen["id"].as_str().unwrap()),
        json!({"typeVersion": null}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let moved = resolved(&app)
        .await
        .into_iter()
        .find(|i| i["id"] == frozen["id"])
        .unwrap();
    assert_eq!(moved["resolvedVersion"], 2);
    assert!(!moved["frozen"].as_bool().unwrap());
}

/// A release with no instances is a successful no-op, not an error.
#[tokio::test]
async fn release_of_a_type_with_no_instances_succeeds_and_is_a_no_op() {
    let (_dir, storage) = seeded();
    let app = build_app(&storage);

    save_draft(&app, "fp_motor", type_body("fp_motor", &["running"])).await;
    let (status, released) = release(&app, "fp_motor").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(released["version"], 1);
    assert_eq!(released["instancesAffected"], 0);
    assert_eq!(released["strandedInstances"], json!([]));

    // A second release still advances the version and still affects nothing.
    let (status, second) = release(&app, "fp_motor").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(second["version"], 2);
    assert_eq!(second["instancesAffected"], 0);
    assert!(resolved(&app).await.is_empty());
}

/// Draft and active are separate on disk, and a draft save is invisible to active readers.
#[tokio::test]
async fn draft_and_active_are_separate_until_release() {
    let (_dir, storage) = seeded();
    let app = build_app(&storage);

    save_draft(&app, "fp_motor", type_body("fp_motor", &["running"])).await;
    let project_dir = storage.project_dir(PROJECT);
    assert!(project_dir.join("faceplates/draft/types.json").exists());
    assert!(
        !project_dir.join("faceplates/active/types.json").exists(),
        "a draft save must not publish anything"
    );

    // Before any release the type is not readable as active.
    let (status, _) = call(
        &app,
        "GET",
        &format!("{}/fp_motor?view=active", types_base()),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    release(&app, "fp_motor").await;

    // An unreleased edit lands in draft only.
    save_draft(
        &app,
        "fp_motor",
        type_body("fp_motor", &["running", "actualSpeed"]),
    )
    .await;
    let (_, draft) = call(
        &app,
        "GET",
        &format!("{}/fp_motor?view=draft", types_base()),
        Value::Null,
    )
    .await;
    let (_, active) = call(
        &app,
        "GET",
        &format!("{}/fp_motor?view=active", types_base()),
        Value::Null,
    )
    .await;
    assert_eq!(view_keys_of_type(&draft).len(), 2, "draft carries the edit");
    assert_eq!(
        view_keys_of_type(&active).len(),
        1,
        "active readers must not see the unreleased draft"
    );

    // A fresh router over the same project directory reads the same separation.
    let reopened = build_app(&storage);
    let (_, active_again) = call(
        &reopened,
        "GET",
        &format!("{}/fp_motor?view=active", types_base()),
        Value::Null,
    )
    .await;
    assert_eq!(view_keys_of_type(&active_again).len(), 1);
}

fn view_keys_of_type(def: &Value) -> Vec<String> {
    def["views"][0]["bindings"]
        .as_array()
        .map(|b| {
            b.iter()
                .map(|binding| binding["faceplateKey"].as_str().unwrap().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// A view binding that names no member of the bound UDT is rejected, naming the offender.
#[tokio::test]
async fn binding_to_an_undeclared_udt_member_is_rejected_with_the_offender_named() {
    let (_dir, storage) = seeded();
    let app = build_app(&storage);

    let mut body = type_body("fp_motor", &["running"]);
    body["views"][0]["bindings"][0]["udtProperty"] = json!("bNotAMember");
    let (status, error) = call(
        &app,
        "PUT",
        &format!("{}/fp_motor/draft", types_base()),
        body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let message = error["error"].as_str().unwrap();
    assert!(
        message.contains("bNotAMember"),
        "error must name the offending property: {message}"
    );
    assert!(
        !storage
            .project_dir(PROJECT)
            .join("faceplates/draft/types.json")
            .exists(),
        "a rejected definition must not be persisted"
    );
}

/// An instance binding to a key its master does not declare, or to an invalid tag URI, is
/// rejected rather than stored and silently ignored.
#[tokio::test]
async fn instance_bindings_are_validated_against_the_master_and_the_tag_namespace() {
    let (_dir, storage) = seeded();
    let app = build_app(&storage);
    save_draft(&app, "fp_motor", type_body("fp_motor", &["running"])).await;
    release(&app, "fp_motor").await;

    let (status, error) = call(
        &app,
        "POST",
        &instances_base(),
        json!({
            "name": "P101",
            "typeId": "fp_motor",
            "bindings": [{"faceplateKey": "notDeclared", "tagUri": ""}]
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(error["error"].as_str().unwrap().contains("notDeclared"));

    let (status, error) = call(
        &app,
        "POST",
        &instances_base(),
        json!({
            "name": "P101",
            "typeId": "fp_motor",
            "bindings": [{"faceplateKey": "running", "tagUri": "P101.bRunning"}]
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(error["error"].as_str().unwrap().contains("tagUri"));

    // A declared key with a valid absolute URI is accepted.
    let (status, created) = call(
        &app,
        "POST",
        &instances_base(),
        json!({
            "name": "P101",
            "typeId": "fp_motor",
            "bindings": [{"faceplateKey": "running", "tagUri": "external.boiler.p101.b_running"}]
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "valid binding rejected: {created}"
    );
}

/// The release notification reaches an already-open telemetry stream, which is how an open
/// operator station refreshes without a reload.
#[tokio::test]
async fn release_emits_faceplate_propagated_on_the_telemetry_stream() {
    let (_dir, storage) = seeded();
    let app = build_app(&storage);
    save_draft(&app, "fp_motor", type_body("fp_motor", &["running"])).await;

    // The stream must be open (and therefore subscribed) before the release.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/telemetry/stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut events = response.into_body().into_data_stream();

    let (status, _) = release(&app, "fp_motor").await;
    assert_eq!(status, StatusCode::OK);

    let mut seen = String::new();
    let found = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(chunk) = events.next().await {
            seen.push_str(&String::from_utf8_lossy(&chunk.unwrap()));
            if seen.contains("FACEPLATE_PROPAGATED") {
                return true;
            }
        }
        false
    })
    .await
    .expect("timed out waiting for FACEPLATE_PROPAGATED on the telemetry stream");

    assert!(found, "stream ended without the release event; saw: {seen}");
    assert!(
        seen.contains("\"typeId\":\"fp_motor\"") && seen.contains("\"version\":1"),
        "event must identify the released type and version; saw: {seen}"
    );
}

/// Unknown projects and types are errors, never a plausible empty success.
#[tokio::test]
async fn missing_project_or_type_returns_an_error() {
    let (_dir, storage) = seeded();
    let app = build_app(&storage);

    let (status, _) = call(
        &app,
        "GET",
        "/api/v1/projects/no_such_project/faceplates/types",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = call(
        &app,
        "GET",
        &format!("{}/fp_missing", types_base()),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = call(
        &app,
        "POST",
        &format!("{}/fp_missing/release", types_base()),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, body) = call(&app, "GET", &types_base(), Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["types"], json!([]), "no fabricated types");
}

/// A type created through the API has no bindings until an author adds them, and the list
/// reports it as draft-only.
#[tokio::test]
async fn created_type_starts_as_draft_only_with_no_invented_bindings() {
    let (_dir, storage) = seeded();
    let app = build_app(&storage);

    let (status, created) = call(
        &app,
        "POST",
        &types_base(),
        json!({"name": "Motor", "udt": udt()}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create failed: {created}");
    let id = created["id"].as_str().unwrap();
    assert!(id.starts_with("fp_"));
    for view in created["views"].as_array().unwrap() {
        assert_eq!(view["bindings"], json!([]), "no fabricated bindings");
    }

    let (_, list) = call(&app, "GET", &types_base(), Value::Null).await;
    let summary = &list["types"][0];
    assert_eq!(summary["id"], id);
    assert_eq!(summary["activeVersion"], Value::Null);
    assert_eq!(summary["hasDraft"], true);
    assert_eq!(summary["instanceCount"], 0);
    assert_eq!(summary["udtName"], "FB_Motor_VFD");
}
