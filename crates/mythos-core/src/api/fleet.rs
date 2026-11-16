//! The fleet and call home (ADR-012 §2.6).
//!
//! | Route | Who | Does |
//! | :--- | :--- | :--- |
//! | `GET /api/v1/fleet` | viewer | every paired node: how it is reached, who it says it is, what it runs |
//! | `POST /api/v1/callhome` | a paired node, as itself | its long poll: brings an answer, takes the next request |

use crate::api::state::ApiState;
use crate::auth::Identity;
use crate::callhome::{self, JobAnswer};
use crate::pairing::{Peer, PeerView};
use axum::extract::{Extension, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use mythos_pal::PlatformShm;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

/// How long the fleet waits for one node before saying it cannot be reached.
const NODE_WAIT: Duration = Duration::from_secs(8);

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({ "error": message.into() }))).into_response()
}

/// One node of the fleet, asked now.
async fn member<S: PlatformShm + 'static>(state: &ApiState<S>, peer: Peer) -> Value {
    let call_home = state.call_home.as_deref();
    let via = callhome::via(call_home, &peer);
    let last_poll_s = call_home
        .and_then(|c| c.last_poll(&peer.id))
        .map(|d| d.as_secs());
    let asked = tokio::time::timeout(NODE_WAIT, async {
        let node = callhome::ask(call_home, &peer, "GET", "/api/v1/node", None).await?;
        if node.status != 200 {
            return Err(format!("it answered {}: {}", node.status, node.body));
        }
        let active = callhome::ask(call_home, &peer, "GET", "/api/v1/projects/active", None)
            .await
            .ok()
            .filter(|a| a.status == 200)
            .and_then(|a| a.body["project_id"].as_str().map(str::to_string));
        let running = match active {
            Some(project) => {
                let hash = callhome::ask(
                    call_home,
                    &peer,
                    "GET",
                    &format!("/api/v1/slices/{project}"),
                    None,
                )
                .await
                .ok()
                .filter(|a| a.status == 200)
                .and_then(|a| a.body["hash"].as_str().map(str::to_string));
                json!({ "project": project, "hash": hash })
            }
            None => Value::Null,
        };
        Ok::<_, String>((node.body, running))
    })
    .await
    .unwrap_or_else(|_| {
        Err(format!(
            "it did not answer within {} s",
            NODE_WAIT.as_secs()
        ))
    });
    let paired = PeerView::from(&peer);
    match asked {
        Ok((node, running)) => json!({
            "paired": paired,
            "via": via,
            "last_poll_s": last_poll_s,
            "reachable": true,
            "node": {
                "name": node["name"],
                "role": node["role"],
                "site": node["site"],
                "version": node["version"],
            },
            "running": running,
        }),
        Err(e) => json!({
            "paired": paired,
            "via": via,
            "last_poll_s": last_poll_s,
            "reachable": false,
            "error": e,
        }),
    }
}

/// `GET /api/v1/fleet`: every paired node, asked now, all at once.
pub async fn fleet<S: PlatformShm + 'static>(State(state): State<ApiState<S>>) -> Response {
    let Some(pairings) = &state.pairings else {
        return Json(json!({ "nodes": [], "note": "this server keeps no pairings" }))
            .into_response();
    };
    let asks = pairings
        .peers()
        .into_iter()
        .map(|peer| {
            let state = state.clone();
            async move { member(&state, peer).await }
        })
        .collect::<Vec<_>>();
    let mut tasks = Vec::new();
    for ask in asks {
        tasks.push(tokio::spawn(ask));
    }
    let mut nodes = Vec::new();
    for task in tasks {
        if let Ok(node) = task.await {
            nodes.push(node);
        }
    }
    Json(json!({ "nodes": nodes })).into_response()
}

#[derive(Deserialize)]
pub struct Poll {
    #[serde(default)]
    answer: Option<JobAnswer>,
}

/// `POST /api/v1/callhome`: a paired node calling home asks for its next request.
pub async fn poll<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    identity: Option<Extension<Identity>>,
    Json(poll): Json<Poll>,
) -> Response {
    let Some(call_home) = &state.call_home else {
        return error(StatusCode::NOT_FOUND, "this server keeps no pairings");
    };
    let Some(node) =
        identity.and_then(|Extension(i)| i.via.strip_prefix("pairing:").map(str::to_string))
    else {
        return error(
            StatusCode::FORBIDDEN,
            "only a paired node calls home, presenting its pairing token",
        );
    };
    match call_home.poll(&node, poll.answer).await {
        Some(job) => Json(json!({ "job": job })).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    }
}
