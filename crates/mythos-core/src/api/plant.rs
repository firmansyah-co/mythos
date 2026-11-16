//! Plant projects, download and compare (ADR-012 §2.4, §2.5; `crate::plant`).
//!
//! On the node that engineers the plant:
//!
//! | Route | Who |
//! | :--- | :--- |
//! | `GET /api/v1/projects/:id/plant` | signed in |
//! | `PUT /api/v1/projects/:id/plant/devices/:device` (name, node) | engineer |
//! | `DELETE /api/v1/projects/:id/plant/devices/:device` | engineer |
//! | `GET /api/v1/projects/:id/plant/devices/:device/compare` | signed in |
//! | `POST /api/v1/projects/:id/plant/devices/:device/download` (confirm) | engineer |
//! | `POST /api/v1/projects/:id/plant/download` (confirm): every assigned device | engineer |
//!
//! On the node a device is downloaded to, called by the engineering node over its pairing:
//! `PUT /api/v1/slices/:project` installs and runs a slice, `GET /api/v1/slices/:project` says
//! the hash of what it holds.

use super::ApiState;
use crate::pairing::Peer;
use crate::plant::{self, Device, PlantError, Slice};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use mythos_pal::PlatformShm;
use serde::Deserialize;
use serde_json::{json, Value};

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({ "error": message.into() }))).into_response()
}

fn failed(e: PlantError) -> Response {
    let status = match e {
        PlantError::Invalid(_) => StatusCode::UNPROCESSABLE_ENTITY,
        PlantError::NotFound(_) => StatusCode::NOT_FOUND,
        PlantError::Storage(_) => StatusCode::NOT_FOUND,
        PlantError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    error(status, e.to_string())
}

fn own_id<S: PlatformShm + 'static>(state: &ApiState<S>) -> String {
    state
        .node
        .as_ref()
        .map(|n| n.identity().id)
        .unwrap_or_default()
}

/// A device as shown: with the name of the node it is assigned to, when it is known.
fn shown<S: PlatformShm + 'static>(state: &ApiState<S>, device: &Device) -> Value {
    let own = own_id(state);
    let node = device.node.as_deref().map(|id| {
        if id == own {
            json!({ "id": id, "name": state.node.as_ref().map(|n| n.identity().name), "this": true })
        } else {
            let peer = state.pairings.as_ref().and_then(|p| p.peer(id));
            json!({ "id": id, "name": peer.as_ref().map(|p| p.name.clone()), "paired": peer.is_some() })
        }
    });
    json!({ "id": device.id, "name": device.name, "project": device.project, "node": node })
}

/// `GET /api/v1/projects/:id/plant`
pub async fn get<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(project): Path<String>,
) -> Response {
    match plant::read(&state.storage, &project, &own_id(&state)) {
        Ok((plant, implicit)) => Json(json!({
            "implicit": implicit,
            "devices": plant.devices.iter().map(|d| shown(&state, d)).collect::<Vec<_>>(),
        }))
        .into_response(),
        Err(e) => failed(e),
    }
}

#[derive(Deserialize)]
pub struct DeviceChange {
    name: String,
    /// The node it is assigned to: this node's id, a paired node's, or `null`.
    #[serde(default)]
    node: Option<String>,
}

/// `PUT /api/v1/projects/:id/plant/devices/:device`
pub async fn set_device<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path((project, device)): Path<(String, String)>,
    Json(change): Json<DeviceChange>,
) -> Response {
    let own = own_id(&state);
    // A device is assigned to this node or a paired one, never to a node merely heard of.
    if let Some(node) = change.node.as_deref() {
        let paired = state
            .pairings
            .as_ref()
            .is_some_and(|p| p.peer(node).is_some());
        if node != own && !paired {
            return error(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!(
                    "node '{node}' is not paired with this one: pair it first (Settings → Pairing)"
                ),
            );
        }
    }
    match plant::set_device(
        &state.storage,
        &project,
        &own,
        &device,
        &change.name,
        change.node,
    ) {
        Ok(device) => Json(shown(&state, &device)).into_response(),
        Err(e) => failed(e),
    }
}

/// `DELETE /api/v1/projects/:id/plant/devices/:device`: its project is kept.
pub async fn remove_device<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path((project, device)): Path<(String, String)>,
) -> Response {
    match plant::remove_device(&state.storage, &project, &own_id(&state), &device) {
        Ok(device) => {
            Json(json!({ "removed": device.id, "project_kept": device.project })).into_response()
        }
        Err(e) => failed(e),
    }
}

fn find_device<S: PlatformShm + 'static>(
    state: &ApiState<S>,
    project: &str,
    device: &str,
) -> Result<Device, PlantError> {
    let (plant, _) = plant::read(&state.storage, project, &own_id(state))?;
    plant
        .devices
        .into_iter()
        .find(|d| d.id == device)
        .ok_or_else(|| PlantError::NotFound(format!("no device '{device}' in this plant")))
}

