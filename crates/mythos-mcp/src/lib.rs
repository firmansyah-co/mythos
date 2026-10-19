//! # MythOS Model Context Protocol (MCP) Subsystem
//!
//! Exposes real-time SCADA state, tag topologies, and control tools to autonomous AI agents
//! adhering to the Model Context Protocol (MCP) 2024-11-05 standard (JSON-RPC 2.0).

extern crate self as mythos_mcp;

pub mod context;
pub mod protocol;
pub mod server;
pub mod tools;
pub mod transport;
pub use context::{CallerAuth, McpContext};
pub use protocol::{
    CallToolParams, CallToolResult, Content, Implementation, InitializeParams, InitializeResult,
    JsonRpcError, JsonRpcRequest, JsonRpcResponse, ListToolsParams, ListToolsResult, Notification,
    RequestId, ServerCapabilities, Tool, ToolParameter, ToolsCapability, MCP_PROTOCOL_VERSION,
};
pub use server::{FnToolHandler, McpServer, McpToolHandler};
pub use tools::register_all_tools;
pub use transport::{SseTransport, StdioTransport};

#[cfg(feature = "sse")]
pub use transport::axum_router::mcp_sse_router;

// Re-export procedural macro for ergonomic tool declaration
pub use mythos_mcp_macros::mythos_mcp_tool;

use thiserror::Error;

/// MCP protocol and execution error definitions.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum McpError {
    #[error("Tool execution failed: {0}")]
    ToolExecutionFailed(String),

    #[error("Tool '{0}' not found in registry")]
    ToolNotFound(String),

    #[error("Safety interlock violation: {0}")]
    SafetyInterlockViolation(String),

    #[error("Invalid arguments: {0}")]
    InvalidArguments(String),

    #[error("Internal error: {0}")]
    InternalError(String),
}
