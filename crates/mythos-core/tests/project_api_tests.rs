//! Project routes that a client depends on to tell which project the controller is running.

use axum::{
    body::Body,
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

async fn get_json(router: &Router, path: &str) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn the_active_project_is_null_until_one_is_activated_and_then_names_it() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(dir.path()).unwrap());
    storage
        .create_project(&ProjectConfig::new("plant_a", "Plant A"))
        .unwrap();
    storage
        .create_project(&ProjectConfig::new("plant_b", "Plant B"))
        .unwrap();

    let state = ApiState::<mythos_pal::MemoryShm>::new(
        Arc::clone(&storage),
        Arc::new(RwLock::new(TagCatalog::new())),
        None,
    );
    let router = create_router(state);

    // Two projects exist and neither runs: the answer is null, not the first one found.
    let (status, body) = get_json(&router, "/api/v1/projects/active").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "project_id": null }));

    storage.activate_project("plant_b").unwrap();
    let (_, body) = get_json(&router, "/api/v1/projects/active").await;
    assert_eq!(body, json!({ "project_id": "plant_b" }));

    // The literal segment is not swallowed by the `:id` route, and a real id still resolves.
    let (status, body) = get_json(&router, "/api/v1/projects/plant_a").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "Plant A");
}
