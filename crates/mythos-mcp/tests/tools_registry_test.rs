//! Comprehensive integration test for TSK-503 (AI Industrial Automation Tools Registry)
//! and End-to-End MCP tool invocation through McpServer.

use mythos_core::alarm::{
    AlarmDefinition, AlarmRuntimeState, AlarmSeverity, AlarmState, AlarmTripType,
};
use mythos_core::auth::MerkleAuditLedger;
use mythos_core::historian::TimeSeriesPoint;
use mythos_core::tag::{AccessMode, TagCatalog, TagMetadata};
use mythos_mcp::protocol::*;
use mythos_mcp::tools::*;
use mythos_mcp::{McpContext, McpServer};
use serde_json::json;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::sync::RwLock;

async fn setup_full_server() -> (Arc<McpServer>, Arc<RwLock<TagCatalog>>, NamedTempFile) {
    let mut catalog = TagCatalog::new();

    // 1. Read-only sensor tag
    let mut pt101 = TagMetadata::new("external.Boiler.PT101.pressure.pv", 0).unwrap();
    pt101.access_mode = AccessMode::ReadOnly;
    pt101.engineering_units = "bar".to_string();
    pt101.scale_min = 0.0;
    pt101.scale_max = 20.0;
    catalog.register(pt101).unwrap();

    // 2. Writable setpoint tag with limits 0.0 .. 16.0 bar
    let mut sp101 = TagMetadata::new("external.Boiler.SP101.pressure.sp", 1).unwrap();
    sp101.access_mode = AccessMode::ReadWrite;
    sp101.engineering_units = "bar".to_string();
    sp101.scale_min = 0.0;
    sp101.scale_max = 16.0;
    catalog.register(sp101).unwrap();

    let catalog_arc = Arc::new(RwLock::new(catalog));
    let audit_temp = NamedTempFile::new().unwrap();
    let audit_ledger = Arc::new(MerkleAuditLedger::open_or_create(audit_temp.path()).unwrap());

    let ctx =
        McpContext::new(Arc::clone(&catalog_arc)).with_audit_ledger(Arc::clone(&audit_ledger));

    // Register an alarm into alarm registry
    {
        let mut reg = ctx.alarm_registry.write().await;
        let def = AlarmDefinition {
            alarm_id: 101,
            tag_name: "external.Boiler.PT101.pressure.pv".to_string(),
            description: "High Steam Pressure Trip".to_string(),
            severity: AlarmSeverity::Critical,
            trip_type: AlarmTripType::AnalogHigh {
                trip_threshold: 15.0,
                deadband: 0.5,
            },
            on_delay_ms: 0,
            off_delay_ms: 0,
            first_out_group_id: Some(1),
            plant_suppression_tag: None,
            max_shelve_duration_sec: 3600,
        };
        let mut state = AlarmRuntimeState::new(101);
        state.current_state = AlarmState::UnackAlarm;
        state.is_active_condition = true;

        reg.definitions.insert(101, def);
        reg.states.insert(101, state);
    }

    // Populate historian buffer
    {
        let mut hist = ctx.historian_buffer.write().await;
        hist.series.insert(
            "external.Boiler.PT101.pressure.pv".to_string(),
            vec![
                TimeSeriesPoint {
                    timestamp_ns: 1_000_000_000,
                    value: 10.0,
                },
                TimeSeriesPoint {
                    timestamp_ns: 2_000_000_000,
                    value: 12.5,
                },
                TimeSeriesPoint {
                    timestamp_ns: 3_000_000_000,
                    value: 15.2,
                },
            ],
        );
    }

    let server = Arc::new(McpServer::new(ctx));
    register_all_tools(&server).await;

    (server, catalog_arc, audit_temp)
}

#[tokio::test]
async fn test_tools_list_contains_all_sprint5_tools() {
    let (server, _, _audit) = setup_full_server().await;

    let req = JsonRpcRequest::new(Some(RequestId::Number(1)), "tools/list", None);
    let resp = server.handle_request(req).await.unwrap();
    assert!(resp.error.is_none());

    let list_res: ListToolsResult = serde_json::from_value(resp.result.unwrap()).unwrap();
    let names: Vec<String> = list_res.tools.iter().map(|t| t.name.clone()).collect();

    let expected_tools = [
        "mythos_read_tag",
        "mythos_write_tag",
        "mythos_get_alarm_summary",
        "mythos_ack_alarm",
        "mythos_shelve_alarm",
        "mythos_compile_logic",
        "mythos_hot_reload_logic",
        "mythos_discover_devices",
        "mythos_export_telemetry_csv",
    ];

    for expected in &expected_tools {
        assert!(
            names.contains(&expected.to_string()),
            "Missing expected tool: {}",
            expected
        );
    }
}

