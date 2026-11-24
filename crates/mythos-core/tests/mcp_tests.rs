//! The MCP endpoint: every route is a tool, and an agent can run the controller through it.
//!
//! Run against the real router, a real I/O manager, a real shared-memory tag bus segment and a Modbus TCP
//! simulator, as `io_config_api_tests` is.

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    Router,
};
use driver_modbus::sim::ModbusSimServer;
use mythos_core::{
    api::{create_router, ApiState},
    io::{DriverRegistry, IoManager},
    mcp::{NOT_TOOLS, TOOLS},
    storage::ControllerStorage,
    tag::TagCatalog,
};
use mythos_pal::DefaultShm;
use mythos_shm::ShmReaderHandle;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tower::ServiceExt;

const CAPACITY: usize = 64;
static SEQ: AtomicU32 = AtomicU32::new(0);
const PRESSURE: &str = "external.BoilerRoom.PT101.pressure";

// -----------------------------------------------------------------------------
// Coverage
// -----------------------------------------------------------------------------

/// The (method, path) pairs a router source declares with `.route("...", method(...)...)`.
fn routes_in(source: &str) -> BTreeSet<(String, String)> {
    let mut routes = BTreeSet::new();
    let mut rest = source;
    while let Some(start) = rest.find(".route(") {
        rest = &rest[start + ".route(".len()..];
        // The call's text, up to its matching parenthesis.
        let mut depth = 1;
        let mut end = 0;
        for (i, c) in rest.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i;
                        break;
                    }
                }
                _ => {}
            }
        }
        let call = &rest[..end];
        let Some(open) = call.find('"') else { continue };
        let Some(close) = call[open + 1..].find('"') else {
            continue;
        };
        let path = call[open + 1..open + 1 + close].to_string();
        let handlers = &call[open + 1 + close..];
        for (needle, method) in [
            ("get(", "GET"),
            ("post(", "POST"),
            ("put(", "PUT"),
            ("delete(", "DELETE"),
            ("patch(", "PATCH"),
        ] {
            let mut search = handlers;
            while let Some(at) = search.find(needle) {
                // `get(` but not `target(` or `budget(`.
                let before = search[..at].chars().last();
                if !matches!(before, Some(c) if c.is_alphanumeric() || c == '_') {
                    routes.insert((method.to_string(), path.clone()));
                }
                search = &search[at + needle.len()..];
            }
        }
    }
    routes
}

#[test]
fn every_route_of_the_api_is_a_tool_or_says_why_not() {
    let declared: BTreeSet<(String, String)> = routes_in(include_str!("../src/api/mod.rs"))
        .into_iter()
        .chain(routes_in(include_str!("../src/api/web.rs")))
        .chain(routes_in(include_str!("../src/mcp.rs")))
        .filter(|(_, path)| path != "/mcp")
        .collect();
    assert!(
        declared.len() > 30,
        "the route reader found only {} routes; it is broken, not the API",
        declared.len()
    );

    let covered: BTreeSet<(String, String)> = TOOLS
        .iter()
        .map(|t| (t.method.to_string(), t.path.to_string()))
        .chain(
            NOT_TOOLS
                .iter()
                .map(|n| (n.method.to_string(), n.path.to_string())),
        )
        .collect();

    let missing: Vec<_> = declared.difference(&covered).collect();
    assert!(
        missing.is_empty(),
        "routes with neither a tool nor a reason in NOT_TOOLS: {missing:?}"
    );
    let stale: Vec<_> = covered.difference(&declared).collect();
    assert!(
        stale.is_empty(),
        "tools or exemptions for routes that do not exist: {stale:?}"
    );
}

#[test]
fn the_route_reader_sees_a_route_added_in_the_usual_form() {
    // The coverage test is only as good as this parser; show it the shapes the router uses.
    let source = r#"
        .route("/a", get(x::<S>).post(y::<S>))
        .route(
            "/b/:id",
            axum::routing::delete(z::<S>),
        )
        .route("/c", put(w))
    "#;
    let found: Vec<(String, String)> = routes_in(source).into_iter().collect();
    assert_eq!(
        found,
        vec![
            ("DELETE".to_string(), "/b/:id".to_string()),
            ("GET".to_string(), "/a".to_string()),
            ("POST".to_string(), "/a".to_string()),
            ("PUT".to_string(), "/c".to_string()),
        ]
    );
}

// -----------------------------------------------------------------------------
// Protocol and tools, end to end
// -----------------------------------------------------------------------------

struct Rig {
    router: Router,
    io: Arc<IoManager<DefaultShm>>,
    segment: String,
    _dir: tempfile::TempDir,
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = DefaultShm::unlink(&self.segment);
    }
}

