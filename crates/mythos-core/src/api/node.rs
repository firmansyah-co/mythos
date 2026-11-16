//! `/api/v1/node`: this installation as a node (ADR-012 §2.1). Anyone signed in reads it; an
//! administrator changes its name, site and role.

use super::ApiState;
use crate::node::{Node, NodeChange, NodeError};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use mythos_pal::PlatformShm;
use serde_json::{json, Value};
use std::sync::Arc;

fn view(node: &Node) -> Value {
    let identity = node.identity();
    let wanted = node.listen_ip().ok();
    let listening = node.listening();
    json!({
        "id": identity.id,
        "name": identity.name,
        "role": identity.role,
        "site": identity.site,
        "version": env!("CARGO_PKG_VERSION"),
        "module_protocol": mythos_module::PROTOCOL_VERSION,
        // Where this run listens, and where the role (or MYTHOS_BIND) says to: they differ after
        // the role changed, until the server is started again.
        "listening": listening.map(|a| a.to_string()),
        "listen_address": wanted.map(|ip| ip.to_string()),
        "restart_required": match (listening, wanted) {
            (Some(now), Some(ip)) => now.ip() != ip,
            _ => false,
        },
    })
}

fn no_node() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": "this server has no node identity" })),
    )
        .into_response()
}

fn node_of<S: PlatformShm + 'static>(state: &ApiState<S>) -> Option<Arc<Node>> {
    state.node.clone()
}

/// `GET /api/v1/node`
pub async fn get<S: PlatformShm + 'static>(State(state): State<ApiState<S>>) -> Response {
    match node_of(&state) {
        Some(node) => Json(view(&node)).into_response(),
        None => no_node(),
    }
}

/// `PUT /api/v1/node`: `{"name"?, "site"?, "role"?}`
pub async fn update<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Json(change): Json<NodeChange>,
) -> Response {
    let Some(node) = node_of(&state) else {
        return no_node();
    };
    match node.update(change) {
        Ok(_) => Json(view(&node)).into_response(),
        Err(NodeError::Invalid(message)) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": message })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}
