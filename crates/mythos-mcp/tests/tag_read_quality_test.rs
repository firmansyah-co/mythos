//! `mythos_read_tag` must not stamp GOOD quality on a value nobody measured.
//!
//! `McpContext::execute_tag_read` returned `Ok((0.0, 192, now_ns))` for a tag that is in the
//! catalog but has no recorded value. `192` is `0xC0` — GOOD. So the read path reported a
//! plausible engineering value (0.0), with the one signal a consumer uses to decide a value is
//! trustworthy, and a timestamp implying it had just been sampled.
//!
//! # Why these tests assert on quality, not on the value
//!
//! A test asserting "the value is 0.0" passes both before and after the fix and establishes
//! nothing: the defect *is* that a fabricated 0.0 is indistinguishable from a measured one.
//! Every assertion below is therefore about the quality and about distinguishability, and each
//! one is shown red against the pre-fix behaviour in the negative controls recorded on the PR.

use mythos_core::tag::{AccessMode, TagCatalog, TagMetadata};
use mythos_mcp::protocol::*;
use mythos_mcp::{register_all_tools, McpContext, McpServer};
use mythos_shm::slot::QualityCode;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;

/// A catalog tag with a slot, and nothing recorded for it.
const UNMEASURED_URI: &str = "external.Boiler.PT101.pressure.pv";
/// A catalog tag whose recorded value is a genuine zero.
const ZERO_URI: &str = "external.Boiler.FT101.flow.pv";

async fn server() -> Arc<McpServer> {
    let mut catalog = TagCatalog::new();

    let mut unmeasured = TagMetadata::new(UNMEASURED_URI, 0).unwrap();
    unmeasured.access_mode = AccessMode::ReadOnly;
    unmeasured.engineering_units = "bar".to_string();
    catalog.register(unmeasured).unwrap();

    let mut zero = TagMetadata::new(ZERO_URI, 1).unwrap();
    // ReadWrite: the fixture records a value through the real write path, and the safety
    // interlock rejects writes to READ-ONLY tags.
    zero.access_mode = AccessMode::ReadWrite;
    zero.engineering_units = "m3/h".to_string();
    catalog.register(zero).unwrap();

    let server = Arc::new(McpServer::new(McpContext::new(Arc::new(RwLock::new(
        catalog,
    )))));
    register_all_tools(&server).await;
    server
}

async fn read(server: &McpServer, uri: &str) -> Value {
    let req = JsonRpcRequest::new(
        Some(RequestId::Number(1)),
        "tools/call",
        Some(json!({ "name": "mythos_read_tag", "arguments": { "uri": uri } })),
    );
    let resp = server.handle_request(req).await.expect("no response");
    assert!(
        resp.error.is_none(),
        "unexpected JSON-RPC error: {:?}",
        resp.error
    );
    let result: CallToolResult = serde_json::from_value(resp.result.expect("no result")).unwrap();
    assert_eq!(
        result.is_error, None,
        "read of a catalog tag should not error"
    );
    match result.content.first() {
        Some(Content::Text { text }) => serde_json::from_str(text).expect("body should be JSON"),
        other => panic!("expected text content, got {other:?}"),
    }
}

/// Write through the real tool and assert the write *took effect*, not merely that the JSON-RPC
/// envelope carried no error: a tool-level failure (a safety interlock, say) still returns a
/// successful envelope with `isError` set inside the result, so asserting on the envelope alone
/// would let a fixture silently record nothing and make the test prove nothing.
async fn write_value(server: &McpServer, uri: &str, value: f64) {
    let req = JsonRpcRequest::new(
        Some(RequestId::Number(99)),
        "tools/call",
        Some(json!({
            "name": "mythos_write_tag",
            "arguments": { "uri": uri, "value": value, "operator_intent": "test fixture" }
        })),
    );
    let resp = server.handle_request(req).await.expect("no response");
    assert!(
        resp.error.is_none(),
        "write envelope errored: {:?}",
        resp.error
    );
    let result: CallToolResult = serde_json::from_value(resp.result.expect("no result")).unwrap();
    assert_eq!(
        result.is_error, None,
        "the fixture write did not succeed, so the test below would prove nothing: {:?}",
        result.content
    );
}

#[tokio::test]
async fn an_unmeasured_tag_does_not_come_back_good() {
    let server = server().await;
    let body = read(&server, UNMEASURED_URI).await;

    let quality = body["quality"].as_u64().expect("quality must be present");
    assert_ne!(
        quality,
        u64::from(QualityCode::GOOD.code()),
        "a tag with no recorded value must not be reported GOOD; body: {body}"
    );
    assert_eq!(
        quality,
        u64::from(QualityCode::UNCERTAIN_INITIAL_VALUE.code()),
        "the defined code for 'no first reading yet' is UNCERTAIN_INITIAL_VALUE; body: {body}"
    );
    assert_eq!(body["quality_name"], "UNCERTAIN_INITIAL_VALUE");
    assert_ne!(body["quality_class"], "GOOD");
}

#[tokio::test]
async fn an_unmeasured_tag_is_distinguishable_from_a_genuine_zero() {
    let server = server().await;

    // Record a genuine 0.0 through the write path, so the tag has a real measured zero.
    write_value(&server, ZERO_URI, 0.0).await;

    let measured = read(&server, ZERO_URI).await;
    assert_eq!(
        measured["has_value"], true,
        "the fixture write must actually have recorded a value, or this test proves nothing; \
         body: {measured}"
    );
    let unmeasured = read(&server, UNMEASURED_URI).await;

    // Both carry 0.0 in the value field, and the value field alone cannot tell them apart —
    // which is precisely the defect. The quality must.
    assert_eq!(measured["value"], 0.0);
    assert_ne!(
        measured["quality"], unmeasured["quality"],
        "a measured 0.0 and an unmeasured tag must not share a quality code; \
         measured: {measured}, unmeasured: {unmeasured}"
    );
    assert_eq!(
        measured["quality_name"], "GOOD",
        "a value this context wrote is real, so GOOD is honest there; body: {measured}"
    );
}

#[tokio::test]
async fn an_unmeasured_tag_has_no_timestamp() {
    let server = server().await;
    let body = read(&server, UNMEASURED_URI).await;

    assert_eq!(body["value"], Value::Null, "there is no value to report");
    assert_eq!(body["has_value"], false);
    assert_eq!(
        body["timestamp_ns"],
        Value::Null,
        "a timestamp for a sample that never happened is the same defect as the value; \
         body: {body}"
    );
}

#[tokio::test]
async fn a_measured_tag_still_reports_its_value_and_timestamp() {
    let server = server().await;
    write_value(&server, ZERO_URI, 12.5).await;

    let body = read(&server, ZERO_URI).await;
    assert_eq!(body["value"], 12.5);
    assert_eq!(body["has_value"], true);
    assert_eq!(body["quality_name"], "GOOD");
    assert!(
        body["timestamp_ns"].as_u64().unwrap_or(0) > 0,
        "a real reading carries a real timestamp; body: {body}"
    );
}
