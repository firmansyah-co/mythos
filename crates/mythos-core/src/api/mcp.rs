//! Model Context Protocol (MCP) SSE and JSON-RPC Transport Endpoints.
//!
//! Complies with TSK-501, TSK-504, SYS-REQ-070, SYS-REQ-073, and ADR-005.
//! Exposes:
//! - `GET /api/v1/mcp/sse`: Establishes SSE stream and issues session endpoint.
//! - `POST /api/v1/mcp/message`: Receives JSON-RPC requests with Ed25519 Bearer token verification.

use crate::api::state::ApiState;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use mythos_pal::PlatformShm;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;
use tokio_stream::StreamExt;

/// Active SSE connection session state.
pub struct McpSseSession {
    pub session_id: String,
    pub tx: mpsc::Sender<String>,
}

/// Transport session manager for MCP SSE endpoints.
pub struct McpSseTransport {
    sessions: Arc<RwLock<HashMap<String, McpSseSession>>>,
    counter: AtomicU64,
}

impl Default for McpSseTransport {
    fn default() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            counter: AtomicU64::new(1),
        }
    }
}

impl McpSseTransport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn next_session_id(&self) -> String {
        let count = self.counter.fetch_add(1, Ordering::Relaxed);
        format!("mcp-session-{}-{}", std::process::id(), count)
    }

    pub async fn create_session(&self) -> (String, mpsc::Receiver<String>) {
        let session_id = self.next_session_id();
        let (tx, rx) = mpsc::channel(128);
        let mut sessions = self.sessions.write().await;
        sessions.insert(
            session_id.clone(),
            McpSseSession {
                session_id: session_id.clone(),
                tx,
            },
        );
        (session_id, rx)
    }

    pub async fn close_session(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        sessions.remove(session_id);
    }

    pub async fn has_session(&self, session_id: &str) -> bool {
        let sessions = self.sessions.read().await;
        sessions.contains_key(session_id)
    }

    pub async fn send_to_session(&self, session_id: &str, message: String) -> bool {
        let sessions = self.sessions.read().await;
        if let Some(session) = sessions.get(session_id) {
            session.tx.send(message).await.is_ok()
        } else {
            false
        }
    }

    pub async fn active_sessions_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions.len()
    }
}

#[derive(Debug, Deserialize)]
pub struct McpMessageQuery {
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
}

/// GET /api/v1/mcp/sse
///
/// Establishes Server-Sent Events stream for AI agents and announces message endpoint.
pub async fn mcp_sse_handler<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (session_id, rx) = state.mcp_transport.create_session().await;
    let endpoint_event = Event::default()
        .event("endpoint")
        .data(format!("/api/v1/mcp/message?sessionId={}", session_id));

    let stream = ReceiverStream::new(rx).map(|msg| Ok(Event::default().event("message").data(msg)));

    let initial_stream = tokio_stream::once(Ok(endpoint_event)).chain(stream);

    Sse::new(initial_stream).keep_alive(KeepAlive::default())
}

