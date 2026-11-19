mod common;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use mythos_core::{
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
    let app = common::hmi_router(storage.clone(), Arc::new(RwLock::new(TagCatalog::new()))).await;
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
    let reopened =
        common::hmi_router(storage.clone(), Arc::new(RwLock::new(TagCatalog::new()))).await;
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
        .unwrap()
        .join("hmi/screens/active/screens.json")
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
        .unwrap()
        .join("hmi/screens/active/screens.json")
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

/// A project with one router, for the tests below.
async fn app_with(projects: &[&str]) -> (Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(dir.path()).unwrap());
    for id in projects {
        storage
            .create_project(&ProjectConfig::new(*id, *id))
            .unwrap();
    }
    let app = common::hmi_router(storage, Arc::new(RwLock::new(TagCatalog::new()))).await;
    (app, dir)
}

#[tokio::test]
async fn the_released_view_is_what_was_released_not_what_is_being_edited() {
    let (app, _dir) = app_with(&["plant_a"]).await;
    let base = "/api/v1/projects/plant_a/hmi/screens";
    let active = format!("{base}?view=active");

    let (_, screen) = request(&app, "POST", base, json!({"name": "Overview"})).await;
    let id = screen["id"].as_str().unwrap();
    let first = json!({"components": [{"id": "a", "type": "text", "text": "first"}]});
    request(&app, "PUT", &format!("{base}/{id}/draft"), first.clone()).await;

    // Nothing has been released: the released view is empty, not a copy of the draft.
    assert_eq!(
        request(&app, "GET", &active, Value::Null).await.1,
        json!([])
    );

    request(
        &app,
        "POST",
        "/api/v1/projects/plant_a/hmi/commit",
        Value::Null,
    )
    .await;
    let (_, released) = request(&app, "GET", &active, Value::Null).await;
    assert_eq!(released[0]["canvasData"], first);

    // Editing on does not change what was released.
    let second = json!({"components": [{"id": "a", "type": "text", "text": "second"}]});
    request(&app, "PUT", &format!("{base}/{id}/draft"), second.clone()).await;
    let (_, released) = request(&app, "GET", &active, Value::Null).await;
    assert_eq!(released[0]["canvasData"], first);
    let (_, draft) = request(&app, "GET", &format!("{base}?view=draft"), Value::Null).await;
    assert_eq!(draft[0]["canvasData"], second);
    // No view named means the draft.
    assert_eq!(request(&app, "GET", base, Value::Null).await.1, draft);

    let (status, _) = request(&app, "GET", &format!("{base}?view=live"), Value::Null).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn deleting_a_screen_removes_it_from_the_draft_and_from_the_next_release() {
    let (app, _dir) = app_with(&["plant_a", "plant_b"]).await;
    let base = "/api/v1/projects/plant_a/hmi/screens";
    let active = format!("{base}?view=active");

    let (_, keep) = request(&app, "POST", base, json!({"name": "Keep"})).await;
    let (_, gone) = request(&app, "POST", base, json!({"name": "Gone"})).await;
    let gone_id = gone["id"].as_str().unwrap();
    request(
        &app,
        "POST",
        "/api/v1/projects/plant_a/hmi/commit",
        Value::Null,
    )
    .await;

    let (status, _) = request(&app, "DELETE", &format!("{base}/{gone_id}"), Value::Null).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let names = |screens: &Value| -> Vec<String> {
        screens
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["name"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(
        names(&request(&app, "GET", base, Value::Null).await.1),
        ["Keep"]
    );
    // Still released until the draft is released again.
    assert_eq!(
        names(&request(&app, "GET", &active, Value::Null).await.1),
        ["Keep", "Gone"]
    );
    request(
        &app,
        "POST",
        "/api/v1/projects/plant_a/hmi/commit",
        Value::Null,
    )
    .await;
    assert_eq!(
        names(&request(&app, "GET", &active, Value::Null).await.1),
        ["Keep"]
    );

    // A screen that is not there, in this project or reached through another one.
    let (status, _) = request(&app, "DELETE", &format!("{base}/{gone_id}"), Value::Null).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let keep_id = keep["id"].as_str().unwrap();
    let (status, _) = request(
        &app,
        "DELETE",
        &format!("/api/v1/projects/plant_b/hmi/screens/{keep_id}"),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        names(&request(&app, "GET", base, Value::Null).await.1),
        ["Keep"]
    );
}

#[tokio::test]
async fn releasing_screens_is_announced_on_the_telemetry_stream() {
    use futures_util::StreamExt;
    use std::time::Duration;

    let (app, _dir) = app_with(&["plant_a"]).await;
    let base = "/api/v1/projects/plant_a/hmi/screens";
    request(&app, "POST", base, json!({"name": "Overview"})).await;

    // An operator station: the stream is open before the release happens.
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
    let mut body = response.into_body().into_data_stream();

    // Nothing is announced while nothing has been released.
    let mut seen = String::new();
    let quiet = tokio::time::timeout(Duration::from_millis(400), async {
        while let Some(Ok(chunk)) = body.next().await {
            seen.push_str(&String::from_utf8_lossy(&chunk));
        }
    })
    .await;
    assert!(quiet.is_err(), "the stream stays open");
    assert!(
        !seen.contains("SCREENS_RELEASED"),
        "nothing was released yet: {seen}"
    );

    request(
        &app,
        "POST",
        "/api/v1/projects/plant_a/hmi/commit",
        Value::Null,
    )
    .await;

    let announced = tokio::time::timeout(Duration::from_secs(5), async {
        let mut buffer = seen;
        loop {
            let chunk = body.next().await.expect("stream ended").unwrap();
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            if let Some(start) = buffer.find("event: SCREENS_RELEASED") {
                if let Some(end) = buffer[start..].find("\n\n") {
                    let event = &buffer[start..start + end];
                    let data = event
                        .lines()
                        .find_map(|l| l.strip_prefix("data: "))
                        .unwrap();
                    return serde_json::from_str::<Value>(data).unwrap();
                }
            }
        }
    })
    .await
    .expect("the release was not announced on the telemetry stream");

    assert_eq!(announced, json!({"project_id": "plant_a", "screens": 1}));
}

/// A project whose screens were written by the core, before the HMI was a module, keeps them:
/// the module takes them over the first time it serves the project, and leaves the originals.
#[tokio::test]
async fn screens_saved_before_the_hmi_was_a_module_are_still_there() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(dir.path()).unwrap());
    storage
        .create_project(&ProjectConfig::new("plant_a", "plant_a"))
        .unwrap();
    let project = storage.project_dir("plant_a").unwrap();
    let screen = json!([{
        "id": "screen_old", "name": "Boiler", "is_root": true, "subscreens": [],
        "canvasData": {"title": "Boiler", "components": [{"id": "t", "type": "text", "x": 0, "y": 0, "width": 80, "height": 24, "text": "Steam"}]}
    }]);
    for stage in ["draft", "active"] {
        std::fs::create_dir_all(project.join("screens").join(stage)).unwrap();
        std::fs::write(
            project.join("screens").join(stage).join("screens.json"),
            screen.to_string(),
        )
        .unwrap();
    }
    std::fs::create_dir_all(project.join("faceplates/draft")).unwrap();
    std::fs::write(project.join("faceplates/draft/types.json"), "[]").unwrap();

    let app = common::hmi_router(storage.clone(), Arc::new(RwLock::new(TagCatalog::new()))).await;
    for view in ["draft", "active"] {
        let (status, body) = request(
            &app,
            "GET",
            &format!("/api/v1/projects/plant_a/hmi/screens?view={view}"),
            Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body, screen, "the {view} screens are the ones saved before");
    }
    assert!(project.join("hmi/faceplates/draft/types.json").is_file());
    assert!(
        project.join("screens/draft/screens.json").is_file(),
        "the originals stay, for a server rolled back to before the module"
    );
}
