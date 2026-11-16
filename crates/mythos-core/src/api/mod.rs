//! # MythOS Axum REST and SSE API Gateway
//!
//! REST endpoints for health, tags (read-only), projects and their HMI, data-type, faceplate
//! and I/O configuration, and the SSE telemetry stream (`/api/v1/telemetry/stream`, one batch
//! every 100 ms).

pub mod auth;
pub mod datatypes;
pub mod faceplates;
mod io;
pub mod modules;
mod screens;
mod services;
pub mod state;
pub mod telemetry;
mod updates;
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

/// Project creation schema for POST /api/v1/projects
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateProjectRequest {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
}

/// Construct the complete Axum Router: the REST and SSE API, its MCP endpoint (`/mcp`), and the
/// web workbench.
pub fn create_router<S: PlatformShm + 'static>(state: ApiState<S>) -> Router {
    let mut routes = api_routes::<S>();
    if let Some(accounts) = state.auth.clone() {
        // A route layer, so the matched route pattern is known when the policy is looked up.
        routes = routes.route_layer(axum::middleware::from_fn_with_state(
            accounts,
            auth::enforce,
        ));
    }
    let api: Router = routes.with_state(state.clone());
    Router::new()
        .merge(api.clone())
        .merge(crate::mcp::router(
            api,
            state.auth.clone(),
            state.services.clone(),
        ))
        // Web HMI Static Asset Serving & SPA Routing
        .merge(web::web_router::<S>().with_state(state))
}

/// Every `/api/v1` route. Each one has an MCP tool or a stated reason not to (`crate::mcp`).
fn api_routes<S: PlatformShm + 'static>() -> Router<ApiState<S>> {
    Router::new()
        // Health
        .route("/api/v1/health", get(health_handler))
        // Signing in, and one's own account
        .route("/api/v1/auth/status", get(auth::status::<S>))
        .route("/api/v1/auth/setup", post(auth::setup::<S>))
        .route("/api/v1/auth/login", post(auth::login::<S>))
        .route("/api/v1/auth/logout", post(auth::logout::<S>))
        .route("/api/v1/auth/me", get(auth::me))
        .route("/api/v1/auth/me/password", put(auth::change_password::<S>))
        .route(
            "/api/v1/auth/me/tokens",
            get(auth::list_tokens::<S>).post(auth::create_token::<S>),
        )
        .route(
            "/api/v1/auth/me/tokens/:token",
            axum::routing::delete(auth::revoke_token::<S>),
        )
        // Settings → Updates
        .route("/api/v1/updates", get(updates::status::<S>))
        .route("/api/v1/updates/releases", get(updates::releases::<S>))
        .route("/api/v1/updates/follow", put(updates::follow::<S>))
        .route("/api/v1/updates/install", post(updates::install::<S>))
        .route("/api/v1/updates/rollback", post(updates::rollback::<S>))
        // Accounts (administrators)
        .route(
            "/api/v1/users",
            get(auth::list_users::<S>).post(auth::create_user::<S>),
        )
        .route(
            "/api/v1/users/:username",
            put(auth::update_user::<S>).delete(auth::delete_user::<S>),
        )
        // Tags
        // Read-only. A tag exists because the field I/O runtime registered it and feeds it;
        // there is no route to create one, because nothing would feed it.
        .route("/api/v1/tags", get(list_tags_handler::<S>))
        .route(
            "/api/v1/tags/values",
            get(telemetry::tag_values_handler::<S>),
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
            "/api/v1/projects/:id/hmi/screens/:screen",
            axum::routing::delete(screens::delete::<S>),
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
        .route("/api/v1/io/drivers", get(io::drivers::<S>))
        .route("/api/v1/modules", get(io::modules::<S>))
        .route("/api/v1/modules/rescan", post(io::rescan_modules::<S>))
        // A module's workbench UI (ADR-011 §2.3)
        .route("/modules/:module/ui/*path", get(modules::ui_file::<S>))
        // Settings → Modules (administrators)
        .route(
            "/api/v1/modules/install",
            post(modules::install::<S>)
                // A package carries an executable per platform: allow far more than the default.
                .layer(axum::extract::DefaultBodyLimit::max(512 * 1024 * 1024)),
        )
        .route(
            "/api/v1/modules/install-url",
            post(modules::install_url::<S>),
        )
        .route(
            "/api/v1/modules/:module/enabled",
            put(modules::set_enabled::<S>),
        )
        .route(
            "/api/v1/modules/:module",
            axum::routing::delete(modules::remove::<S>),
        )
        // A project's field I/O configuration
        .route(
            "/api/v1/projects/:id/io",
            get(io::get_config::<S>).put(io::put_config::<S>),
        )
        // Control logic deployment: answers 501, nothing is compiled (see the handler)
        .route("/api/v1/control/deploy", post(control_deploy_handler::<S>))
        // Every route of a service module (ADR-011), forwarded to its process
        .route(
            "/api/v1/projects/:id/:module/*rest",
            get(services::project_route::<S>)
                .post(services::project_route::<S>)
                .put(services::project_route::<S>)
                .delete(services::project_route::<S>),
        )
        .route(
            "/api/v1/m/:module/*rest",
            get(services::global_route::<S>)
                .post(services::global_route::<S>)
                .put(services::global_route::<S>)
                .delete(services::global_route::<S>),
        )
        // CRDT Collaboration WebSocket Gateway (TSK-311)
        .route("/api/v1/collab/ws/:room_id", get(collab_ws_handler::<S>))
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

    // The controller now runs a different project, so it runs that project's field I/O.
    if let Some(io) = &state.io_manager {
        io.reload().await;
    }

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
