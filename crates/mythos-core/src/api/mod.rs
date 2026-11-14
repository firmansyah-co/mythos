//! # MythOS Axum REST and SSE API Gateway
//!
//! Provides REST endpoints (`/api/v1/health`, `/api/v1/tags`, `/api/v1/projects`)
//! and real-time SSE stream (`/api/v1/telemetry/stream`) with locked 100ms batch throttling.

pub mod datatypes;
pub mod faceplates;
mod screens;
pub mod state;
pub mod telemetry;
pub mod web;

use crate::sdui::{DeploymentRequest, DeploymentResponse};
use crate::storage::StorageError;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use mythos_pal::PlatformShm;
use serde::{Deserialize, Serialize};
use serde_json::json;

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
        .route("/api/v1/projects/active", get(active_project_handler::<S>))
        .route("/api/v1/projects/:id", get(get_project_handler::<S>))
        .route(
            "/api/v1/projects/:id/activate",
            post(activate_project_handler::<S>),
        )
        // Project-scoped persisted HMI drafts
        .route(
            "/api/v1/projects/:id/hmi/screens",
            get(screens::list::<S>).post(screens::create::<S>),
        )
        .route(
            "/api/v1/projects/:id/hmi/screens/:screen/subscreens",
            post(screens::create_subscreen::<S>),
        )
        .route(
            "/api/v1/projects/:id/hmi/screens/:screen/draft",
            axum::routing::put(screens::save_draft::<S>),
        )
        .route(
            "/api/v1/projects/:id/hmi/commit",
            post(screens::commit::<S>),
        )
        // Project-scoped DUT/UDT registry (logic editor data types)
        .route(
            "/api/v1/projects/:id/datatypes",
            get(datatypes::list::<S>).post(datatypes::create::<S>),
        )
        .route(
            "/api/v1/projects/:id/datatypes/:dut",
            get(datatypes::get::<S>).delete(datatypes::delete::<S>),
        )
        .route(
            "/api/v1/projects/:id/datatypes/:dut/draft",
            put(datatypes::save_draft::<S>),
        )
        .route(
            "/api/v1/projects/:id/datatypes/:dut/release",
            post(datatypes::release::<S>),
        )
        // Project-scoped master faceplate types and instances (TSK-309)
        .route(
            "/api/v1/projects/:id/faceplates/types",
            get(faceplates::list_types::<S>).post(faceplates::create_type::<S>),
        )
        .route(
            "/api/v1/projects/:id/faceplates/types/:type",
            get(faceplates::get_type::<S>),
        )
        .route(
            "/api/v1/projects/:id/faceplates/types/:type/draft",
            put(faceplates::save_draft::<S>),
        )
        .route(
            "/api/v1/projects/:id/faceplates/types/:type/release",
            post(faceplates::release::<S>),
        )
        .route(
            "/api/v1/projects/:id/faceplates/instances",
            get(faceplates::list_instances::<S>).post(faceplates::create_instance::<S>),
        )
        .route(
            "/api/v1/projects/:id/faceplates/instances/:instance",
            put(faceplates::update_instance::<S>),
        )
        // Telemetry Stream (100ms batched SSE)
        .route(
            "/api/v1/telemetry/stream",
            get(telemetry_stream_handler::<S>),
        )
        // Field I/O runtime status
        .route("/api/v1/io/status", get(io_status_handler::<S>))
        // Control logic deployment: answers 501, nothing is compiled (see the handler)
        .route("/api/v1/control/deploy", post(control_deploy_handler::<S>))
        // CRDT Collaboration WebSocket Gateway (TSK-311)
        .route("/api/v1/collab/ws/:room_id", get(collab_ws_handler::<S>))
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

/// `GET /api/v1/io/status` — the connections feeding the tag bus and how their last poll went.
async fn io_status_handler<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
) -> impl IntoResponse {
    Json(state.io_status.snapshot())
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
    let config = state.storage.get_project(&id).map_err(|e| match e {
        StorageError::InvalidProjectId(_) => StatusCode::BAD_REQUEST,
        _ => StatusCode::NOT_FOUND,
    })?;
    Ok(Json(config))
}

/// `GET /api/v1/projects/active` — which project the controller runs, or `null` when none is.
///
/// The field I/O runtime serves the active project only, so a client needs this to know whether
/// the live tags it sees belong to the project it has open.
async fn active_project_handler<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let active = state.storage.get_active_project().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
    })?;
    Ok(Json(json!({ "project_id": active })))
}

async fn activate_project_handler<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    state.storage.activate_project(&id).map_err(|e| {
        let status = match e {
            StorageError::InvalidProjectId(_) => StatusCode::BAD_REQUEST,
            StorageError::ProjectNotFound(_) => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(json!({"error": e.to_string()})))
    })?;

    Ok(Json(json!({"status": "activated", "project_id": id})))
}

/// `POST /api/v1/control/deploy` — not implemented.
///
/// No compiler and no logic runtime are linked into this build, so nothing can be compiled or
/// deployed. The route answers `501` with `success: false` instead of being absent so that a
/// client sees an explicit refusal in the response shape it already parses. It must never
/// report success: the handler it replaces hashed the request text and answered
/// "Successfully compiled and deployed" for any input.
async fn control_deploy_handler<S: PlatformShm + 'static>(
    State(_state): State<ApiState<S>>,
    Json(_payload): Json<DeploymentRequest>,
) -> (StatusCode, Json<DeploymentResponse>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(DeploymentResponse {
            success: false,
            compilation_time_ms: 0,
            message: "Logic deployment is not implemented in this build: no compiler or logic \
                      runtime is present. Nothing was compiled and nothing was deployed."
                .to_string(),
            binary_hash: None,
        }),
    )
}

async fn collab_ws_handler<S: PlatformShm + 'static>(
    ws: WebSocketUpgrade,
    Path(room_id): Path<String>,
    State(state): State<ApiState<S>>,
) -> impl IntoResponse {
    let room = state.room_manager.get_or_create_room(&room_id).await;
    ws.on_upgrade(move |socket| crate::collab::handle_collab_ws(socket, room))
}
