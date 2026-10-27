//! Project-scoped DUT/UDT registry API.
//!
//! Follows the same project-scoped shape as screens and faceplate types. There is no seed data and
//! no default type: a project with no data types returns an empty list, and the editor is expected
//! to render an explicit empty state rather than sample types (SYS-REQ-098).

use super::ApiState;
use crate::datatype::{DataTypeError, DataTypeStore, DataTypeSummary, UdtDefinition};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use mythos_pal::PlatformShm;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::Mutex;

// Draft saves and releases are read-modify-write over the same project directory.
static DATATYPE_WRITES: Mutex<()> = Mutex::const_new(());

type ApiError = (StatusCode, Json<Value>);

fn failure(status: StatusCode, message: impl ToString) -> ApiError {
    (status, Json(json!({ "error": message.to_string() })))
}

fn map_error(error: DataTypeError) -> ApiError {
    match error {
        DataTypeError::NotFound(_) => failure(StatusCode::NOT_FOUND, error),
        DataTypeError::Invalid(_) => failure(StatusCode::BAD_REQUEST, error),
        other => failure(StatusCode::INTERNAL_SERVER_ERROR, other),
    }
}

fn store<S: PlatformShm>(state: &ApiState<S>, project: &str) -> Result<DataTypeStore, ApiError> {
    if project.is_empty()
        || !project
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(failure(StatusCode::BAD_REQUEST, "Invalid project ID"));
    }
    state.storage.get_project(project).map_err(|_| {
        failure(
            StatusCode::NOT_FOUND,
            format!("Project '{project}' not found"),
        )
    })?;
    Ok(DataTypeStore::new(state.storage.project_dir(project)))
}

#[derive(Deserialize)]
pub struct ViewQuery {
    view: Option<String>,
}

/// `POST /api/v1/projects/:id/datatypes` body.
#[derive(Deserialize)]
pub struct CreateDataTypeRequest {
    #[serde(flatten)]
    definition: UdtDefinition,
}

/// `PUT /api/v1/projects/:id/datatypes/:dut/draft` body.
#[derive(Deserialize)]
pub struct SaveDataTypeRequest {
    #[serde(flatten)]
    definition: UdtDefinition,
}

/// List the project's data types for the editor's picker, read from the draft stage.
pub async fn list<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(project): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let store = store(&state, &project)?;
    let types: Vec<DataTypeSummary> = store.summaries().map_err(map_error)?;
    Ok(Json(json!({ "types": types })))
}

/// Create a draft data type. The id is assigned server-side.
pub async fn create<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(project): Path<String>,
    Json(payload): Json<CreateDataTypeRequest>,
) -> Result<(StatusCode, Json<crate::datatype::DataTypeEntry>), ApiError> {
    let _guard = DATATYPE_WRITES.lock().await;
    let store = store(&state, &project)?;
    let created = store.create_type(payload.definition).map_err(map_error)?;
    Ok((StatusCode::CREATED, Json(created)))
}

/// Read one data type. `?view=draft` returns the authored definition; default is the released one.
pub async fn get<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path((project, dut)): Path<(String, String)>,
    Query(query): Query<ViewQuery>,
) -> Result<Json<crate::datatype::DataTypeEntry>, ApiError> {
    let store = store(&state, &project)?;
    let view = query.view.unwrap_or_else(|| "active".to_owned());
    Ok(Json(store.read_type(&dut, &view).map_err(map_error)?))
}

/// Save the authored definition into the draft stage.
pub async fn save_draft<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path((project, dut)): Path<(String, String)>,
    Json(payload): Json<SaveDataTypeRequest>,
) -> Result<Json<crate::datatype::DataTypeEntry>, ApiError> {
    let _guard = DATATYPE_WRITES.lock().await;
    let store = store(&state, &project)?;
    let saved = store
        .save_draft(&dut, payload.definition)
        .map_err(map_error)?;
    Ok(Json(saved))
}

/// Promote the draft to a released version.
pub async fn release<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path((project, dut)): Path<(String, String)>,
) -> Result<Json<crate::datatype::DataTypeEntry>, ApiError> {
    let _guard = DATATYPE_WRITES.lock().await;
    let store = store(&state, &project)?;
    Ok(Json(store.release(&dut).map_err(map_error)?))
}

/// Remove a draft data type. Released versions are retained.
pub async fn delete<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path((project, dut)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let _guard = DATATYPE_WRITES.lock().await;
    let store = store(&state, &project)?;
    store.delete_draft(&dut).map_err(map_error)?;
    Ok(StatusCode::NO_CONTENT)
}
