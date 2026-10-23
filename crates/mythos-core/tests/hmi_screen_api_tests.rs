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

async fn request(app: &Router, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
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

#[tokio::test]
async fn screen_create_save_reopen_and_commit_are_project_scoped() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(dir.path()).unwrap());
    for id in ["plant_a", "plant_b"] {
        storage.create_project(&ProjectConfig::new(id, id)).unwrap();
    }
    let app = create_router(ApiState::<mythos_pal::MemoryShm>::new(
        storage.clone(),
        Arc::new(RwLock::new(TagCatalog::new())),
        None,
    ));
    let base = "/api/v1/projects/plant_a/hmi/screens";
    let (status, screen) = request(
        &app,
        "POST",
        base,
        json!({"name":"Overview", "is_root":true}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = screen["id"].as_str().unwrap();
    let canvas =
        json!({"title":"Edited overview", "components":[{"id":"draft_label", "type":"LABEL"}]});
    assert_eq!(
        request(&app, "PUT", &format!("{base}/{id}/draft"), canvas.clone())
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    let reopened = create_router(ApiState::<mythos_pal::MemoryShm>::new(
        storage.clone(),
        Arc::new(RwLock::new(TagCatalog::new())),
        None,
    ));
    let (status, screens) = request(&reopened, "GET", base, Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(screens[0]["canvasData"], canvas);
    assert_eq!(
        request(
            &app,
            "GET",
            "/api/v1/projects/plant_b/hmi/screens",
            Value::Null
        )
        .await
        .1,
        json!([])
    );
    assert_eq!(
        request(
            &app,
            "PUT",
            &format!("/api/v1/projects/plant_b/hmi/screens/{id}/draft"),
            canvas.clone()
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert!(!storage
        .project_dir("plant_a")
        .join("screens/active/screens.json")
        .exists());
    let (status, committed) = request(
        &app,
        "POST",
        "/api/v1/projects/plant_a/hmi/commit",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(committed[id], canvas);
    assert!(storage
        .project_dir("plant_a")
        .join("screens/active/screens.json")
        .exists());
    assert_eq!(
        request(
            &app,
            "POST",
            "/api/v1/projects/missing/hmi/screens",
            json!({"name":"Missing"})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(&app, "POST", base, json!({"name":"  "})).await.0,
        StatusCode::BAD_REQUEST
    );
}