/// Where a device goes: this node, or a paired one.
enum Target {
    Here,
    Peer(Peer),
}

fn target_of<S: PlatformShm + 'static>(
    state: &ApiState<S>,
    device: &Device,
) -> Result<Target, String> {
    let Some(node) = device.node.as_deref() else {
        return Err(format!("device '{}' is assigned to no node", device.name));
    };
    if node == own_id(state) {
        return Ok(Target::Here);
    }
    state
        .pairings
        .as_ref()
        .and_then(|p| p.peer(node))
        .map(Target::Peer)
        .ok_or_else(|| {
            format!(
                "node '{node}' of device '{}' is no longer paired",
                device.name
            )
        })
}

async fn ask(
    peer: &Peer,
    method: &'static str,
    path: String,
    body: Option<Value>,
) -> Result<crate::tls::Answer, String> {
    use std::net::ToSocketAddrs;
    let address = peer.address.clone().ok_or_else(|| {
        format!(
            "node '{}' cannot be reached: it answers on its own computer only",
            peer.name
        )
    })?;
    let target = address
        .to_socket_addrs()
        .map_err(|e| format!("{address}: {e}"))?
        .next()
        .ok_or_else(|| format!("'{address}' names no address"))?;
    let fingerprint = peer.fingerprint.clone();
    let token = peer.token_to_it.clone();
    tokio::task::spawn_blocking(move || {
        crate::tls::request_pinned(
            target,
            &fingerprint,
            method,
            &path,
            Some(&token),
            body.as_ref(),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

/// What compare says of one device.
async fn compare_one<S: PlatformShm + 'static>(
    state: &ApiState<S>,
    plant_id: &str,
    device: &Device,
) -> Value {
    let local = match plant::pack_device(&state.storage, plant_id, device) {
        Ok(slice) => slice.hash,
        Err(e) => {
            return json!({ "device": device.id, "state": "unreadable", "error": e.to_string() })
        }
    };
    let target = match target_of(state, device) {
        Ok(target) => target,
        Err(e) => {
            return json!({ "device": device.id, "state": "not_assigned", "error": e, "project_hash": local })
        }
    };
    match target {
        // This node runs its own project files: the same by definition, but say whether it runs
        // this project at all.
        Target::Here => {
            let active = state.storage.get_active_project().ok().flatten();
            let runs = active.as_deref() == Some(device.project.as_str());
            json!({ "device": device.id, "state": if runs { "same" } else { "not_running" }, "project_hash": local, "node_hash": local })
        }
        Target::Peer(peer) => match ask(
            &peer,
            "GET",
            format!("/api/v1/slices/{}", device.project),
            None,
        )
        .await
        {
            Ok(answer) if answer.status == 200 => {
                let there = answer.body["hash"].as_str().unwrap_or_default().to_string();
                let runs = answer.body["active"].as_bool().unwrap_or(false);
                let state_word = if there != local {
                    "differs"
                } else if runs {
                    "same"
                } else {
                    "not_running"
                };
                json!({ "device": device.id, "state": state_word, "project_hash": local, "node_hash": there })
            }
            Ok(answer) if answer.status == 404 => {
                json!({ "device": device.id, "state": "never_downloaded", "project_hash": local })
            }
            Ok(answer) => {
                json!({ "device": device.id, "state": "unreachable", "error": format!("it answered {}: {}", answer.status, answer.body), "project_hash": local })
            }
            Err(e) => {
                json!({ "device": device.id, "state": "unreachable", "error": e, "project_hash": local })
            }
        },
    }
}

/// `GET /api/v1/projects/:id/plant/devices/:device/compare`
pub async fn compare<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path((project, device)): Path<(String, String)>,
) -> Response {
    match find_device(&state, &project, &device) {
        Ok(device) => Json(compare_one(&state, &project, &device).await).into_response(),
        Err(e) => failed(e),
    }
}

#[derive(Deserialize)]
pub struct Confirm {
    #[serde(default)]
    confirm: bool,
}

const UNCONFIRMED: &str =
    "a download changes what a node runs: send {\"confirm\": true} to say it is meant";

/// Make `project` what this node runs: activate it and apply its I/O.
async fn run_here<S: PlatformShm + 'static>(
    state: &ApiState<S>,
    project: &str,
) -> Result<(), String> {
    state
        .storage
        .activate_project(project)
        .map_err(|e| e.to_string())?;
    if let Some(io) = &state.io_manager {
        io.reload().await;
    }
    Ok(())
}

/// Download one device: what it reports.
async fn download_one<S: PlatformShm + 'static>(
    state: &ApiState<S>,
    plant_id: &str,
    device: &Device,
) -> Value {
    let target = match target_of(state, device) {
        Ok(target) => target,
        Err(e) => return json!({ "device": device.id, "state": "refused", "error": e }),
    };
    let slice = match plant::pack_device(&state.storage, plant_id, device) {
        Ok(slice) => slice,
        Err(e) => return json!({ "device": device.id, "state": "refused", "error": e.to_string() }),
    };
    match target {
        Target::Here => match run_here(state, &device.project).await {
            Ok(()) => {
                json!({ "device": device.id, "state": "done", "hash": slice.hash, "node": "this" })
            }
            Err(e) => json!({ "device": device.id, "state": "refused", "error": e }),
        },
        Target::Peer(peer) => {
            let body = json!({ "slice": slice, "confirm": true });
            match ask(
                &peer,
                "PUT",
                format!("/api/v1/slices/{}", device.project),
                Some(body),
            )
            .await
            {
                Ok(answer) if answer.status == 200 => {
                    let there = answer.body["hash"].as_str().unwrap_or_default();
                    if there == slice.hash {
                        json!({ "device": device.id, "state": "done", "hash": there, "node": peer.name })
                    } else {
                        json!({ "device": device.id, "state": "refused", "error": format!("{} holds {there}, not the {} sent", peer.name, slice.hash) })
                    }
                }
                Ok(answer) => json!({
                    "device": device.id,
                    "state": "refused",
                    "error": answer.body["error"].as_str().map(str::to_string).unwrap_or_else(|| format!("it answered {}", answer.status)),
                }),
                Err(e) => json!({ "device": device.id, "state": "unreachable", "error": e }),
            }
        }
    }
}

