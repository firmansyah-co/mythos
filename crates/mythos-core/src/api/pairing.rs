//! Pairing routes (ADR-012 §2.3, `crate::pairing`).
//!
//! | Route | Who | What |
//! | :--- | :--- | :--- |
//! | `POST /api/v1/pairing/code` | administrator | make a one-use code granting a role |
//! | `POST /api/v1/pairing/accept` | anyone holding the code | another node pairs with this one |
//! | `GET /api/v1/pairings` | signed in | the paired nodes, no token |
//! | `POST /api/v1/pairings` | administrator | pair with another node, pinning its certificate |
//! | `GET /api/v1/pairings/:node/status` | signed in | ask the paired node, as this node |
//! | `DELETE /api/v1/pairings/:node` | administrator | unpair, and tell it |
//! | `DELETE /api/v1/pairing/peers/:node` | that node | it unpaired; forget it |

use super::ApiState;
use crate::auth::{Identity, Role};
use crate::pairing::{Acceptance, Accepted, Introduction, PairingError, Pairings, Peer, PeerView};
use crate::tls::Remote;
use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use mythos_pal::PlatformShm;
use serde::Deserialize;
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({ "error": message.into() }))).into_response()
}

/// Why a route cannot go on: its status and what to say.
type Refusal = (StatusCode, &'static str);

fn refuse((status, message): Refusal) -> Response {
    error(status, message)
}

fn parts<S: PlatformShm + 'static>(
    state: &ApiState<S>,
) -> Result<(Arc<Pairings>, Arc<crate::node::Node>), Refusal> {
    match (&state.pairings, &state.node) {
        (Some(pairings), Some(node)) => Ok((Arc::clone(pairings), Arc::clone(node))),
        _ => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "this server does not pair with other nodes",
        )),
    }
}

/// What this node says about itself when it pairs.
fn introduce(node: &crate::node::Node) -> Result<Introduction, Refusal> {
    let identity = node.identity();
    let fingerprint = node.fingerprint().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "this node answers no HTTPS, so it has no certificate to pair with",
    ))?;
    // A node that answers HTTPS on its own computer only cannot be reached: it says no port.
    let https_port = node
        .listening_tls()
        .filter(|a| !a.ip().is_loopback())
        .map(|a| a.port());
    Ok(Introduction {
        id: identity.id,
        name: identity.name,
        role: identity.role.as_str().to_string(),
        https_port,
        fingerprint,
    })
}

#[derive(Deserialize)]
pub struct CodeRequest {
    /// What the node that pairs with this code may do here; an engineer when absent.
    role: Option<Role>,
}

/// `POST /api/v1/pairing/code`
pub async fn make_code<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Json(body): Json<CodeRequest>,
) -> Response {
    let (pairings, _) = match parts(&state) {
        Ok(p) => p,
        Err(r) => return refuse(r),
    };
    let granted = body.role.unwrap_or(Role::Engineer);
    let (code, expires_unix_s) = pairings.make_code(granted);
    Json(json!({ "code": code, "expires_unix_s": expires_unix_s, "granted": granted }))
        .into_response()
}

