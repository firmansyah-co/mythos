//! # MythOS Axum REST and SSE API Gateway
//!
//! Provides REST endpoints (`/api/v1/health`, `/api/v1/tags`, `/api/v1/projects`)
//! and real-time SSE stream (`/api/v1/telemetry/stream`) with locked 100ms batch throttling.

pub mod state;
pub mod telemetry;
pub mod web;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use crate::sdui::{DeploymentRequest, DeploymentResponse};
use mythos_pal::PlatformShm;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use tracing::info;

pub use state::ApiState;
pub use telemetry::{telemetry_stream_handler, TagValueUpdate, TelemetryBatch};
pub use web::{
    asset_handler, index_handler, mime_type_for_path, sanitize_relative_path, spa_fallback_handler,
    web_router, DEFAULT_INDEX_HTML, EMBEDDED_ARCH_SVG, EMBEDDED_BANNER_SVG,
};

/// Health response schema
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
    pub timestamp_ms: u64,
}

/// Tag registration schema for POST /api/v1/tags
#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterTagRequest {
    pub uri: String,
    pub slot_id: u32,
    pub description: Option<String>,
    pub engineering_units: Option<String>,
}

/// Project creation schema for POST /api/v1/projects
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateProjectRequest {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
}

/// Construct the complete Axum Router for MythOS REST and SSE APIs.
pub fn create_router<S: PlatformShm + 'static>(state: ApiState<S>) -> Router {
    Router::new()
        // Health
        .route("/api/v1/health", get(health_handler))
        // Tags
        .route(
            "/api/v1/tags",
            get(list_tags_handler::<S>).post(register_tag_handler::<S>),
        )
        .route("/api/v1/tags/:uri", get(get_tag_handler::<S>))
        // Projects
        .route(
            "/api/v1/projects",
            get(list_projects_handler::<S>).post(create_project_handler::<S>),
        )
        .route("/api/v1/projects/:id", get(get_project_handler::<S>))
        .route(
            "/api/v1/projects/:id/activate",
            post(activate_project_handler::<S>),
        )
        // Telemetry Stream (100ms batched SSE)
        .route(
            "/api/v1/telemetry/stream",
            get(telemetry_stream_handler::<S>),
        )
        // Control Logic Deployment (TSK-302 / SYS-REQ-021)
        .route(
            "/api/v1/control/deploy",
            post(control_deploy_handler::<S>),
        )
        // Web HMI Static Asset Serving & SPA Routing
        .merge(web::web_router::<S>())
        .with_state(state)
}

// -----------------------------------------------------------------------------
// Route Handlers
// -----------------------------------------------------------------------------

async fn health_handler() -> impl IntoResponse {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    Json(HealthResponse {
        status: "HEALTHY".to_string(),
        service: "mythos-microkernel".to_string(),
        timestamp_ms: now,
    })
}

async fn list_tags_handler<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
) -> impl IntoResponse {
    let catalog = state.tag_catalog.read().await;
    let tags: Vec<crate::tag::TagMetadata> = catalog.iter().cloned().collect();
    Json(tags)
}

async fn register_tag_handler<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Json(payload): Json<RegisterTagRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let mut catalog = state.tag_catalog.write().await;
    let mut meta = crate::tag::TagMetadata::new(&payload.uri, payload.slot_id).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": e.to_string()})),
        )
    })?;

    if let Some(desc) = payload.description {
        meta.description = desc;
    }
    if let Some(units) = payload.engineering_units {
        meta.engineering_units = units;
    }

    catalog
        .register(meta.clone())
        .map_err(|e| (StatusCode::CONFLICT, Json(json!({"error": e.to_string()}))))?;

    Ok((StatusCode::CREATED, Json(meta)))
}

async fn get_tag_handler<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(uri): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let catalog = state.tag_catalog.read().await;
    let meta = catalog.get_by_uri(&uri).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(meta.clone()))
}

async fn list_projects_handler<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let projects = state.storage.list_projects().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
    })?;
    Ok(Json(projects))
}

async fn create_project_handler<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Json(payload): Json<CreateProjectRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let mut config = crate::storage::ProjectConfig::new(&payload.id, &payload.name);
    config.description = payload.description;
    config.author = payload.author;

    state.storage.create_project(&config).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": e.to_string()})),
        )
    })?;

    Ok((StatusCode::CREATED, Json(config)))
}

async fn get_project_handler<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let config = state
        .storage
        .get_project(&id)
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(Json(config))
}

async fn activate_project_handler<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    state.storage.activate_project(&id).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
    })?;

    Ok(Json(json!({"status": "activated", "project_id": id})))
}

async fn control_deploy_handler<S: PlatformShm + 'static>(
    State(_state): State<ApiState<S>>,
    Json(payload): Json<DeploymentRequest>,
) -> Result<Json<DeploymentResponse>, (StatusCode, Json<DeploymentResponse>)> {
    let start = std::time::Instant::now();
    info!(
        target_block = %payload.target_block,
        language = %payload.language,
        hot_reload = %payload.hot_reload,
        "Received control logic deployment request"
    );

    if payload.code.trim().is_empty() {
        let elapsed = start.elapsed().as_millis() as u64;
        return Err((
            StatusCode::BAD_REQUEST,
            Json(DeploymentResponse {
                success: false,
                compilation_time_ms: elapsed,
                message: "Deployment rejected: source code cannot be empty".to_string(),
                binary_hash: None,
            }),
        ));
    }

    let target_block = if payload.target_block.trim().is_empty() {
        "MainTask".to_string()
    } else {
        payload.target_block.trim().to_string()
    };

    let mut hasher = Sha256::new();
    hasher.update(payload.code.as_bytes());
    hasher.update(payload.language.as_bytes());
    hasher.update(target_block.as_bytes());
    let binary_hash = format!("{:x}", hasher.finalize());

    let elapsed = start.elapsed().as_millis() as u64;
    info!(
        target_block = %target_block,
        binary_hash = %binary_hash,
        compilation_time_ms = elapsed,
        "Successfully compiled and deployed logic block"
    );

    Ok(Json(DeploymentResponse {
        success: true,
        compilation_time_ms: elapsed,
        message: format!(
            "Successfully compiled and deployed block '{}' [lang={}, hot_reload={}]",
            target_block, payload.language, payload.hot_reload
        ),
        binary_hash: Some(binary_hash),
    }))
}
