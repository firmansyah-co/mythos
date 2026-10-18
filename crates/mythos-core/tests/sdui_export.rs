//! Integration tests for Server-Driven UI (SDUI) TypeScript type generation (TSK-302 / ADR-006).

use mythos_core::sdui::{
    ComponentType, DeploymentRequest, DeploymentResponse, FaceplateBindingConfig,
    HmiLifecycleMode, SduiComponent, SduiProperty, SduiScreenLayout, SduiZone,
};
use std::fs;
use std::path::PathBuf;
use ts_rs::TS;

#[test]
fn test_sdui_typescript_type_export() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("Workspace root must exist");

    let targets = vec![
        workspace_root.join("frontend/packages/ui-shell/src/types/generated"),
        workspace_root.join("frontend/src/types/generated"),
    ];

    for target_dir in &targets {
        fs::create_dir_all(target_dir).expect("Failed to create target generated types directory");

        // Export all SDUI and deployment types
        HmiLifecycleMode::export_all_to(target_dir).expect("Failed to export HmiLifecycleMode");
        ComponentType::export_all_to(target_dir).expect("Failed to export ComponentType");
        SduiProperty::export_all_to(target_dir).expect("Failed to export SduiProperty");
        FaceplateBindingConfig::export_all_to(target_dir).expect("Failed to export FaceplateBindingConfig");
        SduiComponent::export_all_to(target_dir).expect("Failed to export SduiComponent");
        SduiZone::export_all_to(target_dir).expect("Failed to export SduiZone");
        SduiScreenLayout::export_all_to(target_dir).expect("Failed to export SduiScreenLayout");
        DeploymentRequest::export_all_to(target_dir).expect("Failed to export DeploymentRequest");
        DeploymentResponse::export_all_to(target_dir).expect("Failed to export DeploymentResponse");

        // Also create an index.ts barrel export for frontend convenience
        let index_file = target_dir.join("index.ts");
        let barrel_content = r#"// Auto-generated TypeScript definitions from MythOS Rust microkernel (TSK-302).
// Generated via `ts-rs` during `cargo test`. Do not edit manually.

export * from './HmiLifecycleMode';
export * from './ComponentType';
export * from './SduiProperty';
export * from './FaceplateBindingConfig';
export * from './SduiComponent';
export * from './SduiZone';
export * from './SduiScreenLayout';
export * from './DeploymentRequest';
export * from './DeploymentResponse';
"#;
        fs::write(&index_file, barrel_content).expect("Failed to write index.ts barrel");
    }

    // Verify generated files exist in both targets
    for target_dir in &targets {
        assert!(target_dir.join("HmiLifecycleMode.ts").exists());
        assert!(target_dir.join("ComponentType.ts").exists());
        assert!(target_dir.join("SduiProperty.ts").exists());
        assert!(target_dir.join("FaceplateBindingConfig.ts").exists());
        assert!(target_dir.join("SduiComponent.ts").exists());
        assert!(target_dir.join("SduiZone.ts").exists());
        assert!(target_dir.join("SduiScreenLayout.ts").exists());
        assert!(target_dir.join("DeploymentRequest.ts").exists());
        assert!(target_dir.join("DeploymentResponse.ts").exists());
        assert!(target_dir.join("index.ts").exists());
    }
}

#[tokio::test]
async fn test_control_deploy_endpoint_success() {
    use axum::body::Body;
    use axum::http::{header, Request, StatusCode};
    use mythos_core::api::{create_router, ApiState};
    use mythos_core::sdui::{DeploymentRequest, DeploymentResponse};
    use mythos_core::storage::ControllerStorage;
    use mythos_core::tag::TagCatalog;
    use std::sync::Arc;
    use tempfile::tempdir;
    use tokio::sync::RwLock;
    use tower::ServiceExt;

    let tmp_dir = tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(tmp_dir.path()).unwrap());
    let catalog = Arc::new(RwLock::new(TagCatalog::new()));
    let state = ApiState::<mythos_pal::MemoryShm>::new(storage, catalog, None);
    let router = create_router(state);

    let deploy_req = DeploymentRequest {
        code: "PROGRAM Main\nVAR x: INT; END_VAR\nx := x + 1;\nEND_PROGRAM".to_string(),
        language: "ST".to_string(),
        target_block: "MainTask".to_string(),
        hot_reload: true,
    };

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/control/deploy")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&deploy_req).unwrap()))
        .unwrap();

    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let deploy_resp: DeploymentResponse = serde_json::from_slice(&body_bytes).unwrap();

    assert!(deploy_resp.success);
    assert!(deploy_resp.binary_hash.is_some());
    assert!(deploy_resp.message.contains("MainTask"));
    assert!(deploy_resp.message.contains("lang=ST"));
    assert!(deploy_resp.message.contains("hot_reload=true"));
}

#[tokio::test]
async fn test_control_deploy_endpoint_empty_code_rejected() {
    use axum::body::Body;
    use axum::http::{header, Request, StatusCode};
    use mythos_core::api::{create_router, ApiState};
    use mythos_core::sdui::{DeploymentRequest, DeploymentResponse};
    use mythos_core::storage::ControllerStorage;
    use mythos_core::tag::TagCatalog;
    use std::sync::Arc;
    use tempfile::tempdir;
    use tokio::sync::RwLock;
    use tower::ServiceExt;

    let tmp_dir = tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(tmp_dir.path()).unwrap());
    let catalog = Arc::new(RwLock::new(TagCatalog::new()));
    let state = ApiState::<mythos_pal::MemoryShm>::new(storage, catalog, None);
    let router = create_router(state);

    let deploy_req = DeploymentRequest {
        code: "   ".to_string(),
        language: "ST".to_string(),
        target_block: "MainTask".to_string(),
        hot_reload: false,
    };

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/control/deploy")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&deploy_req).unwrap()))
        .unwrap();

    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let deploy_resp: DeploymentResponse = serde_json::from_slice(&body_bytes).unwrap();

    assert!(!deploy_resp.success);
    assert!(deploy_resp.binary_hash.is_none());
    assert!(deploy_resp.message.contains("cannot be empty"));
}
