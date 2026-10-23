use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use mythos_core::api::{create_router, ApiState, DEFAULT_INDEX_HTML};
use mythos_core::storage::ControllerStorage;
use mythos_core::tag::{TagCatalog, TagMetadata};
use std::fs;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::sync::RwLock;
use tower::ServiceExt; // for oneshot()

fn setup_test_state() -> (ApiState<mythos_pal::MemoryShm>, tempfile::TempDir) {
    let tmp_dir = tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(tmp_dir.path()).unwrap());
    let mut catalog = TagCatalog::new();
    let meta = TagMetadata::new("external.BoilerRoom.PT101.pressure.pv", 0).unwrap();
    catalog.register(meta).unwrap();
    let tag_catalog = Arc::new(RwLock::new(catalog));

    let state = ApiState::<mythos_pal::MemoryShm>::new(storage, tag_catalog, None);
    (state, tmp_dir)
}

#[tokio::test]
async fn test_web_hmi_embedded_root_serves_html() {
    let (state, _tmp) = setup_test_state();
    let router = create_router(state);

    let req = Request::builder().uri("/").body(Body::empty()).unwrap();

    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let content_type = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap();
    assert!(content_type.starts_with("text/html"));

    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8_lossy(&body_bytes);

    assert!(body_str.contains("MythOS Web HMI"));
    assert!(body_str.contains("ANSI/ISA-101.01"));
    assert!(body_str.contains("TagBus Real-Time Status Monitor"));
    assert_eq!(body_str, DEFAULT_INDEX_HTML);
}

#[tokio::test]
async fn test_api_health_endpoint_still_serves_json() {
    let (state, _tmp) = setup_test_state();
    let router = create_router(state);

    let req = Request::builder()
        .uri("/api/v1/health")
        .body(Body::empty())
        .unwrap();

    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let content_type = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap();
    assert_eq!(content_type, "application/json");

    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(body_json["status"], "HEALTHY");
    assert_eq!(body_json["service"], "mythos-microkernel");
}

#[tokio::test]
async fn test_api_unknown_route_returns_404_json_not_html() {
    let (state, _tmp) = setup_test_state();
    let router = create_router(state);

    let req = Request::builder()
        .uri("/api/v1/unknown_missing_route")
        .body(Body::empty())
        .unwrap();

    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let content_type = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap();
    assert_eq!(content_type, "application/json");

    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body_json.get("error").is_some());
}

#[tokio::test]
async fn test_spa_fallback_arbitrary_client_routes() {
    let (state, _tmp) = setup_test_state();
    let router = create_router(state);

    let client_routes = [
        "/topology",
        "/screens",
        "/screens/active/main",
        "/projects/alpha",
        "/settings/general",
    ];

    for route in client_routes {
        let req = Request::builder().uri(route).body(Body::empty()).unwrap();

        let resp = router.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "Route {} failed", route);

        let content_type = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap();
        assert!(content_type.starts_with("text/html"));

        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let body_str = String::from_utf8_lossy(&body_bytes);
        assert!(body_str.contains("MythOS Web HMI"));
    }
}

#[tokio::test]
async fn test_embedded_asset_serving() {
    let (state, _tmp) = setup_test_state();
    let router = create_router(state);

    let req = Request::builder()
        .uri("/assets/mythos-banner.svg")
        .body(Body::empty())
        .unwrap();

    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let content_type = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap();
    assert_eq!(content_type, "image/svg+xml");

    let req2 = Request::builder()
        .uri("/assets/mythos-architecture.svg")
        .body(Body::empty())
        .unwrap();

    let resp2 = router.oneshot(req2).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);
    let content_type2 = resp2
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap();
    assert_eq!(content_type2, "image/svg+xml");
}

#[tokio::test]
async fn test_missing_asset_returns_404_not_index_html() {
    let (state, _tmp) = setup_test_state();
    let router = create_router(state);

    let req = Request::builder()
        .uri("/assets/nonexistent-bundle.js")
        .body(Body::empty())
        .unwrap();

    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_runtime_static_dir_override() {
    let (mut state, _tmp) = setup_test_state();
    let static_dir = tempdir().unwrap();

    // Create custom index.html and assets in custom static dir
    let custom_html = "<!DOCTYPE html><html><body><h1>Custom React 19 HMI</h1></body></html>";
    fs::write(static_dir.path().join("index.html"), custom_html).unwrap();

    let assets_dir = static_dir.path().join("assets");
    fs::create_dir(&assets_dir).unwrap();
    let custom_js = "console.log('MythOS Custom JS Bundle');";
    fs::write(assets_dir.join("main.js"), custom_js).unwrap();

    let custom_css = "body { background: #000; }";
    fs::write(static_dir.path().join("style.css"), custom_css).unwrap();

    // Configure static_dir on state
    state = state.with_static_dir(static_dir.path());
    let router = create_router(state);

    // 1. Root / serves custom index.html
    let req = Request::builder().uri("/").body(Body::empty()).unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&body), custom_html);

    // 2. /assets/main.js serves custom javascript
    let req = Request::builder()
        .uri("/assets/main.js")
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let content_type = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap();
    assert!(content_type.starts_with("text/javascript"));
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&body), custom_js);

    // 3. /style.css serves custom css
    let req = Request::builder()
        .uri("/style.css")
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let content_type = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap();
    assert!(content_type.starts_with("text/css"));
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&body), custom_css);

    // 4. SPA fallback serves custom index.html
    let req = Request::builder()
        .uri("/topology")
        .body(Body::empty())
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&body), custom_html);
}

#[tokio::test]
async fn test_path_traversal_protection() {
    let (state, _tmp) = setup_test_state();
    let router = create_router(state);

    let req = Request::builder()
        .uri("/assets/../../etc/passwd")
        .body(Body::empty())
        .unwrap();

    let resp = router.oneshot(req).await.unwrap();
    assert!(resp.status() == StatusCode::BAD_REQUEST || resp.status() == StatusCode::NOT_FOUND);
}
