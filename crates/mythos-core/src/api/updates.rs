//! Settings → Updates: the server's version and installation, the published releases, and
//! installing one (`crate::updates`).

use super::ApiState;
use crate::updates::{Updates, UpdatesError};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use mythos_pal::PlatformShm;
use mythos_update::config::ReleaseChannel;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({ "error": message.into() }))).into_response()
}

fn failed(e: UpdatesError) -> Response {
    let status = match e {
        UpdatesError::NoInstallation(_) => StatusCode::CONFLICT,
        UpdatesError::Refused(_) => StatusCode::CONFLICT,
        UpdatesError::Failed(_) => StatusCode::BAD_GATEWAY,
    };
    error(status, e.to_string())
}

// A handler returns the `Response` at once; boxing it only to unbox it on the next line gains nothing.
#[allow(clippy::result_large_err)]
fn updates_of<S: PlatformShm + 'static>(state: &ApiState<S>) -> Result<Arc<Updates>, Response> {
    state.updates.clone().ok_or_else(|| {
        error(
            StatusCode::SERVICE_UNAVAILABLE,
            "this server does not manage updates",
        )
    })
}

/// `GET /api/v1/updates`
pub async fn status<S: PlatformShm + 'static>(State(state): State<ApiState<S>>) -> Response {
    match updates_of(&state) {
        Ok(updates) => Json(updates.status()).into_response(),
        Err(r) => r,
    }
}

#[derive(Deserialize)]
pub struct ReleasesQuery {
    #[serde(default)]
    refresh: bool,
}

/// `GET /api/v1/updates/releases`: what the release feed publishes. A feed that cannot be read
/// is a 502 saying why, never a list.
pub async fn releases<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Query(query): Query<ReleasesQuery>,
) -> Response {
    let updates = match updates_of(&state) {
        Ok(u) => u,
        Err(r) => return r,
    };
    match updates.releases(query.refresh).await {
        Ok(list) => Json(list).into_response(),
        Err(e) => error(StatusCode::BAD_GATEWAY, e),
    }
}

#[derive(Deserialize)]
pub struct Follow {
    channel: ReleaseChannel,
    #[serde(default)]
    pinned_version: Option<String>,
}

/// `PUT /api/v1/updates/follow`: follow a channel, or stay on one version.
pub async fn follow<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Json(body): Json<Follow>,
) -> Response {
    let updates = match updates_of(&state) {
        Ok(u) => u,
        Err(r) => return r,
    };
    match updates.follow(body.channel, body.pinned_version.as_deref()) {
        Ok(status) => Json(status).into_response(),
        Err(e) => failed(e),
    }
}

#[derive(Deserialize, Default)]
pub struct InstallRequest {
    #[serde(default)]
    version: Option<String>,
}

/// `POST /api/v1/updates/install`: install a version (or what is followed) and make it current.
pub async fn install<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    body: Option<Json<InstallRequest>>,
) -> Response {
    let updates = match updates_of(&state) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let version = body.and_then(|Json(b)| b.version);
    match updates.install(version).await {
        Ok(done) => Json(done).into_response(),
        Err(e) => failed(e),
    }
}

/// `POST /api/v1/updates/rollback`
pub async fn rollback<S: PlatformShm + 'static>(State(state): State<ApiState<S>>) -> Response {
    let updates = match updates_of(&state) {
        Ok(u) => u,
        Err(r) => return r,
    };
    match updates.rollback().await {
        Ok(done) => Json(done).into_response(),
        Err(e) => failed(e),
    }
}
