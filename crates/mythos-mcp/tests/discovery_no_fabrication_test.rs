//! `mythos_discover_devices` must never report a device it did not measure.
//!
//! The tool previously returned an invented Siemens ET200SP (device_id
//! `siemens-et200sp-01`, MAC `00:1B:1B:3A:4F:90`, IP `192.168.1.10`, article
//! number `6ES7155-6AU01-0BN0`, vendor `Siemens AG`, confidence 1.0) whenever no
//! discovery engine was attached to the MCP context. Nothing marked it fabricated,
//! so a caller could not distinguish it from a real field result.
//!
//! These tests drive the real `tools/call` path through `McpServer`, not the tool
//! function in isolation, and assert the outcome a client would actually receive.

use mythos_core::discovery::{
    DiscoveredDevice, DiscoveryContext, DiscoveryEngine, DiscoveryError, ProtocolDiscoverer,
};
use mythos_core::tag::TagCatalog;
use mythos_mcp::protocol::*;
use mythos_mcp::{register_all_tools, McpContext, McpServer};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Every hardware identifier the deleted mock payload used to invent.
const FABRICATED_IDENTIFIERS: &[&str] = &[
    "siemens-et200sp-01",
    "00:1B:1B:3A:4F:90",
    "192.168.1.10",
    "6ES7155-6AU01-0BN0",
    "Siemens AG",
    "io-et200sp-cell1",
];

async fn server_with(ctx: McpContext) -> Arc<McpServer> {
    let server = Arc::new(McpServer::new(ctx));
    register_all_tools(&server).await;
    server
}

async fn context_without_engine() -> McpContext {
    let catalog = Arc::new(RwLock::new(TagCatalog::new()));
    McpContext::new(catalog)
}

async fn call_discover(server: &McpServer, args: Value) -> CallToolResult {
    let req = JsonRpcRequest::new(
        Some(RequestId::Number(1)),
        "tools/call",
        Some(json!({ "name": "mythos_discover_devices", "arguments": args })),
    );
    let resp = server
        .handle_request(req)
        .await
        .expect("tools/call returned no response");
    assert!(
        resp.error.is_none(),
        "unexpected JSON-RPC error: {:?}",
        resp.error
    );
    serde_json::from_value(resp.result.expect("missing result")).unwrap()
}

fn body(res: &CallToolResult) -> Value {
    match res.content.first() {
        Some(Content::Text { text }) => {
            serde_json::from_str(text).expect("tool body should be JSON for discover_devices")
        }
        other => panic!("expected a text content element, got {other:?}"),
    }
}

/// The whole content payload as one string, so an assertion cannot miss a
/// fabricated identifier hiding in a field the test did not think to name.
fn raw_content(res: &CallToolResult) -> String {
    serde_json::to_string(&res.content).expect("content is serializable")
}

fn assert_no_fabricated_identifiers(raw: &str) {
    for needle in FABRICATED_IDENTIFIERS {
        assert!(
            !raw.contains(needle),
            "response contains fabricated identifier {needle:?}: {raw}"
        );
    }
}

/// A discoverer that returns one device, standing in for a real field sweep.
struct FixedDiscoverer {
    protocol: &'static str,
    devices: Vec<DiscoveredDevice>,
}

#[async_trait::async_trait]
impl ProtocolDiscoverer for FixedDiscoverer {
    fn protocol_id(&self) -> &str {
        self.protocol
    }

    async fn discover(
        &self,
        _ctx: &DiscoveryContext,
    ) -> Result<Vec<DiscoveredDevice>, DiscoveryError> {
        Ok(self.devices.clone())
    }
}

#[tokio::test]
async fn no_engine_attached_reports_unavailable_with_zero_devices() {
    let server = server_with(context_without_engine().await).await;

    let res = call_discover(&server, json!({ "interface": "eth0" })).await;

    // Marked as an error: a caller must not read this as a completed sweep.
    assert_eq!(
        res.is_error,
        Some(true),
        "no engine means no scan; the result must be flagged, not returned as success"
    );

    let body = body(&res);
    assert_eq!(body["status"], "unavailable");
    assert_eq!(body["count"], 0);
    assert_eq!(
        body["devices"].as_array().map(Vec::len),
        Some(0),
        "no device may be reported when none was measured"
    );

    let reason = body["reason"]
        .as_str()
        .expect("the unavailable result must carry a reason");
    assert!(
        reason.contains("no discovery engine"),
        "reason should say why: {reason}"
    );

    assert_no_fabricated_identifiers(&raw_content(&res));
}