#[tokio::test]
async fn test_tool_mythos_read_and_write_tag_with_audit() {
    let (server, _, _audit) = setup_full_server().await;

    // 1. Read PT101
    let read_req = JsonRpcRequest::new(
        Some(RequestId::Number(2)),
        "tools/call",
        Some(json!({
            "name": "mythos_read_tag",
            "arguments": {
                "uri": "external.Boiler.PT101.pressure.pv"
            }
        })),
    );
    let read_resp = server.handle_request(read_req).await.unwrap();
    assert!(read_resp.error.is_none());

    // 2. Write Read-Only tag PT101 -> Must be rejected by safety interlock
    let write_ro_req = JsonRpcRequest::new(
        Some(RequestId::Number(3)),
        "tools/call",
        Some(json!({
            "name": "mythos_write_tag",
            "arguments": {
                "uri": "external.Boiler.PT101.pressure.pv",
                "value": 14.0,
                "operator_intent": "Attempt write to RO"
            }
        })),
    );
    let write_ro_resp = server.handle_request(write_ro_req).await.unwrap();
    let tool_res: CallToolResult = serde_json::from_value(write_ro_resp.result.unwrap()).unwrap();
    assert_eq!(tool_res.is_error, Some(true));

    // 3. Write SP101 with out-of-bounds value (25.0 > max 16.0) -> Must be rejected by clamp check
    let write_oob_req = JsonRpcRequest::new(
        Some(RequestId::Number(4)),
        "tools/call",
        Some(json!({
            "name": "mythos_write_tag",
            "arguments": {
                "uri": "external.Boiler.SP101.pressure.sp",
                "value": 25.0,
                "operator_intent": "Dangerous overpressure setpoint"
            }
        })),
    );
    let write_oob_resp = server.handle_request(write_oob_req).await.unwrap();
    let tool_res: CallToolResult = serde_json::from_value(write_oob_resp.result.unwrap()).unwrap();
    assert_eq!(tool_res.is_error, Some(true));

    // 4. Valid write to SP101 within bounds (12.5 bar) -> Must succeed and write to audit ledger
    let write_valid_req = JsonRpcRequest::new(
        Some(RequestId::Number(5)),
        "tools/call",
        Some(json!({
            "name": "mythos_write_tag",
            "arguments": {
                "uri": "external.Boiler.SP101.pressure.sp",
                "value": 12.5,
                "operator_intent": "Energy efficiency optimization setpoint"
            }
        })),
    );
    let write_valid_resp = server.handle_request(write_valid_req).await.unwrap();
    let tool_res: CallToolResult =
        serde_json::from_value(write_valid_resp.result.unwrap()).unwrap();
    assert_eq!(tool_res.is_error, None);

    // Verify audit ledger
    let ledger = server.context().audit_ledger().unwrap();
    assert!(ledger.verify_integrity().is_ok());
    let entries = ledger.read_entries().unwrap();
    assert!(!entries.is_empty());
    let last = entries.last().unwrap();
    assert_eq!(last.action, "write_tag");
    assert_eq!(last.target, "external.Boiler.SP101.pressure.sp");
    assert!(last.verify_hash());
}