async fn rig() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(dir.path()).unwrap());
    let segment = format!(
        "mythos_mcp_test_{}_{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::SeqCst)
    );
    let _ = DefaultShm::unlink(&segment);
    let catalog = Arc::new(RwLock::new(TagCatalog::new()));
    let mut registry = DriverRegistry::empty();
    registry.register("modbus", || Box::new(driver_modbus::ModbusDriver::new()));
    let io = IoManager::<DefaultShm>::new(
        Arc::clone(&storage),
        Arc::clone(&catalog),
        segment.clone(),
        CAPACITY,
        registry,
    );
    io.reload().await;
    let reader = ShmReaderHandle::<DefaultShm>::new(&segment, CAPACITY).unwrap();
    let state = ApiState::<DefaultShm>::new(storage, catalog, Some(reader))
        .with_io_manager(Arc::clone(&io));
    Rig {
        router: create_router(state),
        io,
        segment,
        _dir: dir,
    }
}

impl Rig {
    async fn post(&self, message: Value, origin: Option<&str>) -> (StatusCode, Value) {
        let mut request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header(header::HOST, "127.0.0.1:8080")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "application/json, text/event-stream");
        if let Some(origin) = origin {
            request = request.header(header::ORIGIN, origin);
        }
        let response = self
            .router
            .clone()
            .oneshot(request.body(Body::from(message.to_string())).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn rpc(&self, method: &str, params: Value) -> Value {
        let (status, body) = self
            .post(
                json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    /// Call a tool; returns (isError, status the route answered, its body).
    async fn tool(&self, name: &str, arguments: Value) -> (bool, Value, Value) {
        let answer = self
            .rpc("tools/call", json!({"name": name, "arguments": arguments}))
            .await;
        let result = &answer["result"];
        assert!(result.is_object(), "no result: {answer}");
        (
            result["isError"].as_bool().unwrap(),
            result["structuredContent"]["status"].clone(),
            result["structuredContent"]["body"].clone(),
        )
    }
}

#[tokio::test]
async fn the_endpoint_speaks_the_protocol() {
    let rig = rig().await;

    let init = rig
        .rpc(
            "initialize",
            json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}),
        )
        .await;
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "mythos");
    assert!(init["result"]["capabilities"]["tools"].is_object());

    // A version this server does not know is answered with the newest it does.
    let old = rig
        .rpc("initialize", json!({"protocolVersion": "1999-01-01"}))
        .await;
    assert_eq!(old["result"]["protocolVersion"], "2025-06-18");

    // A notification gets no JSON-RPC answer.
    let (status, _) = rig
        .post(
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);

    assert_eq!(rig.rpc("ping", json!({})).await["result"], json!({}));

