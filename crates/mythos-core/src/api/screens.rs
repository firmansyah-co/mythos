//! Project-scoped HMI screen drafts, persisted without simulated entities.

use super::ApiState;
use axum::{extract::{Path, State}, http::StatusCode, Json};
use mythos_pal::PlatformShm;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::PathBuf};
use tokio::sync::Mutex;

// Serialize read-modify-write operations, including concurrent browser requests.
static SCREEN_WRITES: Mutex<()> = Mutex::const_new(());
type ApiError = (StatusCode, Json<Value>);

#[derive(Clone, Serialize, Deserialize)]
pub struct Screen {
    id: String,
    name: String,
    is_root: bool,
    #[serde(default)]
    subscreens: Vec<SubScreen>,
    #[serde(rename = "canvasData")]
    canvas_data: Value,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SubScreen {
    id: String,
    name: String,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Deserialize)]
pub struct CreateScreen {
    name: String,
    #[serde(default)]
    is_root: bool,
}

#[derive(Deserialize)]
pub struct CreateSubScreen {
    name: String,
    #[serde(rename = "type")]
    kind: String,
}

fn failure(status: StatusCode, message: impl ToString) -> ApiError {
    (status, Json(json!({ "error": message.to_string() })))
}

fn project_dir<S: PlatformShm>(state: &ApiState<S>, id: &str) -> Result<PathBuf, ApiError> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
        return Err(failure(StatusCode::BAD_REQUEST, "Invalid project ID"));
    }
    state.storage.get_project(id)
        .map_err(|e| failure(StatusCode::NOT_FOUND, e))?;
    Ok(state.storage.project_dir(id))
}

fn read_screens(dir: &std::path::Path, stage: &str) -> Result<Vec<Screen>, ApiError> {
    let path = dir.join(format!("screens/{stage}/screens.json"));
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|e| failure(StatusCode::INTERNAL_SERVER_ERROR, e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(failure(StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

fn write_screens<S: PlatformShm>(state: &ApiState<S>, dir: &std::path::Path, stage: &str, screens: &[Screen]) -> Result<(), ApiError> {
    let bytes = serde_json::to_vec_pretty(screens)
        .map_err(|e| failure(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    state.storage.atomic_write_file(&dir.join(format!("screens/{stage}/screens.json")), &bytes)
        .map_err(|e| failure(StatusCode::INTERNAL_SERVER_ERROR, e))
}

pub async fn list<S: PlatformShm>(State(state): State<ApiState<S>>, Path(project): Path<String>) -> Result<Json<Vec<Screen>>, ApiError> {
    let _guard = SCREEN_WRITES.lock().await;
    let dir = project_dir(&state, &project)?;
    Ok(Json(read_screens(&dir, "draft")?))
}

pub async fn create<S: PlatformShm>(State(state): State<ApiState<S>>, Path(project): Path<String>, Json(payload): Json<CreateScreen>) -> Result<(StatusCode, Json<Screen>), ApiError> {
    if payload.name.trim().is_empty() {
        return Err(failure(StatusCode::BAD_REQUEST, "Screen name is required"));
    }
    let _guard = SCREEN_WRITES.lock().await;
    let dir = project_dir(&state, &project)?;
    let mut screens = read_screens(&dir, "draft")?;
    let screen = Screen {
        id: format!("screen_{:032x}", rand::random::<u128>()),
        name: payload.name.trim().to_owned(),
        is_root: payload.is_root,
        subscreens: Vec::new(),
        canvas_data: json!({"title": payload.name.trim(), "components": []}),
    };
    screens.push(screen.clone());
    write_screens(&state, &dir, "draft", &screens)?;
    Ok((StatusCode::CREATED, Json(screen)))
}

pub async fn create_subscreen<S: PlatformShm>(State(state): State<ApiState<S>>, Path((project, screen)): Path<(String, String)>, Json(payload): Json<CreateSubScreen>) -> Result<(StatusCode, Json<SubScreen>), ApiError> {
    if payload.name.trim().is_empty() || !["POPUP", "SLIDE_OVER", "EMBEDDED"].contains(&payload.kind.as_str()) {
        return Err(failure(StatusCode::BAD_REQUEST, "Invalid sub-screen name or type"));
    }
    let _guard = SCREEN_WRITES.lock().await;
    let dir = project_dir(&state, &project)?;
    let mut screens = read_screens(&dir, "draft")?;
    let target = screens.iter_mut().find(|s| s.id == screen)
        .ok_or_else(|| failure(StatusCode::NOT_FOUND, "Screen not found"))?;
    let sub = SubScreen { id: format!("sub_{:032x}", rand::random::<u128>()), name: payload.name.trim().to_owned(), kind: payload.kind };
    target.subscreens.push(sub.clone());
    write_screens(&state, &dir, "draft", &screens)?;
    Ok((StatusCode::CREATED, Json(sub)))
}

pub async fn save_draft<S: PlatformShm>(State(state): State<ApiState<S>>, Path((project, screen)): Path<(String, String)>, Json(canvas): Json<Value>) -> Result<StatusCode, ApiError> {
    if !canvas.is_object() || !canvas.get("components").is_some_and(Value::is_array) {
        return Err(failure(StatusCode::BAD_REQUEST, "Canvas must contain a components array"));
    }
    let _guard = SCREEN_WRITES.lock().await;
    let dir = project_dir(&state, &project)?;
    let mut screens = read_screens(&dir, "draft")?;
    let target = screens.iter_mut().find(|s| s.id == screen)
        .ok_or_else(|| failure(StatusCode::NOT_FOUND, "Screen not found"))?;
    target.canvas_data = canvas;
    write_screens(&state, &dir, "draft", &screens)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn commit<S: PlatformShm>(State(state): State<ApiState<S>>, Path(project): Path<String>) -> Result<Json<BTreeMap<String, Value>>, ApiError> {
    let _guard = SCREEN_WRITES.lock().await;
    let dir = project_dir(&state, &project)?;
    let screens = read_screens(&dir, "draft")?;
    write_screens(&state, &dir, "active", &screens)?;
    Ok(Json(screens.into_iter().map(|s| (s.id, s.canvas_data)).collect()))
}