#[tokio::test]
async fn test_tool_alarm_lifecycle_ack_and_shelve() {
    let (server, _, _audit) = setup_full_server().await;

    // 1. Get summary
    let sum_req = JsonRpcRequest::new(
        Some(RequestId::Number(10)),
        "tools/call",
        Some(json!({
            "name": "mythos_get_alarm_summary",
            "arguments": { "filter": "UNACK" }
        })),
    );
    let sum_resp = server.handle_request(sum_req).await.unwrap();
    let res: CallToolResult = serde_json::from_value(sum_resp.result.unwrap()).unwrap();
    assert_eq!(res.is_error, None);

    // 2. Ack alarm 101
    let ack_req = JsonRpcRequest::new(
        Some(RequestId::Number(11)),
        "tools/call",
        Some(json!({
            "name": "mythos_ack_alarm",
            "arguments": {
                "alarm_id": 101,
                "reason": "AI verified steam bypass open"
            }
        })),
    );
    let ack_resp = server.handle_request(ack_req).await.unwrap();
    let res: CallToolResult = serde_json::from_value(ack_resp.result.unwrap()).unwrap();
    assert_eq!(res.is_error, None);

    // 3. Shelve alarm 101
    let shelve_req = JsonRpcRequest::new(
        Some(RequestId::Number(12)),
        "tools/call",
        Some(json!({
            "name": "mythos_shelve_alarm",
            "arguments": {
                "alarm_id": 101,
                "duration_seconds": 1800,
                "reason": "Sensor calibration maintenance scheduled"
            }
        })),
    );
    let shelve_resp = server.handle_request(shelve_req).await.unwrap();
    let res: CallToolResult = serde_json::from_value(shelve_resp.result.unwrap()).unwrap();
    assert_eq!(res.is_error, None);
}

#[tokio::test]
async fn test_tool_compile_and_hot_reload() {
    let (server, _, _audit) = setup_full_server().await;

    // 1. Compile logic
    let comp_req = JsonRpcRequest::new(
        Some(RequestId::Number(20)),
        "tools/call",
        Some(json!({
            "name": "mythos_compile_logic",
            "arguments": {
                "target_block": "MainTask",
                "language": "StructuredText",
                "source_code": "PROGRAM MainTask\nVAR\n  Count: INT;\nEND_VAR\nCount := Count + 1;\nEND_PROGRAM"
            }
        })),
    );
    let comp_resp = server.handle_request(comp_req).await.unwrap();
    let res: CallToolResult = serde_json::from_value(comp_resp.result.unwrap()).unwrap();
    assert_eq!(res.is_error, None);

    // 2. Hot reload logic
    let reload_req = JsonRpcRequest::new(
        Some(RequestId::Number(21)),
        "tools/call",
        Some(json!({
            "name": "mythos_hot_reload_logic",
            "arguments": {
                "module_path": "/var/lib/mythos/logic/active/MainTask.so",
                "operator_intent": "Update PID gain loop"
            }
        })),
    );
    let reload_resp = server.handle_request(reload_req).await.unwrap();
    let res: CallToolResult = serde_json::from_value(reload_resp.result.unwrap()).unwrap();
    assert_eq!(res.is_error, None);
}

#[tokio::test]
async fn test_tool_device_discovery_and_telemetry_csv() {
    let (server, _, _audit) = setup_full_server().await;

    // 1. Discover devices
    let disc_req = JsonRpcRequest::new(
        Some(RequestId::Number(30)),
        "tools/call",
        Some(json!({
            "name": "mythos_discover_devices",
            "arguments": {
                "interface": "eth0",
                "protocol": "profinet-dcp"
            }
        })),
    );
    let disc_resp = server.handle_request(disc_req).await.unwrap();
    let res: CallToolResult = serde_json::from_value(disc_resp.result.unwrap()).unwrap();
    assert_eq!(res.is_error, None);

    // 2. Export telemetry CSV
    let csv_req = JsonRpcRequest::new(
        Some(RequestId::Number(31)),
        "tools/call",
        Some(json!({
            "name": "mythos_export_telemetry_csv",
            "arguments": {
                "tags": ["external.Boiler.PT101.pressure.pv"],
                "start_time_ns": 1_000_000_000u64,
                "end_time_ns": 3_000_000_000u64,
                "interval_ms": 1000,
                "resample_strategy": "FORWARD_FILL"
            }
        })),
    );
    let csv_resp = server.handle_request(csv_req).await.unwrap();
    let res: CallToolResult = serde_json::from_value(csv_resp.result.unwrap()).unwrap();
    assert_eq!(res.is_error, None);
    if let Content::Text { text } = &res.content[0] {
        assert!(text.contains("timestamp,external.Boiler.PT101.pressure.pv"));
        assert!(text.contains("10"));
    } else {
        panic!("Expected CSV text output");
    }
}
