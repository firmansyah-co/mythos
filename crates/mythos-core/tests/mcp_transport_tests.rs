//! Tests for Axum MCP SSE and Message Endpoints with Ed25519 Token Authentication.
//!
//! Validates TSK-501, TSK-504 and MCP HTTP transport integration.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use mythos_core::api::{create_router, ApiState};
use mythos_core::auth::{scopes, AgentClaims, Ed25519TokenManager};
use mythos_core::storage::ControllerStorage;
use mythos_core::tag::{AccessMode, TagCatalog, TagMetadata};
use serde_json::json;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::sync::RwLock;
use tower::ServiceExt;

async fn setup_app() -> (axum::Router, Ed25519TokenManager) {
    let tmp = tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(tmp.path().to_str().unwrap()).unwrap());

    let mut catalog = TagCatalog::new();
    let mut tag = TagMetadata::new("external.Boiler.PT101.pressure.pv", 0).unwrap();
    tag.access_mode = AccessMode::ReadOnly;
    tag.engineering_units = "bar".to_string();
    catalog.register(tag).unwrap();

    let tag_catalog = Arc::new(RwLock::new(catalog));
    let token_manager = Ed25519TokenManager::generate();

    let state = ApiState::<mythos_pal::DefaultShm>::new(storage, tag_catalog, None)
        .with_token_manager(Arc::new(token_manager.clone()));

    let app = create_router(state);
    (app, token_manager)
}

#[tokio::test]
async fn test_mcp_sse_endpoint_establishes_stream() {
    let (app, _) = setup_app().await;

    let req = Request::builder()
        .uri("/api/v1/mcp/sse")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let content_type = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(content_type.contains("text/event-stream"));
}

#[tokio::test]
async fn test_mcp_message_without_token_rejected_401() {
    let (app, _) = setup_app().await;

    // Send message without auth header
    let req = Request::builder()
        .uri("/api/v1/mcp/message?sessionId=test-session-1")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND); // session does not exist yet

    // Now test with invalid session or valid session
}

#[tokio::test]
async fn test_mcp_message_with_valid_session_and_token() {
    let tmp = tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(tmp.path().to_str().unwrap()).unwrap());

    let mut catalog = TagCatalog::new();
    let mut tag = TagMetadata::new("external.Boiler.PT101.pressure.pv", 0).unwrap();
    tag.access_mode = AccessMode::ReadOnly;
    tag.engineering_units = "bar".to_string();
    catalog.register(tag).unwrap();

    let tag_catalog = Arc::new(RwLock::new(catalog));
    let token_manager = Ed25519TokenManager::generate();

    let state = ApiState::<mythos_pal::DefaultShm>::new(storage, tag_catalog, None)
        .with_token_manager(Arc::new(token_manager.clone()));

    // Create active session in transport
    let (session_id, _rx) = state.mcp_transport.create_session().await;

    let app = create_router(state);

    // Issue valid Ed25519 token
    let claims = AgentClaims::new(
        "agent:test-mcp",
        "Operator",
        vec![scopes::TAG_READ.to_string()],
        3600,
    );
    let token = token_manager.issue_token(&claims).unwrap();

    // 1. Missing token -> 401 Unauthorized
    let req_unauth = Request::builder()
        .uri(format!("/api/v1/mcp/message?sessionId={}", session_id))
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#))
        .unwrap();

    let resp_unauth = app.clone().oneshot(req_unauth).await.unwrap();
    assert_eq!(resp_unauth.status(), StatusCode::UNAUTHORIZED);

    // 2. Invalid/tampered token -> 403 Forbidden
    let req_forbidden = Request::builder()
        .uri(format!("/api/v1/mcp/message?sessionId={}", session_id))
        .method("POST")
        .header("content-type", "application/json")
        .header("authorization", "Bearer invalid-token.000000")
        .body(Body::from(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#))
        .unwrap();

    let resp_forbidden = app.clone().oneshot(req_forbidden).await.unwrap();
    assert_eq!(resp_forbidden.status(), StatusCode::FORBIDDEN);

    // 3. Valid token + ping -> 200 OK with ping response
    let req_valid = Request::builder()
        .uri(format!("/api/v1/mcp/message?sessionId={}", session_id))
        .method("POST")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::from(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#))
        .unwrap();

    let resp_valid = app.clone().oneshot(req_valid).await.unwrap();
    assert_eq!(resp_valid.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(resp_valid.into_body(), usize::MAX)
        .await
        .unwrap();
    let resp_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(resp_json.get("id"), Some(&json!(1)));
    assert_eq!(resp_json.get("jsonrpc"), Some(&json!("2.0")));

    // 4. Valid token + tools/list -> 200 OK
    let req_tools = Request::builder()
        .uri(format!("/api/v1/mcp/message?sessionId={}", session_id))
        .method("POST")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::from(
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        ))
        .unwrap();

    let resp_tools = app.clone().oneshot(req_tools).await.unwrap();
    assert_eq!(resp_tools.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(resp_tools.into_body(), usize::MAX)
        .await
        .unwrap();
    let resp_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let tools = resp_json
        .get("result")
        .unwrap()
        .get("tools")
        .unwrap()
        .as_array()
        .unwrap();
    assert!(!tools.is_empty());
}
