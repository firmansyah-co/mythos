//! A project's field I/O configuration: read it, check it, save it, and apply it.

use crate::api::state::ApiState;
use crate::io::{ConfigProblem, DriverRegistry, IoConfig, IoManager, IO_CONFIG_FILE};
use crate::storage::StorageError;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use mythos_pal::PlatformShm;
use serde_json::{json, Value};
use std::path::PathBuf;

type ApiError = (StatusCode, Json<Value>);

fn failure(status: StatusCode, message: impl ToString) -> ApiError {
    (status, Json(json!({ "error": message.to_string() })))
}

/// The path of an existing project's `io.json`.
fn config_path<S: PlatformShm>(state: &ApiState<S>, project: &str) -> Result<PathBuf, ApiError> {
    let dir = state
        .storage
        .project_dir(project)
        .map_err(|e| failure(StatusCode::BAD_REQUEST, e))?;
    state.storage.get_project(project).map_err(|e| match e {
        StorageError::InvalidProjectId(_) => failure(StatusCode::BAD_REQUEST, e),
        _ => failure(
            StatusCode::NOT_FOUND,
            format!("Project '{project}' not found"),
        ),
    })?;
    Ok(dir.join(IO_CONFIG_FILE))
}

/// `GET /api/v1/io/drivers` — the driver names a configuration may use.
pub async fn drivers<S: PlatformShm + 'static>(State(state): State<ApiState<S>>) -> Json<Value> {
    let names = match &state.io_manager {
        Some(io) => io.driver_names(),
        None => Vec::new(),
    };
    Json(json!(names))
}

/// `POST /api/v1/modules/rescan` — look for modules again and swap every running connection
/// whose driver changed to what is installed now. The server also does this by itself when a
/// module executable is added, removed or replaced.
pub async fn rescan_modules<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
) -> Result<Json<Value>, ApiError> {
    match &state.io_manager {
        Some(io) => Ok(Json(json!(io.rescan().await))),
        None => Err(failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "this server runs no field I/O",
        )),
    }
}

/// `GET /api/v1/modules` — the modules this server found, what each provides, the schema of
/// the configuration it accepts, and the executables that could not be used as modules.
pub async fn modules<S: PlatformShm + 'static>(State(state): State<ApiState<S>>) -> Json<Value> {
    let services = match &state.services {
        Some(host) => json!(host.statuses().await),
        None => json!([]),
    };
    match &state.io_manager {
        Some(io) => Json(json!({
            "drivers": io.drivers(),
            "services": services,
            "problems": io.module_problems(),
        })),
        None => Json(json!({ "drivers": [], "services": services, "problems": [] })),
    }
}

/// `GET /api/v1/projects/:id/io` — the project's configuration, or `null` when it has none.
pub async fn get_config<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(project): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let path = config_path(&state, &project)?;
    let config =
        IoConfig::load(&path).map_err(|e| failure(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(json!({ "config": config })))
}

/// `PUT /api/v1/projects/:id/io` — check, save and, for the active project, apply.
///
/// A configuration with any problem is refused whole and nothing is written: a file that the
/// runtime would reject at start is not something to store. On success `applied` says whether
/// the running field I/O was replaced, which happens only when the project is the active one.
pub async fn put_config<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(project): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let path = config_path(&state, &project)?;

    let config: IoConfig = serde_json::from_value(body).map_err(|e| {
        failure(
            StatusCode::BAD_REQUEST,
            format!("not an I/O configuration: {e}"),
        )
    })?;

    let problems: Vec<ConfigProblem> = match &state.io_manager {
        Some(io) => io.check(&config).await,
        None => IoManager::<S>::check_with(&DriverRegistry::empty(), &config).await,
    };
    if !problems.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "the I/O configuration has problems and was not saved",
                "problems": problems,
            })),
        ));
    }

    let bytes = serde_json::to_vec_pretty(&config)
        .map_err(|e| failure(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    state
        .storage
        .atomic_write_file(&path, &bytes)
        .map_err(|e| failure(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let is_active = matches!(
        state.storage.get_active_project(),
        Ok(Some(active)) if active == project
    );
    let (applied, changes) = match (&state.io_manager, is_active) {
        (Some(io), true) => (true, io.reload().await),
        _ => (false, None),
    };

    Ok(Json(
        json!({ "config": config, "applied": applied, "changes": changes }),
    ))
}
