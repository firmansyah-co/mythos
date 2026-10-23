//! Project-scoped master faceplate type and instance API (TSK-309).

use super::ApiState;
use crate::faceplate::{
    release_event_payload, FaceplateError, FaceplateInstance, FaceplateReleaseResult,
    FaceplateStore, FaceplateType, FaceplateTypeSummary, InstanceBinding, ResolvedInstance,
    UdtDefinition,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use mythos_pal::PlatformShm;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::Mutex;

// Releases and draft saves are read-modify-write on the same project directory; serialize
// them so two designers cannot interleave a draft save with a release.
static FACEPLATE_WRITES: Mutex<()> = Mutex::const_new(());

type ApiError = (StatusCode, Json<Value>);

fn failure(status: StatusCode, message: impl ToString) -> ApiError {
    (status, Json(json!({ "error": message.to_string() })))
}

fn map_error(error: FaceplateError) -> ApiError {
    match error {
        FaceplateError::TypeNotFound(_) | FaceplateError::InstanceNotFound(_) => {
            failure(StatusCode::NOT_FOUND, error)
        }
        FaceplateError::Invalid(_) => failure(StatusCode::BAD_REQUEST, error),
        FaceplateError::ProjectNotFound(_) => failure(StatusCode::NOT_FOUND, error),
        other => failure(StatusCode::INTERNAL_SERVER_ERROR, other),
    }
}

fn store<S: PlatformShm>(state: &ApiState<S>, project: &str) -> Result<FaceplateStore, ApiError> {
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
    Ok(FaceplateStore::new(state.storage.project_dir(project)))
}

/// `?view=active|draft`, defaulting to the published view.
#[derive(Deserialize)]
pub struct ViewQuery {
    view: Option<String>,
}

/// `POST /api/v1/projects/:id/faceplates/types` body.
#[derive(Deserialize)]
pub struct CreateTypeRequest {
    name: String,
    udt: UdtDefinition,
}

/// `POST /api/v1/projects/:id/faceplates/instances` body.
#[derive(Deserialize)]
pub struct CreateInstanceRequest {
    name: String,
    #[serde(rename = "typeId")]
    type_id: String,
    #[serde(rename = "typeVersion", default)]
    type_version: Option<u32>,
    #[serde(default)]
    bindings: Vec<InstanceBinding>,
}

/// `PUT /api/v1/projects/:id/faceplates/instances/:instance` body.
///
/// `typeVersion` is tri-state: absent leaves the freeze state untouched, `null` unfreezes
/// (follow active), a number freezes to that published version.
#[derive(Deserialize)]
pub struct UpdateInstanceRequest {
    #[serde(rename = "typeVersion", default, deserialize_with = "double_option")]
    type_version: Option<Option<u32>>,
    #[serde(default)]
    bindings: Option<Vec<InstanceBinding>>,
}

fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// List types for the Zone 3 tree, read from the draft stage.
pub async fn list_types<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(project): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let store = store(&state, &project)?;
    let types: Vec<FaceplateTypeSummary> = store.type_summaries().map_err(map_error)?;
    Ok(Json(json!({ "types": types })))
}

/// Create a draft-only master type. The id is assigned server-side.
pub async fn create_type<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(project): Path<String>,
    Json(payload): Json<CreateTypeRequest>,
) -> Result<(StatusCode, Json<FaceplateType>), ApiError> {
    let _guard = FACEPLATE_WRITES.lock().await;
    let store = store(&state, &project)?;
    let created = store
        .create_type(payload.name, payload.udt)
        .map_err(map_error)?;
    Ok((StatusCode::CREATED, Json(created)))
}

/// Read one type. `?view=active` (default) returns the published definition; `draft` returns
/// the authored definition.
pub async fn get_type<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path((project, id)): Path<(String, String)>,
    Query(query): Query<ViewQuery>,
) -> Result<Json<FaceplateType>, ApiError> {
    let store = store(&state, &project)?;
    let view = query.view.unwrap_or_else(|| "active".to_owned());
    Ok(Json(store.read_type(&id, &view).map_err(map_error)?))
}

/// Save the authored definition into the draft stage. Bindings are validated against the
/// UDT's declared members.
pub async fn save_draft<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path((project, id)): Path<(String, String)>,
    Json(payload): Json<FaceplateType>,
) -> Result<StatusCode, ApiError> {
    let _guard = FACEPLATE_WRITES.lock().await;
    let store = store(&state, &project)?;
    // The path is authoritative; a mismatched body id would otherwise silently write to a
    // different type than the one the caller addressed.
    if !payload.id.is_empty() && payload.id != id {
        return Err(failure(
            StatusCode::BAD_REQUEST,
            format!(
                "Body type id '{}' does not match route type id '{}'",
                payload.id, id
            ),
        ));
    }
    let mut def = payload;
    def.id = id;
    store.save_draft(def).map_err(map_error)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Promote the draft to a published version and notify open operator stations.
pub async fn release<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path((project, id)): Path<(String, String)>,
) -> Result<Json<FaceplateReleaseResult>, ApiError> {
    let _guard = FACEPLATE_WRITES.lock().await;
    let store = store(&state, &project)?;
    let result = store.release(&id).map_err(map_error)?;

    let at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    state.notify_hmi(super::state::HmiEvent::new(
        "FACEPLATE_PROPAGATED",
        release_event_payload(&result, at_ms),
    ));

    Ok(Json(result))
}

/// List instances resolved against their master at read time.
pub async fn list_instances<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(project): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let store = store(&state, &project)?;
    let instances: Vec<ResolvedInstance> = store.resolve_instances().map_err(map_error)?;
    Ok(Json(json!({ "instances": instances })))
}

/// Place a new instance. Nothing is copied from the master, so a later release applies here.
pub async fn create_instance<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(project): Path<String>,
    Json(payload): Json<CreateInstanceRequest>,
) -> Result<(StatusCode, Json<FaceplateInstance>), ApiError> {
    let _guard = FACEPLATE_WRITES.lock().await;
    let store = store(&state, &project)?;
    let created = store
        .create_instance(
            payload.name,
            payload.type_id,
            payload.type_version,
            payload.bindings,
        )
        .map_err(map_error)?;
    Ok((StatusCode::CREATED, Json(created)))
}

/// Edit an instance's tag bindings and/or freeze it to a published version.
pub async fn update_instance<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path((project, instance)): Path<(String, String)>,
    Json(payload): Json<UpdateInstanceRequest>,
) -> Result<Json<FaceplateInstance>, ApiError> {
    let _guard = FACEPLATE_WRITES.lock().await;
    let store = store(&state, &project)?;
    let updated = store
        .update_instance(&instance, payload.type_version, payload.bindings)
        .map_err(map_error)?;
    Ok(Json(updated))
}
