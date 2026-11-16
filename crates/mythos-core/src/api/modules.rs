//! Settings → Modules (ADR-011 §2.4): switch a module off and on, install one from a package,
//! remove one. Administrators only (`crate::auth::policy`).

use super::ApiState;
use crate::modules::ModuleError;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use mythos_module::protocol::ModuleKind;
use mythos_module::ModuleBinary;
use mythos_pal::PlatformShm;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({ "error": message.into() }))).into_response()
}

fn failed(e: ModuleError) -> Response {
    let status = match e {
        ModuleError::BadPackage(_) | ModuleError::NoBuildForPlatform(_) => {
            StatusCode::UNPROCESSABLE_ENTITY
        }
        ModuleError::Conflict(_) => StatusCode::CONFLICT,
        ModuleError::NotFound(_) => StatusCode::NOT_FOUND,
        ModuleError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    error(status, e.to_string())
}

/// One module, as Settings → Modules shows it.
#[derive(Debug, Clone, Serialize)]
pub struct ModuleRow {
    pub id: String,
    pub name: String,
    pub version: String,
    pub kind: ModuleKind,
    pub provides: String,
    pub description: String,
    pub path: String,
    pub enabled: bool,
    /// Installed from a package (in the install directory), so it can be removed. A module
    /// beside the server belongs to the MythOS installation and can only be disabled.
    pub removable: bool,
}

/// Every module the server found, used or switched off.
pub fn rows<S: PlatformShm + 'static>(state: &ApiState<S>) -> Vec<ModuleRow> {
    let (Some(io), Some(settings)) = (&state.io_manager, &state.modules) else {
        return Vec::new();
    };
    let install_dir = settings.install_dir();
    let mut rows: Vec<ModuleRow> = io
        .all_modules()
        .iter()
        .map(|m| ModuleRow {
            id: m.manifest.id.clone(),
            name: m.manifest.name.clone(),
            version: m.manifest.version.clone(),
            kind: m.manifest.kind,
            provides: m.manifest.provides.clone(),
            description: m.manifest.description.clone(),
            path: m.path.display().to_string(),
            enabled: !settings.is_disabled(&m.manifest.id),
            removable: m.path.parent() == Some(install_dir.as_path()),
        })
        .collect();
    rows.sort_by(|a, b| a.id.cmp(&b.id));
    rows
}

/// Read the module directories again and wait until the service modules follow, so that what
/// the caller is told is what is running.
async fn apply<S: PlatformShm + 'static>(state: &ApiState<S>) {
    if let Some(io) = &state.io_manager {
        io.rescan().await;
        if let Some(services) = &state.services {
            services.sync(io.service_modules()).await;
        }
    }
}

fn find<S: PlatformShm + 'static>(state: &ApiState<S>, id: &str) -> Option<Arc<ModuleBinary>> {
    state
        .io_manager
        .as_ref()?
        .all_modules()
        .into_iter()
        .find(|m| m.manifest.id == id)
}

#[derive(Deserialize)]
pub struct Enabled {
    enabled: bool,
}

/// `PUT /api/v1/modules/:module/enabled`
pub async fn set_enabled<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(id): Path<String>,
    Json(body): Json<Enabled>,
) -> Response {
    let Some(settings) = state.modules.clone() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "this server does not manage modules",
        );
    };
    if find(&state, &id).is_none() {
        return failed(ModuleError::NotFound(id));
    }
    if let Err(e) = settings.set_enabled(&id, body.enabled) {
        return failed(e);
    }
    apply(&state).await;
    Json(json!({ "modules": rows(&state) })).into_response()
}

async fn install_bytes<S: PlatformShm + 'static>(state: &ApiState<S>, package: &[u8]) -> Response {
    let (Some(settings), Some(io)) = (state.modules.clone(), state.io_manager.clone()) else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "this server does not manage modules",
        );
    };
    let others: Vec<_> = io
        .all_modules()
        .iter()
        .map(|m| (m.manifest.clone(), m.path.clone()))
        .collect();
    match settings.install(package, &others).await {
        Ok(manifest) => {
            apply(state).await;
            (
                StatusCode::CREATED,
                Json(json!({
                    "installed": manifest.id,
                    "version": manifest.version,
                    "enabled": !settings.is_disabled(&manifest.id),
                    "modules": rows(state),
                })),
            )
                .into_response()
        }
        Err(e) => failed(e),
    }
}

/// `POST /api/v1/modules/install`: the request body is the package (a zip).
pub async fn install<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    package: Bytes,
) -> Response {
    install_bytes(&state, &package).await
}

#[derive(Deserialize)]
pub struct FromUrl {
    url: String,
    /// The SHA-256 the package must have.
    sha256: String,
}

/// `POST /api/v1/modules/install-url`: download a package, check its SHA-256, install it.
pub async fn install_url<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Json(body): Json<FromUrl>,
) -> Response {
    let url = body.url.clone();
    let bytes =
        match tokio::task::spawn_blocking(move || mythos_update::install::download_bytes(&url))
            .await
        {
            Ok(Ok(bytes)) => bytes,
            Ok(Err(e)) => return error(StatusCode::BAD_GATEWAY, e.to_string()),
            Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };
    let actual = hex::encode(<sha2::Sha256 as sha2::Digest>::digest(&bytes));
    if !actual.eq_ignore_ascii_case(body.sha256.trim()) {
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!(
                "the package at {} has SHA-256 {actual}, not {}; nothing was installed",
                body.url, body.sha256
            ),
        );
    }
    install_bytes(&state, &bytes).await
}

/// `DELETE /api/v1/modules/:module`: stop it, then remove its executable and UI. Only a module
/// installed from a package can be removed.
pub async fn remove<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(id): Path<String>,
) -> Response {
    let Some(settings) = state.modules.clone() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "this server does not manage modules",
        );
    };
    let Some(module) = find(&state, &id) else {
        return failed(ModuleError::NotFound(id));
    };
    if module.path.parent() != Some(settings.install_dir().as_path()) {
        return failed(ModuleError::Conflict(format!(
            "module '{}' is part of this MythOS installation ({}), not an installed package; disable it instead",
            id,
            module.path.display()
        )));
    }
    // Stop it first: Windows does not let a running executable be deleted.
    if let Err(e) = settings.set_enabled(&id, false) {
        return failed(e);
    }
    apply(&state).await;
    let manifest = module.manifest.clone();
    let path = module.path.clone();
    let settings_for_remove = Arc::clone(&settings);
    let removed =
        tokio::task::spawn_blocking(move || settings_for_remove.remove(&manifest, &path)).await;
    match removed {
        Ok(Ok(())) => {
            apply(&state).await;
            Json(json!({ "removed": id, "modules": rows(&state) })).into_response()
        }
        Ok(Err(e)) => failed(e),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
