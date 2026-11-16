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
    let fingerprint = node.fingerprint();
    json!({
        "id": identity.id,
        "name": identity.name,
        "role": identity.role,
        "site": identity.site,
        // What the node does, and whether that is the role's default or its own setting.
        "field_io": { "on": identity.field_io(), "from_role": identity.field_io.is_none() },
        "announce": { "on": identity.announce(), "from_role": identity.announce.is_none() },
        "field_io_off": identity.field_io_off_reason(),
        "http_on_network": identity.http_on_network,
        "version": env!("CARGO_PKG_VERSION"),
        "module_protocol": mythos_module::PROTOCOL_VERSION,
        // Where this run answers HTTP and HTTPS, and where the node now says to: they differ after
        // the role or `http_on_network` changed, until the server is started again.
        "listening": node.listening().map(|a| a.to_string()),
        "listen_address": node.http_ip().ok().map(|ip| ip.to_string()),
        "https": {
            "listening": node.listening_tls().map(|a| a.to_string()),
            "listen_address": node.listen_ip().ok().map(|ip| ip.to_string()),
            // What another node or an engineer compares before pairing (ADR-012 §2.3).
            "fingerprint": fingerprint,
            "fingerprint_spoken": fingerprint.as_deref().map(crate::tls::spoken),
        },
        "restart_required": node.restart_required(),
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
    let before = node.identity().field_io();
    match node.update(change) {
        Ok(after) => {
            // Field I/O follows at once: no restart is needed to stop or start reading devices.
            if after.field_io() != before {
                if let Some(io) = &state.io_manager {
                    io.set_field_io_off(after.field_io_off_reason());
                    io.reload().await;
                }
            }
            // The service modules learn the change (the discovery module announces the new name).
            if let Some(services) = &state.services {
                services.node_changed().await;
            }
            Json(view(&node)).into_response()
        }
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