#[tokio::test]
async fn no_engine_attached_with_protocol_filter_still_reports_unavailable() {
    let server = server_with(context_without_engine().await).await;

    let res = call_discover(
        &server,
        json!({ "interface": "enp3s0", "protocol": "profinet-dcp" }),
    )
    .await;

    assert_eq!(res.is_error, Some(true));
    let body = body(&res);
    assert_eq!(body["status"], "unavailable");
    assert_eq!(body["count"], 0);
    assert_eq!(body["interface"], "enp3s0");
    assert_eq!(body["protocol"], "profinet-dcp");
    assert_no_fabricated_identifiers(&raw_content(&res));
}

#[tokio::test]
async fn engine_attached_reports_only_measured_devices() {
    // The positive path must still work: with an engine that really returns a
    // device, that device is reported as a success.
    let mut engine = DiscoveryEngine::new();
    engine.register(Arc::new(FixedDiscoverer {
        protocol: "profinet-dcp",
        devices: vec![DiscoveredDevice {
            device_id: "measured-device-1".to_string(),
            protocol: "profinet-dcp".to_string(),
            station_name: Some("cell-1".to_string()),
            mac_address: Some("AA:BB:CC:DD:EE:FF".to_string()),
            ip_address: None,
            modbus_slave_id: None,
            fingerprint_model: None,
            vendor: None,
            confidence: 0.75,
            properties: Default::default(),
        }],
    }));
    let engine = Arc::new(engine);

    let ctx = context_without_engine().await.with_discovery_engine(engine);
    let server = server_with(ctx).await;

    let res = call_discover(&server, json!({ "protocol": "profinet-dcp" })).await;

    assert_eq!(res.is_error, None, "a real result is not an error");
    let body = body(&res);
    assert_eq!(body["status"], "complete");
    assert_eq!(body["count"], 1);
    let devices = body["devices"].as_array().unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0]["device_id"], "measured-device-1");
    // And the invented device is still nowhere to be found.
    assert_no_fabricated_identifiers(&raw_content(&res));
}

#[tokio::test]
async fn engine_attached_but_scan_empty_is_a_genuine_zero() {
    // An engine with no discoverers registered runs nothing and finds nothing.
    // That is a real (degenerate) measurement, and is reported as a success with
    // count 0 — distinct from the unavailable case above, which is an error.
    let ctx = context_without_engine()
        .await
        .with_discovery_engine(Arc::new(DiscoveryEngine::new()));
    let server = server_with(ctx).await;

    let res = call_discover(&server, json!({})).await;

    assert_eq!(res.is_error, None, "a completed sweep is not an error");
    let body = body(&res);
    assert_eq!(body["status"], "complete");
    assert_eq!(body["count"], 0);
    assert_eq!(body["devices"].as_array().map(Vec::len), Some(0));
    assert_no_fabricated_identifiers(&raw_content(&res));
}

#[tokio::test]
async fn failed_scan_is_not_reported_as_an_empty_network() {
    // Requesting a protocol the engine has no discoverer for makes run_discovery
    // return an error. It must surface as a failure, not as "0 devices found".
    let ctx = context_without_engine()
        .await
        .with_discovery_engine(Arc::new(DiscoveryEngine::new()));
    let server = server_with(ctx).await;

    let res = call_discover(&server, json!({ "protocol": "modbus-tcp" })).await;

    assert_eq!(
        res.is_error,
        Some(true),
        "a scan that could not run must not look like a scan that found nothing"
    );
    let body = body(&res);
    assert_eq!(body["status"], "failed");
    assert_eq!(body["count"], 0);
    let reason = body["reason"]
        .as_str()
        .expect("failure must carry a reason");
    assert!(
        reason.contains("modbus-tcp"),
        "reason should name the protocol that failed: {reason}"
    );
    assert_no_fabricated_identifiers(&raw_content(&res));
}