    let listed = rig.rpc("tools/list", json!({})).await;
    let tools = listed["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), TOOLS.len());
    for tool in tools {
        assert_eq!(tool["inputSchema"]["type"], "object", "{tool}");
        assert!(tool["description"].as_str().unwrap().len() > 5, "{tool}");
    }
    let save = tools
        .iter()
        .find(|t| t["name"] == "save_io_config")
        .unwrap();
    assert_eq!(
        save["inputSchema"]["required"],
        json!(["project_id", "body"])
    );
    assert_eq!(save["annotations"]["destructiveHint"], true);

    let unknown = rig.rpc("tools/call", json!({"name": "no_such_tool"})).await;
    assert_eq!(unknown["error"]["code"], -32602);
    let method = rig.rpc("resources/list", json!({})).await;
    assert_eq!(method["error"]["code"], -32601);

    // A web page elsewhere cannot drive the controller through a browser on this machine.
    let (status, _) = rig
        .post(
            json!({"jsonrpc": "2.0", "id": 1, "method": "ping"}),
            Some("http://evil.example"),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = rig
        .post(
            json!({"jsonrpc": "2.0", "id": 1, "method": "ping"}),
            Some("http://localhost:5173"),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    rig.io.shutdown().await;
}

#[tokio::test]
async fn an_agent_can_configure_and_read_a_device() {
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(0, 4242).await;
    let rig = rig().await;

    let (error, status, _) = rig
        .tool(
            "create_project",
            json!({"body": {"id": "plant", "name": "Plant", "description": "d", "author": "agent"}}),
        )
        .await;
    assert!(!error, "status {status}");
    let (_, _, body) = rig
        .tool("activate_project", json!({"project_id": "plant"}))
        .await;
    assert!(body.is_object(), "{body}");
    let (_, _, active) = rig.tool("get_active_project", json!({})).await;
    assert_eq!(active["project_id"], "plant");

    let connection = |slave_id: u8| {
        json!({"connections": [{
            "id": "plc1", "driver": "modbus", "poll_interval_ms": 50,
            "config": {"transport": "tcp", "host": "127.0.0.1", "port": sim.port(),
                "slave_id": slave_id, "timeout_ms": 300,
                "mappings": [{"uri": PRESSURE, "address": 0, "function_code": 3}]}
        }]})
    };

    // A configuration the driver refuses comes back as a tool error naming the problem.
    let (error, status, body) = rig
        .tool(
            "save_io_config",
            json!({"project_id": "plant", "body": connection(0)}),
        )
        .await;
    assert!(error);
    assert_eq!(status, 400);
    assert!(
        body["problems"][0]["message"]
            .as_str()
            .unwrap()
            .contains("1..=247"),
        "{body}"
    );

    let (error, _, body) = rig
        .tool(
            "save_io_config",
            json!({"project_id": "plant", "body": connection(1)}),
        )
        .await;
    assert!(!error, "{body}");
    assert_eq!(body["applied"], true);
    assert_eq!(body["changes"]["started"], json!(["plc1"]));

    // The value the agent reads is the device's, with its quality.
    let mut reading = Value::Null;
    for _ in 0..100 {
        let (_, _, values) = rig
            .tool("read_tag_values", json!({"prefix": "external.BoilerRoom"}))
            .await;
        reading = values["updates"][0].clone();
        if reading["quality_name"] == "GOOD" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    assert_eq!(reading["uri"], PRESSURE, "{reading}");
    assert_eq!(reading["quality_name"], "GOOD", "{reading}");
    assert_eq!(reading["value"], 4242);
    assert_eq!(reading["type"], "UINT");

    let (_, _, none) = rig
        .tool("read_tag_values", json!({"prefix": "external.Nowhere"}))
        .await;
    assert_eq!(none["updates"], json!([]));

    let (_, _, status) = rig.tool("get_io_status", json!({})).await;
    assert_eq!(status["connections"][0]["state"], "RUNNING");
    let (_, _, tag) = rig.tool("get_tag", json!({"uri": PRESSURE})).await;
    assert_eq!(tag["uri"], PRESSURE);

    // Publishing a screen is the HMI module's (ADR-011): `mythos-module-tests/tests/hmi_mcp.rs`.

    let (error, _, modules) = rig.tool("list_modules", json!({})).await;
    assert!(!error);
    assert_eq!(modules["drivers"][0]["name"], "modbus");

    // Arguments that do not fit the tool are refused before anything is sent.
    let (error, _, _) = rig.tool("get_project", json!({})).await;
    assert!(error);

    rig.io.shutdown().await;
}

/// The field names every documented body uses, filled in from the tool's own schema where it
/// names them, sent to the route. A schema that drifts from the route's struct fails here.
#[tokio::test]
async fn the_bodies_the_tools_document_are_what_the_routes_accept() {
    let rig = rig().await;
    let listed = rig.rpc("tools/list", json!({})).await;
    let schema_of = |tool: &str| -> Value {
        listed["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == tool)
            .unwrap()["inputSchema"]["properties"]["body"]
            .clone()
    };
    let required = |schema: &Value| -> Vec<String> {
        schema["required"]
            .as_array()
            .map(|r| r.iter().map(|v| v.as_str().unwrap().to_string()).collect())
            .unwrap_or_default()
    };

    let (error, _, body) = rig
        .tool(
            "create_project",
            json!({"body": {"id": "p", "name": "P", "description": "", "author": "a"}}),
        )
        .await;
    assert!(!error, "{body}");

    // A UDT, with the member field names the schema documents.
    let udt_schema = schema_of("create_datatype");
    assert_eq!(required(&udt_schema), vec!["name", "kind"]);
    let member = &udt_schema["properties"]["members"]["items"];
    assert_eq!(required(member), vec!["name", "dataType"]);
    let udt = json!({"name": "Motor", "kind": "STRUCT",
        "members": [{"name": "running", "dataType": "BOOL"}, {"name": "speed", "dataType": "REAL"}]});
    let (error, _, created) = rig
        .tool("create_datatype", json!({"project_id": "p", "body": udt}))
        .await;
    assert!(!error, "{created}");

    // The faceplate tools are the HMI module's: `mythos-module-tests/tests/hmi_mcp.rs`.

    rig.io.shutdown().await;
}

#[tokio::test]
async fn an_agent_saving_runtimes_it_read_before_another_save_is_refused_with_words() {
    let rig = rig().await;
    let (error, status, _) = rig
        .tool(
            "create_project",
            json!({"body": {"id": "plant", "name": "Plant", "description": "d", "author": "agent"}}),
        )
        .await;
    assert!(!error, "status {status}");

    // Two agents read the project's runtimes: none yet.
    let (error, _, read) = rig
        .tool("get_runtimes_config", json!({"project_id": "plant"}))
        .await;
    assert!(!error, "{read}");
    let revision = read["revision"].as_str().unwrap().to_string();
    assert_eq!(
        revision,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );

    // The first saves with what it read, and is told the revision of what it saved.
    let (error, status, saved) = rig
        .tool(
            "save_runtimes_config",
            json!({"project_id": "plant", "revision": revision, "body": {"instances": []}}),
        )
        .await;
    assert!(!error, "{status} {saved}");
    let now = saved["revision"].as_str().unwrap().to_string();
    assert_ne!(now, revision);

    // The second, from the same read, is refused and nothing it sent is saved.
    let (error, status, refused) = rig
        .tool(
            "save_runtimes_config",
            json!({"project_id": "plant", "revision": revision, "start": "cold", "body": {"instances": []}}),
        )
        .await;
    assert!(error, "{refused}");
    assert_eq!(status, 409);
    assert_eq!(
        refused["error"],
        format!("the project's runtimes have changed since they were read (revision {revision}, now {now}), so nothing was saved: read them again")
    );
    let (_, _, after) = rig
        .tool("get_runtimes_config", json!({"project_id": "plant"}))
        .await;
    assert_eq!(after["revision"], now.as_str());

    rig.io.shutdown().await;
}
