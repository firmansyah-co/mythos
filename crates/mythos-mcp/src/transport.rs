//! # Model Context Protocol Transports
//!
//! Provides Stdio (standard input/output framed by `\n`) and Server-Sent Events (SSE) transports.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info};

use crate::server::McpServer;

/// Standard I/O transport executing JSON-RPC communication over stdin and stdout.
pub struct StdioTransport {
    buffer_capacity: usize,
}

impl Default for StdioTransport {
    fn default() -> Self {
        Self {
            buffer_capacity: 64 * 1024,
        }
    }
}

impl StdioTransport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_buffer_capacity(mut self, capacity: usize) -> Self {
        self.buffer_capacity = capacity;
        self
    }

    /// Run the stdio loop until stdin reaches EOF or an error occurs.
    pub async fn run(&self, server: Arc<McpServer>) -> std::io::Result<()> {
        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut reader = BufReader::with_capacity(self.buffer_capacity, stdin).lines();

        info!("MCP StdioTransport initialized, awaiting commands on stdin");

        while let Some(line) = reader.next_line().await? {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            debug!("MCP stdio incoming: {}", trimmed);
            if let Some(response_json) = server.handle_raw_json(trimmed).await {
                debug!("MCP stdio outgoing: {}", response_json);
                stdout.write_all(response_json.as_bytes()).await?;
                stdout.write_all(b"\n").await?;
                stdout.flush().await?;
            }
        }

        info!("MCP StdioTransport received EOF, shutting down cleanly");
        Ok(())
    }

    /// Process an async stream with custom reader and writer (ideal for testing).
    pub async fn process_streams<R, W>(
        &self,
        server: Arc<McpServer>,
        reader: R,
        mut writer: W,
    ) -> std::io::Result<()>
    where
        R: tokio::io::AsyncRead + Unpin,
        W: tokio::io::AsyncWrite + Unpin,
    {
        let mut lines = BufReader::new(reader).lines();
        while let Some(line) = lines.next_line().await? {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Some(response_json) = server.handle_raw_json(trimmed).await {
                writer.write_all(response_json.as_bytes()).await?;
                writer.write_all(b"\n").await?;
                writer.flush().await?;
            }
        }
        Ok(())
    }
}

/// Active Server-Sent Event (SSE) Session state.
pub struct SseSession {
    pub session_id: String,
    pub tx: mpsc::Sender<String>,
}

/// SSE Transport Manager for web/network-connected AI agents.
pub struct SseTransport {
    sessions: Arc<RwLock<HashMap<String, SseSession>>>,
    counter: AtomicU64,
}

impl Default for SseTransport {
    fn default() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            counter: AtomicU64::new(1),
        }
    }
}

impl SseTransport {
    pub fn new() -> Self {
        Self::default()
    }

    /// Generate a unique session ID.
    pub fn next_session_id(&self) -> String {
        let count = self.counter.fetch_add(1, Ordering::Relaxed);
        format!("mcp-session-{}-{}", std::process::id(), count)
    }

    /// Register a new active SSE session.
    pub async fn create_session(&self) -> (String, mpsc::Receiver<String>) {
        let session_id = self.next_session_id();
        let (tx, rx) = mpsc::channel(128);
        let mut sessions = self.sessions.write().await;
        sessions.insert(
            session_id.clone(),
            SseSession {
                session_id: session_id.clone(),
                tx,
            },
        );
        (session_id, rx)
    }

    /// Close and remove an active session.
    pub async fn close_session(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        sessions.remove(session_id);
    }

    /// Check if a session exists and is active.
    pub async fn has_session(&self, session_id: &str) -> bool {
        let sessions = self.sessions.read().await;
        sessions.contains_key(session_id)
    }

    /// Send a message to an active SSE session.
    pub async fn send_to_session(&self, session_id: &str, message: String) -> bool {
        let sessions = self.sessions.read().await;
        if let Some(session) = sessions.get(session_id) {
            session.tx.send(message).await.is_ok()
        } else {
            false
        }
    }

    /// Total active SSE connections.
    pub async fn active_sessions_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions.len()
    }
}

#[cfg(feature = "sse")]
pub mod axum_router {
    use super::*;
    use axum::extract::{Query, State};
    use axum::http::StatusCode;
    use axum::response::sse::{Event, KeepAlive, Sse};
    use axum::response::{IntoResponse, Response};
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use futures_util::stream::Stream;
    use std::convert::Infallible;
    use tokio_stream::wrappers::ReceiverStream;
    use tokio_stream::StreamExt;

    #[derive(Clone)]
    pub struct SseRouterState {
        pub server: Arc<McpServer>,
        pub transport: Arc<SseTransport>,
    }

    #[derive(serde::Deserialize)]
    pub struct MessageQuery {
        #[serde(rename = "sessionId")]
        pub session_id: Option<String>,
    }

    /// GET /mcp/sse handler initializing an SSE event stream.
    pub async fn sse_handler(
        State(state): State<SseRouterState>,
    ) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
        let (session_id, rx) = state.transport.create_session().await;
        let endpoint_event = Event::default()
            .event("endpoint")
            .data(format!("/mcp/message?sessionId={}", session_id));

        let stream =
            ReceiverStream::new(rx).map(|msg| Ok(Event::default().event("message").data(msg)));

        let initial_stream = tokio_stream::once(Ok(endpoint_event)).chain(stream);

        Sse::new(initial_stream).keep_alive(KeepAlive::default())
    }

    /// POST /mcp/message handler for sending JSON-RPC messages to the server.
    pub async fn message_handler(
        State(state): State<SseRouterState>,
        Query(query): Query<MessageQuery>,
        body: String,
    ) -> Response {
        let session_id = query.session_id.unwrap_or_default();
        if !state.transport.has_session(&session_id).await {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Invalid or expired sessionId" })),
            )
                .into_response();
        }

        if let Some(resp_json) = state.server.handle_raw_json(&body).await {
            // Also notify the SSE stream
            let _ = state
                .transport
                .send_to_session(&session_id, resp_json.clone())
                .await;
            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                resp_json,
            )
                .into_response()
        } else {
            StatusCode::ACCEPTED.into_response()
        }
    }

    /// Build an Axum router for MCP SSE and Message endpoints.
    pub fn mcp_sse_router(server: Arc<McpServer>, transport: Arc<SseTransport>) -> Router {
        let state = SseRouterState { server, transport };
        Router::new()
            .route("/mcp/sse", get(sse_handler))
            .route("/mcp/message", post(message_handler))
            .with_state(state)
    }
}
