//! Procedural Macro #[mythos_mcp_tool] Verification Test Suite.
//!
//! Validates:
//! 1. Generated Tool schema (name, description, parameter JSON schema).
//! 2. Tool invocation through generated macro wrapper struct and register hook.
//! 3. Strongly-typed deserialization of input arguments.

use mythos_core::tag::TagCatalog;
use mythos_mcp::protocol::*;
use mythos_mcp::{mythos_mcp_tool, CallToolResult, McpContext, McpError, McpServer};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Typed arguments for custom test tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct CustomSetpointArgs {
    /// Identifier of the target loop
    pub loop_id: String,
    /// Target setpoint temperature in degrees Celsius
    pub setpoint_celsius: f64,
    /// Enable PID auto-tuning mode
    pub auto_tune: bool,
}

/// Custom operator action tool declared via procedural macro
#[mythos_mcp_tool(
    name = "mythos_tune_loop",
    description = "Adjusts PID loop setpoint with auto-tune flag",
    category = "control",
    safety_level = "safety_critical"
)]
pub async fn tune_loop(
    ctx: McpContext,
    args: CustomSetpointArgs,
) -> Result<CallToolResult, McpError> {
    ctx.check_permission("tag:write")?;

    Ok(CallToolResult::ok_json(&serde_json::json!({
        "status": "applied",
        "loop_id": args.loop_id,
        "setpoint": args.setpoint_celsius,
        "auto_tune": args.auto_tune,
        "operator": ctx.caller_id(),
    }))
    .unwrap())
}

#[tokio::test]
async fn test_macro_tool_schema_generation() {
    // 1. Verify generated tool definition
    let def = TuneLoopTool::tool_definition();
    assert_eq!(def.name, "mythos_tune_loop");
    assert_eq!(
        def.description.as_deref(),
        Some("Adjusts PID loop setpoint with auto-tune flag")
    );

    // 2. Verify JSON Schema properties
    let schema = def.input_schema;
    assert_eq!(schema.get("type").and_then(|v| v.as_str()), Some("object"));
    let props = schema
        .get("properties")
        .expect("Expected properties in inputSchema");

    assert!(props.get("loop_id").is_some());
    assert_eq!(
        props
            .get("loop_id")
            .unwrap()
            .get("type")
            .and_then(|v| v.as_str()),
        Some("string")
    );

    assert!(props.get("setpoint_celsius").is_some());
    assert_eq!(
        props
            .get("setpoint_celsius")
            .unwrap()
            .get("type")
            .and_then(|v| v.as_str()),
        Some("number")
    );

    assert!(props.get("auto_tune").is_some());
    assert_eq!(
        props
            .get("auto_tune")
            .unwrap()
            .get("type")
            .and_then(|v| v.as_str()),
        Some("boolean")
    );

    // Verify required fields
    let required = schema
        .get("required")
        .and_then(|v| v.as_array())
        .expect("Expected required array");
    let req_fields: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
    assert!(req_fields.contains(&"loop_id"));
    assert!(req_fields.contains(&"setpoint_celsius"));
    assert!(req_fields.contains(&"auto_tune"));
}

#[tokio::test]
async fn test_macro_wrapper_invocation_and_registration() {
    let catalog = Arc::new(RwLock::new(TagCatalog::new()));
    let ctx = McpContext::new(catalog);
    let server = Arc::new(McpServer::new(ctx));

    // Register tool using macro-generated hook
    TuneLoopTool::register(&server).await;

    // Verify tool appears in tools/list
    let list_req = JsonRpcRequest::new(Some(RequestId::Number(1)), "tools/list", None);
    let resp = server.handle_request(list_req).await.unwrap();
    let res: ListToolsResult = serde_json::from_value(resp.result.unwrap()).unwrap();
    assert_eq!(res.tools.len(), 1);
    assert_eq!(res.tools[0].name, "mythos_tune_loop");

    // Call tool with valid typed arguments
    let call_req = JsonRpcRequest::new(
        Some(RequestId::Number(2)),
        "tools/call",
        Some(serde_json::json!({
            "name": "mythos_tune_loop",
            "arguments": {
                "loop_id": "TIC-4001",
                "setpoint_celsius": 145.5,
                "auto_tune": true
            }
        })),
    );

    let resp = server.handle_request(call_req).await.unwrap();
    assert!(resp.error.is_none());
    let call_res: CallToolResult = serde_json::from_value(resp.result.unwrap()).unwrap();
    assert_eq!(call_res.is_error, None);

    if let Content::Text { text } = &call_res.content[0] {
        assert!(text.contains("TIC-4001"));
        assert!(text.contains("145.5"));
        assert!(text.contains("applied"));
    } else {
        panic!("Expected text content");
    }
}