/// POST /api/v1/mcp/message
///
/// Receives JSON-RPC messages from AI agents with Ed25519 cryptographic token verification.
pub async fn mcp_message_handler<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Query(query): Query<McpMessageQuery>,
    headers: HeaderMap,
    body: String,
) -> Response {
    let session_id = query.session_id.unwrap_or_default();
    if !state.mcp_transport.has_session(&session_id).await {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({
                "jsonrpc": "2.0",
                "error": { "code": -32001, "message": "Invalid or expired sessionId" },
                "id": null
            })),
        )
            .into_response();
    }

    // Cryptographic Authentication Gate (TSK-504, SYS-REQ-073)
    if let Some(token_mgr) = &state.token_manager {
        let auth_header = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok());

        let token_str = match auth_header {
            Some(val) if val.starts_with("Bearer ") => &val[7..],
            _ => {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({
                        "jsonrpc": "2.0",
                        "error": {
                            "code": -32002,
                            "message": "Missing or malformed Authorization header. Expected 'Bearer <ed25519_token>'"
                        },
                        "id": null
                    })),
                )
                    .into_response();
            }
        };

        if let Err(auth_err) = token_mgr.verify_token(token_str) {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({
                    "jsonrpc": "2.0",
                    "error": {
                        "code": -32003,
                        "message": format!("Ed25519 token verification rejected: {}", auth_err)
                    },
                    "id": null
                })),
            )
                .into_response();
        }
    }

    // Process JSON-RPC request
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return StatusCode::ACCEPTED.into_response();
    }

    let parsed: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "jsonrpc": "2.0",
                    "error": { "code": -32700, "message": format!("JSON parse error: {}", e) },
                    "id": null
                })),
            )
                .into_response();
        }
    };

    let req_id = parsed.get("id").cloned();
    let method = parsed.get("method").and_then(|m| m.as_str()).unwrap_or("");

    let resp_payload = match method {
        "initialize" => {
            json!({
                "jsonrpc": "2.0",
                "id": req_id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": { "listChanged": true },
                        "logging": {}
                    },
                    "serverInfo": {
                        "name": "mythos-microkernel-mcp",
                        "version": env!("CARGO_PKG_VERSION")
                    },
                    "instructions": "MythOS Native MCP Endpoint"
                }
            })
        }
        "ping" => {
            json!({
                "jsonrpc": "2.0",
                "id": req_id,
                "result": {}
            })
        }
        "notifications/initialized" => {
            return StatusCode::ACCEPTED.into_response();
        }
        "tools/list" => {
            let catalog = state.tag_catalog.read().await;
            let tag_count = catalog.iter().count();
            json!({
                "jsonrpc": "2.0",
                "id": req_id,
                "result": {
                    "tools": [
                        {
                            "name": "mythos_read_tag",
                            "description": "Read live process telemetry from TagBus",
                            "inputSchema": {
                                "type": "object",
                                "properties": { "uri": { "type": "string" } },
                                "required": ["uri"]
                            }
                        },
                        {
                            "name": "mythos_write_tag",
                            "description": "Write setpoint to tag with safety interlocks and audit ledger",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "uri": { "type": "string" },
                                    "value": { "type": "number" },
                                    "operator_intent": { "type": "string" }
                                },
                                "required": ["uri", "value"]
                            }
                        }
                    ],
                    "metadata": { "registered_tags": tag_count }
                }
            })
        }
        "tools/call" => {
            let tool_name = parsed
                .get("params")
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("");

            match tool_name {
                "mythos_read_tag" => {
                    let uri = parsed
                        .get("params")
                        .and_then(|p| p.get("arguments"))
                        .and_then(|a| a.get("uri"))
                        .and_then(|u| u.as_str())
                        .unwrap_or("");

                    let catalog = state.tag_catalog.read().await;
                    match catalog.get_by_uri(uri) {
                        Some(meta) => json!({
                            "jsonrpc": "2.0",
                            "id": req_id,
                            "result": {
                                "content": [{
                                    "type": "text",
                                    "text": format!("Tag '{}' [slot={}] units={}", meta.uri, meta.slot_id, meta.engineering_units)
                                }]
                            }
                        }),
                        None => json!({
                            "jsonrpc": "2.0",
                            "id": req_id,
                            "result": {
                                "content": [{
                                    "type": "text",
                                    "text": format!("Tag '{}' not found", uri)
                                }],
                                "isError": true
                            }
                        }),
                    }
                }
                other => json!({
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "error": {
                        "code": -32601,
                        "message": format!("Tool '{}' not found", other)
                    }
                }),
            }
        }
        other => {
            json!({
                "jsonrpc": "2.0",
                "id": req_id,
                "error": {
                    "code": -32601,
                    "message": format!("Method '{}' not found", other)
                }
            })
        }
    };

    let resp_str = serde_json::to_string(&resp_payload).unwrap_or_default();
    let _ = state
        .mcp_transport
        .send_to_session(&session_id, resp_str.clone())
        .await;

    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        resp_str,
    )
        .into_response()
}
