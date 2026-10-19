use mythos_core::tag::TagCatalog;
use mythos_mcp::{McpContext, McpServer, SseTransport};
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::test]
async fn test_sse_session_lifecycle() {
    let transport = SseTransport::new();
    assert_eq!(transport.active_sessions_count().await, 0);

    let (sess1, mut rx1) = transport.create_session().await;
    assert!(sess1.starts_with("mcp-session-"));
    assert_eq!(transport.active_sessions_count().await, 1);
    assert!(transport.has_session(&sess1).await);

    // Send message to sess1
    let sent = transport
        .send_to_session(
            &sess1,
            "{\"jsonrpc\":\"2.0\",\"result\":\"ok\"}".to_string(),
        )
        .await;
    assert!(sent);

    let msg = rx1.recv().await.unwrap();
    assert_eq!(msg, "{\"jsonrpc\":\"2.0\",\"result\":\"ok\"}");

    // Close sess1
    transport.close_session(&sess1).await;
    assert_eq!(transport.active_sessions_count().await, 0);
    assert!(!transport.has_session(&sess1).await);
}

#[tokio::test]
async fn test_sse_axum_routes() {
    let catalog = Arc::new(RwLock::new(TagCatalog::new()));
    let ctx = McpContext::new(catalog);
    let server = Arc::new(McpServer::new(ctx));
    let transport = Arc::new(SseTransport::new());

    let _router = mythos_mcp::mcp_sse_router(server, transport.clone());

    // Verify router exists and can be composed
    assert_eq!(transport.active_sessions_count().await, 0);
}
