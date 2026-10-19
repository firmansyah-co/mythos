//! MCP Tools for SCADA process variable and setpoint management.
//!
//! Exposes:
//! - `mythos_read_tag`: Reads live value, quality code, and timestamp.
//! - `mythos_write_tag`: Writes setpoint with safety checks, clamping, and audit logging.

use mythos_mcp_macros::mythos_mcp_tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::McpContext;
use crate::protocol::CallToolResult;
use crate::McpError;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReadTagArgs {
    /// Canonical tag URI (e.g. "external.BoilerRoom.PT101.pressure.pv")
    pub uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WriteTagArgs {
    /// Canonical tag URI (e.g. "external.BoilerRoom.SP101.pressure.sp")
    pub uri: String,
    /// Numerical or setpoint value to write
    pub value: f64,
    /// Mandatory reason string explaining why the AI agent is modifying plant state
    pub operator_intent: Option<String>,
}

/// Read real-time process variable or sensor telemetry from MythOS TagBus.
#[mythos_mcp_tool(
    name = "mythos_read_tag",
    category = "scada_telemetry",
    safety_level = "read_only"
)]
pub async fn mythos_read_tag(
    ctx: McpContext,
    args: ReadTagArgs,
) -> Result<CallToolResult, McpError> {
    let (value, quality, ts_ns) = ctx.execute_tag_read(&args.uri).await?;

    let catalog = ctx.tag_catalog.read().await;
    let meta = catalog.get_by_uri(&args.uri);

    let result_json = serde_json::json!({
        "uri": args.uri,
        "value": value,
        "quality": quality,
        "timestamp_ns": ts_ns,
        "slot_id": meta.as_ref().map(|m| m.slot_id),
        "engineering_units": meta.as_ref().map(|m| &m.engineering_units),
        "scale_min": meta.as_ref().map(|m| m.scale_min),
        "scale_max": meta.as_ref().map(|m| m.scale_max),
    });

    CallToolResult::ok_json(&result_json).map_err(|e| McpError::InternalError(e.to_string()))
}

/// Write a setpoint or command state into a field device tag with strict bounds checking and audit logging.
#[mythos_mcp_tool(
    name = "mythos_write_tag",
    category = "scada_control",
    safety_level = "safety_critical"
)]
pub async fn mythos_write_tag(
    ctx: McpContext,
    args: WriteTagArgs,
) -> Result<CallToolResult, McpError> {
    let intent = args
        .operator_intent
        .unwrap_or_else(|| "AI agent setpoint optimization".to_string());
    let (prev_val, new_val, audit_hash) = ctx
        .execute_tag_write(&args.uri, args.value, &intent)
        .await?;

    let result_json = serde_json::json!({
        "success": true,
        "uri": args.uri,
        "previous_value": prev_val,
        "new_value": new_val,
        "operator_intent": intent,
        "audit_hash": audit_hash,
    });

    CallToolResult::ok_json(&result_json).map_err(|e| McpError::InternalError(e.to_string()))
}