/// `POST /api/v1/projects/:id/plant/devices/:device/download`
pub async fn download<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path((project, device)): Path<(String, String)>,
    Json(confirm): Json<Confirm>,
) -> Response {
    if !confirm.confirm {
        return error(StatusCode::PRECONDITION_REQUIRED, UNCONFIRMED);
    }
    let device = match find_device(&state, &project, &device) {
        Ok(device) => device,
        Err(e) => return failed(e),
    };
    let result = download_one(&state, &project, &device).await;
    let status = if result["state"] == "done" {
        StatusCode::OK
    } else {
        StatusCode::BAD_GATEWAY
    };
    (status, Json(result)).into_response()
}

/// `POST /api/v1/projects/:id/plant/download`: every assigned device, each reported.
pub async fn download_all<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(project): Path<String>,
    Json(confirm): Json<Confirm>,
) -> Response {
    if !confirm.confirm {
        return error(StatusCode::PRECONDITION_REQUIRED, UNCONFIRMED);
    }
    let (plant, _) = match plant::read(&state.storage, &project, &own_id(&state)) {
        Ok(p) => p,
        Err(e) => return failed(e),
    };
    let mut results = Vec::new();
    for device in &plant.devices {
        if device.node.is_none() {
            results.push(
                json!({ "device": device.id, "state": "skipped", "error": "assigned to no node" }),
            );
            continue;
        }
        results.push(download_one(&state, &project, device).await);
    }
    let all_done = results
        .iter()
        .all(|r| r["state"] == "done" || r["state"] == "skipped");
    Json(json!({ "complete": all_done, "devices": results })).into_response()
}

#[derive(Deserialize)]
pub struct SliceUpload {
    slice: Slice,
    #[serde(default)]
    confirm: bool,
}

/// `PUT /api/v1/slices/:project`: install `slice` as `project` and run it; on failure, run what
/// ran before.
pub async fn put_slice<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(project): Path<String>,
    Json(upload): Json<SliceUpload>,
) -> Response {
    if !upload.confirm {
        return error(StatusCode::PRECONDITION_REQUIRED, UNCONFIRMED);
    }
    let was_active = state.storage.get_active_project().ok().flatten();
    let previous = match plant::install(&state.storage, &project, &upload.slice) {
        Ok(previous) => previous,
        Err(e) => return failed(e),
    };
    if let Err(e) = run_here(&state, &project).await {
        // Back to what ran before, as it ran.
        let _ = plant::restore(&state.storage, &project, &previous);
        if let Some(active) = was_active {
            let _ = run_here(&state, &active).await;
        }
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!(
                "the slice was installed but could not run, so the previous one runs again: {e}"
            ),
        );
    }
    let hash = plant::hash_here(&state.storage, &project).ok().flatten();
    tracing::info!("Downloaded project '{project}' and made it current ({hash:?})");
    Json(json!({
        "project": project,
        "hash": hash,
        "previous_kept": previous.dir.is_some(),
        "io": state.io_status.snapshot(),
    }))
    .into_response()
}

/// `GET /api/v1/slices/:project`: the hash of what this node holds for `project`.
pub async fn get_slice<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(project): Path<String>,
) -> Response {
    match plant::hash_here(&state.storage, &project) {
        Ok(Some(hash)) => {
            let active = state.storage.get_active_project().ok().flatten();
            Json(json!({ "project": project, "hash": hash, "active": active.as_deref() == Some(project.as_str()) })).into_response()
        }
        Ok(None) => error(
            StatusCode::NOT_FOUND,
            format!("this node holds no project '{project}'"),
        ),
        Err(e) => failed(e),
    }
}
