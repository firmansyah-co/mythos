//! # MythOS Model Context Protocol (MCP) Subsystem
//!
//! Exposes real-time SCADA state, tag topologies, and control tools to autonomous AI agents.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// MCP protocol and execution error definitions.
#[derive(Error, Debug)]
pub enum McpError {
    #[error("Tool execution failed: {0}")]
    ToolExecutionFailed(String),

    #[error("Tool '{0}' not found in registry")]
    ToolNotFound(String),

    #[error("Safety interlock violation: {0}")]
    SafetyInterlockViolation(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

/// Description of an MCP Tool parameter schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParameter {
    pub name: String,
    pub description: String,
    pub r#type: String,
    pub required: bool,
}

/// Manifest of an MCP Tool exposed to LLM agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolManifest {
    pub name: String,
    pub description: String,
    pub parameters: Vec<ToolParameter>,
}