/// `POST /api/v1/pairing/accept`: another node, holding a code made here, pairs with this one.
pub async fn accept<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    remote: Option<Extension<Remote>>,
    Json(request): Json<Acceptance>,
) -> Response {
    let (pairings, node) = match parts(&state) {
        Ok(p) => p,
        Err(r) => return refuse(r),
    };
    let me = match introduce(&node) {
        Ok(me) => me,
        Err(r) => return refuse(r),
    };
    // Where to reach it is where its connection came from, at the port it says it answers.
    let address = match (remote, request.from.https_port) {
        (Some(Extension(Remote(from))), Some(port)) => {
            Some(SocketAddr::new(from.ip(), port).to_string())
        }
        _ => None,
    };
    match pairings.accept(&request, address) {
        Ok((token, granted)) => {
            tracing::info!(
                "Paired with node '{}' ({}), granted {}",
                request.from.name,
                request.from.id,
                granted
            );
            Json(Accepted {
                node: me,
                token,
                granted,
            })
            .into_response()
        }
        Err(PairingError::BadCode) => {
            error(StatusCode::FORBIDDEN, PairingError::BadCode.to_string())
        }
        Err(e @ PairingError::Invalid(_)) => error(StatusCode::UNPROCESSABLE_ENTITY, e.to_string()),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

/// `GET /api/v1/pairings`
pub async fn list<S: PlatformShm + 'static>(State(state): State<ApiState<S>>) -> Response {
    let (pairings, _) = match parts(&state) {
        Ok(p) => p,
        Err(r) => return refuse(r),
    };
    let peers: Vec<PeerView> = pairings.peers().iter().map(PeerView::from).collect();
    Json(json!({ "peers": peers })).into_response()
}

#[derive(Deserialize)]
pub struct PairRequest {
    /// Where the other node answers HTTPS: `host:port`.
    address: String,
    /// The fingerprint its certificate must have, compared by the engineer with the one it shows.
    fingerprint: String,
    /// The pairing code made on it.
    code: String,
    /// What it may do here; a viewer when absent.
    grant: Option<Role>,
}

fn resolve(address: &str) -> Result<SocketAddr, String> {
    use std::net::ToSocketAddrs;
    address
        .to_socket_addrs()
        .map_err(|e| format!("'{address}' is not an address to reach: {e}"))?
        .next()
        .ok_or_else(|| format!("'{address}' names no address"))
}

/// `POST /api/v1/pairings`: pair with the node at `address`, which must present the certificate
/// whose fingerprint is `fingerprint`.
pub async fn pair<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Json(body): Json<PairRequest>,
) -> Response {
    let (pairings, node) = match parts(&state) {
        Ok(p) => p,
        Err(r) => return refuse(r),
    };
    let me = match introduce(&node) {
        Ok(me) => me,
        Err(r) => return refuse(r),
    };
    let fingerprint: String = body
        .fingerprint
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect::<String>()
        .to_ascii_lowercase();
    if fingerprint.len() != 64 {
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "a fingerprint is 64 hexadecimal characters (the SHA-256 the other node shows)",
        );
    }
    let target = match resolve(&body.address) {
        Ok(target) => target,
        Err(e) => return error(StatusCode::UNPROCESSABLE_ENTITY, e),
    };
    let token_for_it = crate::pairing::new_token();
    let request = Acceptance {
        code: body.code.clone(),
        from: me,
        token: token_for_it.clone(),
    };
    let pinned = fingerprint.clone();
    let answer = tokio::task::spawn_blocking(move || {
        crate::tls::request_pinned(
            target,
            &pinned,
            "POST",
            "/api/v1/pairing/accept",
            None,
            Some(&serde_json::to_value(&request).unwrap_or_default()),
        )
    })
    .await;
    let answer = match answer {
        Ok(Ok(answer)) => answer,
        Ok(Err(e)) => {
            return error(
                StatusCode::BAD_GATEWAY,
                format!("{} could not be paired with: {e}", body.address),
            )
        }
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    if answer.status != 200 {
        let said = answer.body["error"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| answer.body.to_string());
        let status = if answer.status == 403 {
            StatusCode::FORBIDDEN
        } else {
            StatusCode::BAD_GATEWAY
        };
        return error(status, format!("{} refused: {said}", body.address));
    }
    let accepted: Accepted = match serde_json::from_value(answer.body) {
        Ok(accepted) => accepted,
        Err(e) => {
            return error(
                StatusCode::BAD_GATEWAY,
                format!(
                    "{} answered something that is not a pairing: {e}",
                    body.address
                ),
            )
        }
    };
    if accepted.node.fingerprint != fingerprint {
        return error(
            StatusCode::BAD_GATEWAY,
            "the node says another certificate than the one it presented; nothing was paired",
        );
    }
    let peer = Peer {
        id: accepted.node.id.clone(),
        name: accepted.node.name.clone(),
        role: accepted.node.role.clone(),
        address: Some(body.address.clone()),
        fingerprint,
        granted: body.grant.unwrap_or(Role::Viewer),
        token_to_it: accepted.token,
        token_from_it_sha256: crate::auth::sha256_hex(&token_for_it),
        paired_unix_s: crate::auth::now_unix_s(),
        last_seen_unix_s: Some(crate::auth::now_unix_s()),
    };
    if let Err(e) = pairings.keep(peer.clone()) {
        return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    tracing::info!(
        "Paired with node '{}' ({}) at {}; it may act here as {}, this node there as {}",
        peer.name,
        peer.id,
        body.address,
        peer.granted,
        accepted.granted
    );
    (
        StatusCode::CREATED,
        Json(json!({ "peer": PeerView::from(&peer), "granted_there": accepted.granted })),
    )
        .into_response()
}

async fn ask(
    peer: &Peer,
    method: &'static str,
    path: String,
) -> Result<crate::tls::Answer, String> {
    let Some(address) = peer.address.clone() else {
        return Err(format!(
            "node '{}' cannot be reached: it answers on its own computer only",
            peer.name
        ));
    };
    let target = resolve(&address)?;
    let fingerprint = peer.fingerprint.clone();
    let token = peer.token_to_it.clone();
    tokio::task::spawn_blocking(move || {
        crate::tls::request_pinned(target, &fingerprint, method, &path, Some(&token), None)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// `GET /api/v1/pairings/:node/status`: ask the paired node who it is, as this node.
pub async fn status<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(id): Path<String>,
) -> Response {
    let (pairings, _) = match parts(&state) {
        Ok(p) => p,
        Err(r) => return refuse(r),
    };
    let Some(peer) = pairings.peer(&id) else {
        return error(StatusCode::NOT_FOUND, format!("no paired node '{id}'"));
    };
    match ask(&peer, "GET", "/api/v1/node".into()).await {
        Ok(answer) if answer.status == 200 => {
            Json(json!({ "reachable": true, "node": answer.body })).into_response()
        }
        Ok(answer) => Json(json!({
            "reachable": false,
            "error": format!("it answered {}: {}", answer.status, answer.body),
        }))
        .into_response(),
        Err(e) => Json(json!({ "reachable": false, "error": e })).into_response(),
    }
}

/// `DELETE /api/v1/pairings/:node`: forget it, and tell it to forget this node.
pub async fn unpair<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(id): Path<String>,
) -> Response {
    let (pairings, node) = match parts(&state) {
        Ok(p) => p,
        Err(r) => return refuse(r),
    };
    let peer = match pairings.forget(&id) {
        Ok(Some(peer)) => peer,
        Ok(None) => return error(StatusCode::NOT_FOUND, format!("no paired node '{id}'")),
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let me = node.identity().id;
    let told = ask(&peer, "DELETE", format!("/api/v1/pairing/peers/{me}")).await;
    let (told, told_error) = match told {
        Ok(answer) if answer.status == 200 || answer.status == 404 => (true, None),
        Ok(answer) => (false, Some(format!("it answered {}", answer.status))),
        Err(e) => (false, Some(e)),
    };
    Json(json!({
        "removed": peer.id,
        // Its token is no longer accepted here, whether or not it could be told.
        "told": told,
        "told_error": told_error,
    }))
    .into_response()
}

/// `DELETE /api/v1/pairing/peers/:node`: the paired node `node` unpaired; only it may say so.
pub async fn peer_left<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(id): Path<String>,
    identity: Option<Extension<Identity>>,
) -> Response {
    let (pairings, _) = match parts(&state) {
        Ok(p) => p,
        Err(r) => return refuse(r),
    };
    let is_it = identity.is_some_and(|Extension(i)| i.via == format!("pairing:{id}"));
    if !is_it {
        return error(
            StatusCode::FORBIDDEN,
            "only the paired node itself says it unpaired",
        );
    }
    match pairings.forget(&id) {
        Ok(_) => Json(json!({ "removed": id })).into_response(),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
