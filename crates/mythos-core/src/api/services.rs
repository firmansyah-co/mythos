//! Requests to a service module's routes (ADR-011 §2.2), forwarded by `crate::services`.
//!
//! Two routes take every module route: `/api/v1/projects/:id/:module/*rest` for a project's data
//! and `/api/v1/m/:module/*rest` for the rest. The accounts middleware lets any signed-in
//! account through to here; the role each route needs is the one its module declares, and is
//! checked in `crate::services::forward` before the module sees the request.

use super::ApiState;
use crate::auth::Identity;
use crate::services::{forward, Forward};
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use mythos_module::protocol::{HttpReply, RouteScope};
use mythos_pal::PlatformShm;
use serde_json::json;
use std::collections::BTreeMap;

fn reply(r: HttpReply) -> Response {
    let status = StatusCode::from_u16(r.status).unwrap_or(StatusCode::BAD_GATEWAY);
    (status, Json(r.body)).into_response()
}

fn error(status: StatusCode, message: String) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

// A handler returns the `Response` at once; boxing it only to unbox it on the next line gains nothing.
#[allow(clippy::result_large_err)]
fn body_of(bytes: &Bytes) -> Result<Option<serde_json::Value>, Response> {
    if bytes.is_empty() {
        return Ok(None);
    }
    serde_json::from_slice(bytes).map(Some).map_err(|e| {
        error(
            StatusCode::BAD_REQUEST,
            format!("the body is not JSON: {e}"),
        )
    })
}

/// `/api/v1/projects/:id/:module/*rest`
pub async fn project_route<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path((id, module, rest)): Path<(String, String, String)>,
    method: Method,
    Query(query): Query<BTreeMap<String, String>>,
    identity: Option<Extension<Identity>>,
    bytes: Bytes,
) -> Response {
    let Some(service) = state.services.as_ref().and_then(|s| s.get(&module)) else {
        return error(
            StatusCode::NOT_FOUND,
            format!("no module serves '{module}'"),
        );
    };
    let dir = match state.storage.project_dir(&id) {
        Ok(dir) if dir.is_dir() => dir,
        Ok(_) => return error(StatusCode::NOT_FOUND, format!("there is no project '{id}'")),
        Err(e) => return error(StatusCode::BAD_REQUEST, e.to_string()),
    };
    let body = match body_of(&bytes) {
        Ok(b) => b,
        Err(r) => return r,
    };
    reply(
        forward(
            &service,
            Forward {
                scope: RouteScope::Project,
                method: method.to_string(),
                rest: format!("/{rest}"),
                query,
                body,
                identity: identity.map(|Extension(i)| i),
                project: Some((id, dir)),
            },
        )
        .await,
    )
}

/// `/api/v1/m/:module/*rest`
pub async fn global_route<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path((module, rest)): Path<(String, String)>,
    method: Method,
    Query(query): Query<BTreeMap<String, String>>,
    identity: Option<Extension<Identity>>,
    bytes: Bytes,
) -> Response {
    let Some(service) = state.services.as_ref().and_then(|s| s.get(&module)) else {
        return error(
            StatusCode::NOT_FOUND,
            format!("no module serves '{module}'"),
        );
    };
    let body = match body_of(&bytes) {
        Ok(b) => b,
        Err(r) => return r,
    };
    reply(
        forward(
            &service,
            Forward {
                scope: RouteScope::Global,
                method: method.to_string(),
                rest: format!("/{rest}"),
                query,
                body,
                identity: identity.map(|Extension(i)| i),
                project: None,
            },
        )
        .await,
    )
}
