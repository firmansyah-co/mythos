use mythos_core::tag::{AccessMode, TagCatalog, TagMetadata};
use mythos_mcp::protocol::*;
use mythos_mcp::{
    mythos_mcp_tool, CallToolResult, McpContext, McpError, McpServer, StdioTransport,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct ReadTagArgs {
    /// Canonical tag URI
    uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct WriteTagArgs {
    /// Canonical tag URI
    uri: String,
    /// Value to write
    value: f64,
}

/// Reads a process variable from the TagBus
#[mythos_mcp_tool(name = "read_tag", category = "telemetry", safety_level = "read_only")]
async fn read_tag(ctx: McpContext, args: ReadTagArgs) -> Result<CallToolResult, McpError> {
    let catalog = ctx.tag_catalog.read().await;
    match catalog.get_by_uri(&args.uri) {
        Some(meta) => Ok(CallToolResult::ok_json(&serde_json::json!({
            "uri": meta.uri,
            "slot_id": meta.slot_id,
            "units": meta.engineering_units,
            "value": 42.0,
        }))
        .unwrap()),
        None => Err(McpError::ToolNotFound(format!(
            "Tag '{}' not found",
            args.uri
        ))),
    }
}

/// Writes a setpoint with safety interlocks
#[mythos_mcp_tool(
    name = "write_tag",
    category = "control",
    safety_level = "safety_critical"
)]
async fn write_tag(ctx: McpContext, args: WriteTagArgs) -> Result<CallToolResult, McpError> {
    let meta = ctx.validate_tag_write(&args.uri).await?;
    Ok(CallToolResult::ok_text(format!(
        "Tag '{}' (slot {}) set to {}",
        meta.uri, meta.slot_id, args.value
    )))
}

async fn setup_test_server() -> (Arc<McpServer>, Arc<RwLock<TagCatalog>>) {
    let mut catalog = TagCatalog::new();
    let mut ro_tag = TagMetadata::new("external.Boiler.PT101.pressure.pv", 0).unwrap();
    ro_tag.access_mode = AccessMode::ReadOnly;
    ro_tag.engineering_units = "bar".to_string();
    catalog.register(ro_tag).unwrap();

    let mut rw_tag = TagMetadata::new("external.Boiler.SP101.pressure.sp", 1).unwrap();
    rw_tag.access_mode = AccessMode::ReadWrite;
    rw_tag.engineering_units = "bar".to_string();
    catalog.register(rw_tag).unwrap();

    let catalog_arc = Arc::new(RwLock::new(catalog));
    let ctx = McpContext::new(Arc::clone(&catalog_arc));
    let server = Arc::new(McpServer::new(ctx));

    // Register tools using generated hooks
    ReadTagTool::register(&server).await;
    WriteTagTool::register(&server).await;

    (server, catalog_arc)
}

#[tokio::test]
async fn test_mcp_initialize() {
    let (server, _) = setup_test_server().await;

    let init_req = JsonRpcRequest::new(
        Some(RequestId::Number(1)),
        "initialize",
        Some(serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "test-agent", "version": "1.0.0" }
        })),
    );

    let resp = server
        .handle_request(init_req)
        .await
        .expect("Expected response");
    assert_eq!(resp.id, Some(RequestId::Number(1)));
    assert!(resp.error.is_none());

    let res: InitializeResult = serde_json::from_value(resp.result.unwrap()).unwrap();
    assert_eq!(res.protocol_version, "2024-11-05");
    assert_eq!(res.server_info.name, "mythos-mcp");
    assert!(res.capabilities.tools.is_some());
}

#[tokio::test]
async fn test_mcp_tools_list() {
    let (server, _) = setup_test_server().await;

    let list_req = JsonRpcRequest::new(Some(RequestId::Number(2)), "tools/list", None);
    let resp = server.handle_request(list_req).await.unwrap();

    let res: ListToolsResult = serde_json::from_value(resp.result.unwrap()).unwrap();
    assert_eq!(res.tools.len(), 2);

    let tool_names: Vec<String> = res.tools.iter().map(|t| t.name.clone()).collect();
    assert!(tool_names.contains(&"read_tag".to_string()));
    assert!(tool_names.contains(&"write_tag".to_string()));

    let read_tool = res.tools.iter().find(|t| t.name == "read_tag").unwrap();
    assert_eq!(
        read_tool.description.as_deref(),
        Some("Reads a process variable from the TagBus")
    );
    assert!(read_tool.input_schema.get("properties").is_some());
}

#[tokio::test]
async fn test_mcp_tool_call_read() {
    let (server, _) = setup_test_server().await;

    let call_req = JsonRpcRequest::new(
        Some(RequestId::String("call-1".to_string())),
        "tools/call",
        Some(serde_json::json!({
            "name": "read_tag",
            "arguments": {
                "uri": "external.Boiler.PT101.pressure.pv"
            }
        })),
    );

    let resp = server.handle_request(call_req).await.unwrap();
    let res: CallToolResult = serde_json::from_value(resp.result.unwrap()).unwrap();
    assert_eq!(res.is_error, None);
    assert_eq!(res.content.len(), 1);

    if let Content::Text { text } = &res.content[0] {
        assert!(text.contains("external.Boiler.PT101.pressure.pv"));
        assert!(text.contains("bar"));
    } else {
        panic!("Expected Text content");
    }
}

