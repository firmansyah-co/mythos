//! `GET /api/v1/remote`: the plant's other devices this node reads tags from, and whether each
//! link is up (ADR-012 §2.6).

use crate::api::state::ApiState;
use axum::extract::State;
use axum::Json;
use mythos_pal::PlatformShm;
use serde_json::{json, Value};

pub async fn links<S: PlatformShm + 'static>(State(state): State<ApiState<S>>) -> Json<Value> {
    let Some(remote) = &state.remote else {
        return Json(json!({ "devices": [], "note": "this server keeps no pairings" }));
    };
    let devices = remote.devices(|| crate::remote::devices_of_running_plant(&state));
    Json(json!({ "devices": remote.links(&devices) }))
}
