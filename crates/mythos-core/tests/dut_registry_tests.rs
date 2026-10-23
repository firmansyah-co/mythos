//! Project-scoped DUT/UDT registry: persistence, validation, and the absence of seed data.
//!
//! The defect these cover: `DataTypeEditor` shipped two hardcoded sample types
//! (`DUT_BottleSpec`, `DUT_MachineState`) as its default prop and persisted nothing, so an engineer
//! authoring a DUT got no storage and a false impression that the samples belonged to their
//! project. The first test below is the negative one: a project with zero data types must return
//! zero types and no sample names anywhere in the response.

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use mythos_core::{
    api::{create_router, ApiState},
    storage::{ControllerStorage, ProjectConfig},
    tag::TagCatalog,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

const PROJECT: &str = "plant_a";

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

fn base() -> String {
    format!("/api/v1/projects/{PROJECT}/datatypes")
}

fn bottle_struct() -> Value {
    json!({
        "name": "DUT_BottleSpec",
        "kind": "STRUCT",
        "members": [
            {"name": "Height_mm", "dataType": "REAL"},
            {"name": "Diameter_mm", "dataType": "REAL"},
            {"name": "Volume_ml", "dataType": "DINT"},
            {"name": "IsGlass", "dataType": "BOOL"}
        ]
    })
}

fn machine_state_enum() -> Value {
    json!({
        "name": "DUT_MachineState",
        "kind": "ENUM",
        "enumConstants": ["STOPPED", "STARTING", "IDLE", "RUNNING", "HOLDING", "HELD", "ABORTED"]
    })
}

/// The negative test the work package requires: an empty project returns no types and no sample
/// names. It would fail against the previous implementation, which had no endpoint at all and whose
/// editor rendered the two samples unconditionally.
#[tokio::test]
async fn a_project_with_no_data_types_returns_none_and_no_sample_names() {
    let (_dir, storage) = seeded();
    let app = build_app(&storage);

    let (status, body) = call(&app, "GET", &base(), Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["types"],
        json!([]),
        "an empty project must report an empty list, never seeded types"
    );

    let serialised = body.to_string();
    for sample in [
        "DUT_BottleSpec",
        "DUT_MachineState",
        "BottleSpec",
        "MachineState",
    ] {
        assert!(
            !serialised.contains(sample),
            "the registry must not invent the sample type '{sample}': {serialised}"
        );
    }

    // Nothing was written to disk either: no registry file is created by merely reading.
    assert!(!storage
        .project_dir(PROJECT)
        .join("datatypes/draft/types.json")
        .exists());
}

/// A defined STRUCT survives a reload with its members, through a fresh router over the same
/// project directory - i.e. it is actually persisted, not held in memory.
#[tokio::test]
async fn a_defined_struct_persists_with_its_members_across_a_reload() {
    let (_dir, storage) = seeded();
    let app = build_app(&storage);

    let (status, created) = call(&app, "POST", &base(), bottle_struct()).await;
    assert_eq!(status, StatusCode::CREATED, "create failed: {created}");
    let id = created["id"].as_str().unwrap().to_owned();

    // A completely new router (new state, same directory) must still see it.
    let reopened = build_app(&storage);
    let (status, fetched) = call(
        &reopened,
        "GET",
        &format!("{}/{id}?view=draft", base()),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "reload failed: {fetched}");
    assert_eq!(fetched["definition"]["name"], "DUT_BottleSpec");
    assert_eq!(fetched["definition"]["kind"], "STRUCT");
    let members = fetched["definition"]["members"].as_array().unwrap();
    assert_eq!(members.len(), 4);
    assert_eq!(members[0]["name"], "Height_mm");
    assert_eq!(members[0]["dataType"], "REAL");
    assert_eq!(members[3]["name"], "IsGlass");

    // And it shows up in the picker list with a real field count.
    let (_, list) = call(&reopened, "GET", &base(), Value::Null).await;
    let summary = &list["types"][0];
    assert_eq!(summary["name"], "DUT_BottleSpec");
    assert_eq!(summary["kind"], "STRUCT");
    assert_eq!(summary["fieldCount"], 4);
    assert_eq!(summary["activeVersion"], Value::Null, "not released yet");
}

/// An ENUM persists its constants and rejects struct members.
#[tokio::test]
async fn an_enum_persists_its_constants_and_rejects_members() {
    let (_dir, storage) = seeded();
    let app = build_app(&storage);

    let (status, created) = call(&app, "POST", &base(), machine_state_enum()).await;
    assert_eq!(status, StatusCode::CREATED, "enum create failed: {created}");
    let id = created["id"].as_str().unwrap().to_owned();

    let (_, fetched) = call(
        &app,
        "GET",
        &format!("{}/{id}?view=draft", base()),
        Value::Null,
    )
    .await;
    assert_eq!(
        fetched["definition"]["enumConstants"],
        json!(["STOPPED", "STARTING", "IDLE", "RUNNING", "HOLDING", "HELD", "ABORTED"])
    );

    let mut malformed = machine_state_enum();
    malformed["name"] = json!("DUT_Confused");
    malformed["members"] = json!([{"name": "x", "dataType": "INT"}]);
    let (status, error) = call(&app, "POST", &base(), malformed).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(error["error"]
        .as_str()
        .unwrap()
        .contains("must not declare struct members"));
}

/// Struct/enum structural rules are enforced, with the offending name in the message.
#[tokio::test]
async fn invalid_definitions_are_rejected_with_the_offender_named() {
    let (_dir, storage) = seeded();
    let app = build_app(&storage);

    // Empty STRUCT.
    let mut empty = bottle_struct();
    empty["members"] = json!([]);
    let (status, error) = call(&app, "POST", &base(), empty).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(error["error"].as_str().unwrap().contains("no members"));

    // Duplicate member.
    let mut duplicate = bottle_struct();
    duplicate["members"] = json!([
        {"name": "Height_mm", "dataType": "REAL"},
        {"name": "Height_mm", "dataType": "REAL"}
    ]);
    let (status, error) = call(&app, "POST", &base(), duplicate).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(error["error"].as_str().unwrap().contains("Height_mm"));

    // Duplicate name across two types.
    let (_, first) = call(&app, "POST", &base(), bottle_struct()).await;
    assert!(!first["id"].is_null());
    let (status, error) = call(&app, "POST", &base(), bottle_struct()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(error["error"]
        .as_str()
        .unwrap()
        .contains("already exists in this project"));

    // Nothing invalid was persisted.
    let (_, list) = call(&app, "GET", &base(), Value::Null).await;
    assert_eq!(
        list["types"].as_array().unwrap().len(),
        1,
        "only the one valid type should exist"
    );
}

/// Draft and active are separate: a draft edit is invisible to released readers until release.
#[tokio::test]
async fn draft_edits_are_invisible_until_release() {
    let (_dir, storage) = seeded();
    let app = build_app(&storage);

    let (_, created) = call(&app, "POST", &base(), bottle_struct()).await;
    let id = created["id"].as_str().unwrap().to_owned();

    // Never released: the released view does not exist.
    let (status, _) = call(&app, "GET", &format!("{}/{id}", base()), Value::Null).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, released) = call(
        &app,
        "POST",
        &format!("{}/{id}/release", base()),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(released["version"], 1);

    // Add a member in draft.
    let mut edited = bottle_struct();
    edited["members"] = json!([
        {"name": "Height_mm", "dataType": "REAL"},
        {"name": "Weight_g", "dataType": "REAL"}
    ]);
    let (status, saved) = call(&app, "PUT", &format!("{}/{id}/draft", base()), edited).await;
    assert_eq!(status, StatusCode::OK, "draft save failed: {saved}");

    let (_, draft_view) = call(
        &app,
        "GET",
        &format!("{}/{id}?view=draft", base()),
        Value::Null,
    )
    .await;
    let (_, active_view) = call(&app, "GET", &format!("{}/{id}", base()), Value::Null).await;
    assert_eq!(
        draft_view["definition"]["members"]
            .as_array()
            .unwrap()
            .len(),
        2,
        "draft carries the edit"
    );
    assert_eq!(
        active_view["definition"]["members"]
            .as_array()
            .unwrap()
            .len(),
        4,
        "released readers must not see the draft edit"
    );

    // Release promotes it.
    let (status, second) = call(
        &app,
        "POST",
        &format!("{}/{id}/release", base()),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(second["version"], 2);
    let (_, active_after) = call(&app, "GET", &format!("{}/{id}", base()), Value::Null).await;
    assert_eq!(
        active_after["definition"]["members"]
            .as_array()
            .unwrap()
            .len(),
        2
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
        "/api/v1/projects/no_such_project/datatypes",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = call(&app, "GET", &format!("{}/dut_missing", base()), Value::Null).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = call(
        &app,
        "PUT",
        &format!("{}/dut_missing/draft", base()),
        bottle_struct(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// Deleting removes the draft entry but keeps released versions resolvable.
#[tokio::test]
async fn deleting_a_draft_keeps_released_versions_resolvable() {
    let (_dir, storage) = seeded();
    let app = build_app(&storage);

    let (_, created) = call(&app, "POST", &base(), bottle_struct()).await;
    let id = created["id"].as_str().unwrap().to_owned();
    call(
        &app,
        "POST",
        &format!("{}/{id}/release", base()),
        Value::Null,
    )
    .await;

    let (status, _) = call(&app, "DELETE", &format!("{}/{id}", base()), Value::Null).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, list) = call(&app, "GET", &base(), Value::Null).await;
    assert_eq!(list["types"], json!([]), "draft entry is gone");
    assert_eq!(
        call(&app, "GET", &format!("{}/{id}", base()), Value::Null)
            .await
            .0,
        StatusCode::OK,
        "the released version is retained, so anything binding to it still resolves"
    );
}

// ---------------------------------------------------------------------------
// Review findings: name ambiguity across stages, and unresolvable type references
// ---------------------------------------------------------------------------

/// B: a released type that has no draft can be shadowed by a new draft with the same name.
///
/// The released version stays resolvable by id but disappears from the list, so the picker shows
/// one `DUT_Motor` while two exist — the exact ambiguity the duplicate-name rule is supposed to
/// prevent. Reachable entirely through the public API.
#[tokio::test]
async fn a_released_type_cannot_be_shadowed_by_a_new_draft_with_the_same_name() {
    let (_dir, storage) = seeded();
    let app = build_app(&storage);

    let (status, created) = call(&app, "POST", &base(), bottle_struct()).await;
    assert_eq!(status, StatusCode::CREATED, "create failed: {created}");
    let released_id = created["id"].as_str().unwrap().to_owned();
    call(
        &app,
        "POST",
        &format!("{}/{released_id}/release", base()),
        Value::Null,
    )
    .await;
    let (status, _) = call(
        &app,
        "DELETE",
        &format!("{}/{released_id}", base()),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // A second type with the same name must be refused: the released one still exists and is
    // still resolvable, so accepting this would create two types named DUT_BottleSpec.
    let (status, error) = call(&app, "POST", &base(), bottle_struct()).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a released type still owns its name even when its draft is gone: {error}"
    );
    assert!(
        error["error"]
            .as_str()
            .unwrap()
            .contains("already exists in this project"),
        "error must explain the clash: {error}"
    );

    // And the released version is still reachable by id, which is why the name is still taken.
    let (status, _) = call(
        &app,
        "GET",
        &format!("{}/{released_id}", base()),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

/// B (the other half): renaming a draft must not collide with a released-only type either.
#[tokio::test]
async fn saving_a_draft_cannot_take_a_name_owned_by_a_released_type() {
    let (_dir, storage) = seeded();
    let app = build_app(&storage);

    let (_, first) = call(&app, "POST", &base(), bottle_struct()).await;
    let first_id = first["id"].as_str().unwrap().to_owned();
    call(
        &app,
        "POST",
        &format!("{}/{first_id}/release", base()),
        Value::Null,
    )
    .await;
    call(
        &app,
        "DELETE",
        &format!("{}/{first_id}", base()),
        Value::Null,
    )
    .await;

    // A second, differently-named type, then rename it onto the released name.
    let mut other = bottle_struct();
    other["name"] = json!("DUT_Other");
    let (_, second) = call(&app, "POST", &base(), other).await;
    let second_id = second["id"].as_str().unwrap().to_owned();

    let (status, error) = call(
        &app,
        "PUT",
        &format!("{}/{second_id}/draft", base()),
        bottle_struct(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "renaming onto a released type's name must be refused: {error}"
    );
    assert!(error["error"]
        .as_str()
        .unwrap()
        .contains("already exists in this project"));
}

/// A: a STRUCT member naming a type that does not exist must be rejected.
#[tokio::test]
async fn struct_members_must_reference_a_known_type() {
    let (_dir, storage) = seeded();
    let app = build_app(&storage);

    let mut bad = bottle_struct();
    bad["members"] = json!([{"name": "Height", "dataType": "NOT_A_REAL_TYPE"}]);
    let (status, error) = call(&app, "POST", &base(), bad).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "unknown type accepted: {error}"
    );
    assert!(
        error["error"].as_str().unwrap().contains("NOT_A_REAL_TYPE"),
        "error must name the offender: {error}"
    );

    // A reference to a project type that does not exist is equally unresolvable.
    let mut dangling = bottle_struct();
    dangling["members"] = json!([{"name": "Spec", "dataType": "DUT_DoesNotExist"}]);
    let (status, error) = call(&app, "POST", &base(), dangling).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(error["error"]
        .as_str()
        .unwrap()
        .contains("DUT_DoesNotExist"));

    // A project type that DOES exist is a legal member type.
    let mut motor = bottle_struct();
    motor["name"] = json!("DUT_Motor");
    motor["members"] = json!([{"name": "Speed", "dataType": "REAL"}]);
    let (status, created) = call(&app, "POST", &base(), motor).await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "primitive member rejected: {created}"
    );

    let mut machine = bottle_struct();
    machine["name"] = json!("DUT_Machine");
    machine["members"] = json!([{"name": "Motor", "dataType": "DUT_Motor"}]);
    let (status, created) = call(&app, "POST", &base(), machine).await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "a reference to an existing project type must be legal: {created}"
    );
}

/// A: an ALIAS base type must also resolve.
#[tokio::test]
async fn alias_base_type_must_resolve() {
    let (_dir, storage) = seeded();
    let app = build_app(&storage);

    let (status, error) = call(
        &app,
        "POST",
        &base(),
        json!({"name": "DUT_Speed", "kind": "ALIAS", "baseType": "ALSO_NOT_A_TYPE"}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "unknown base type accepted: {error}"
    );
    assert!(error["error"].as_str().unwrap().contains("ALSO_NOT_A_TYPE"));

    // A primitive base type is legal.
    let (status, created) = call(
        &app,
        "POST",
        &base(),
        json!({"name": "DUT_Speed", "kind": "ALIAS", "baseType": "REAL"}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "primitive base rejected: {created}"
    );
}