#[tokio::test]
async fn test_mcp_safety_interlock_readonly_rejection() {
    let (server, _) = setup_test_server().await;

    // Attempt to write to ReadOnly tag
    let write_req = JsonRpcRequest::new(
        Some(RequestId::Number(3)),
        "tools/call",
        Some(serde_json::json!({
            "name": "write_tag",
            "arguments": {
                "uri": "external.Boiler.PT101.pressure.pv",
                "value": 12.5
            }
        })),
    );

    let resp = server.handle_request(write_req).await.unwrap();
    let res: CallToolResult = serde_json::from_value(resp.result.unwrap()).unwrap();
    assert_eq!(res.is_error, Some(true));

    if let Content::Text { text } = &res.content[0] {
        assert!(text.contains("safety interlock"));
    } else {
        panic!("Expected Text content");
    }
}

#[tokio::test]
async fn test_mcp_tool_call_write_allowed() {
    let (server, _) = setup_test_server().await;

    // Attempt to write to ReadWrite tag
    let write_req = JsonRpcRequest::new(
        Some(RequestId::Number(4)),
        "tools/call",
        Some(serde_json::json!({
            "name": "write_tag",
            "arguments": {
                "uri": "external.Boiler.SP101.pressure.sp",
                "value": 8.5
            }
        })),
    );

    let resp = server.handle_request(write_req).await.unwrap();
    let res: CallToolResult = serde_json::from_value(resp.result.unwrap()).unwrap();
    assert_eq!(res.is_error, None);

    if let Content::Text { text } = &res.content[0] {
        assert!(text.contains("set to 8.5"));
    } else {
        panic!("Expected Text content");
    }
}

#[tokio::test]
async fn test_mcp_stdio_stream_processing() {
    let (server, _) = setup_test_server().await;
    let transport = StdioTransport::new();

    let input_data = r#"{"jsonrpc":"2.0","id":100,"method":"ping"}
{"jsonrpc":"2.0","id":101,"method":"tools/list"}
"#;

    let mut input_cursor = std::io::Cursor::new(input_data.as_bytes());
    let mut output_buf = Vec::new();

    transport
        .process_streams(server, &mut input_cursor, &mut output_buf)
        .await
        .unwrap();

    let output_str = String::from_utf8(output_buf).unwrap();
    let lines: Vec<&str> = output_str.trim().lines().collect();
    assert_eq!(lines.len(), 2);

    let resp1: JsonRpcResponse = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(resp1.id, Some(RequestId::Number(100)));
    assert_eq!(resp1.result, Some(serde_json::json!({})));

    let resp2: JsonRpcResponse = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(resp2.id, Some(RequestId::Number(101)));
    let list_res: ListToolsResult = serde_json::from_value(resp2.result.unwrap()).unwrap();
    assert_eq!(list_res.tools.len(), 2);
}

#[tokio::test]
async fn test_mcp_error_tool_not_found() {
    let (server, _) = setup_test_server().await;

    let call_req = JsonRpcRequest::new(
        Some(RequestId::Number(99)),
        "tools/call",
        Some(serde_json::json!({
            "name": "non_existent_tool",
            "arguments": {}
        })),
    );

    let resp = server.handle_request(call_req).await.unwrap();
    assert_eq!(resp.id, Some(RequestId::Number(99)));
    assert!(resp.error.is_some());
    let err = resp.error.unwrap();
    assert_eq!(err.code, METHOD_NOT_FOUND);
    assert!(err.message.contains("Tool 'non_existent_tool' not found"));
}

#[tokio::test]
async fn test_mcp_error_invalid_params() {
    let (server, _) = setup_test_server().await;

    // Missing required field 'uri' in ReadTagArgs
    let call_req = JsonRpcRequest::new(
        Some(RequestId::Number(100)),
        "tools/call",
        Some(serde_json::json!({
            "name": "read_tag",
            "arguments": {
                "invalid_field": 123
            }
        })),
    );

    let resp = server.handle_request(call_req).await.unwrap();
    let res: CallToolResult = serde_json::from_value(resp.result.unwrap()).unwrap();
    assert_eq!(res.is_error, Some(true));
    if let Content::Text { text } = &res.content[0] {
        assert!(
            text.contains("Parameter schema validation error")
                || text.contains("missing field `uri`")
        );
    } else {
        panic!("Expected Text content");
    }

    // Call tools/call without params object
    let no_params_req = JsonRpcRequest::new(Some(RequestId::Number(101)), "tools/call", None);
    let no_params_resp = server.handle_request(no_params_req).await.unwrap();
    assert!(no_params_resp.error.is_some());
    let err = no_params_resp.error.unwrap();
    assert_eq!(err.code, INVALID_PARAMS);
}

#[tokio::test]
async fn test_mcp_error_parse_error() {
    let (server, _) = setup_test_server().await;

    // Malformed JSON sent to handle_raw_json
    let malformed_json = r#"{"jsonrpc": "2.0", "method": "ping", broken"#;
    let resp_str = server
        .handle_raw_json(malformed_json)
        .await
        .expect("Expected error response");
    let resp: JsonRpcResponse = serde_json::from_str(&resp_str).unwrap();

    assert!(resp.error.is_some());
    let err = resp.error.unwrap();
    assert_eq!(err.code, PARSE_ERROR);
    assert!(err.message.contains("JSON parse error"));
}
