//! ADR-012 §2.1: whether a node reads field devices follows its role (a server reads none) or
//! its own setting, and a change applies at once, with the reason said where the connections
//! would be. Run against the real router, I/O manager, tag bus segment and a Modbus simulator.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use driver_modbus::sim::ModbusSimServer;
use mythos_core::api::{create_router, ApiState};
use mythos_core::io::{DriverRegistry, IoManager};
use mythos_core::node::{Node, NodeRole};
use mythos_core::storage::{ControllerStorage, ProjectConfig};
use mythos_core::tag::TagCatalog;
use mythos_pal::DefaultShm;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tower::ServiceExt;

static SEQ: AtomicU32 = AtomicU32::new(0);

async fn send(app: &Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(uri);
    let request = match body {
        Some(body) => {
            request = request.header(header::CONTENT_TYPE, "application/json");
            request.body(Body::from(body.to_string()))
        }
        None => request.body(Body::empty()),
    }
    .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn connection_state(app: &Router) -> (Option<String>, Option<String>) {
    let (_, status) = send(app, "GET", "/api/v1/io/status", None).await;
    (
        status["connections"][0]["state"]
            .as_str()
            .map(str::to_string),
        status["note"].as_str().map(str::to_string),
    )
}

async fn until_running(app: &Router) -> (Option<String>, Option<String>) {
    let mut seen = (None, None);
    for _ in 0..100 {
        seen = connection_state(app).await;
        if seen.0.as_deref() == Some("RUNNING") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    seen
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_reads_no_field_devices_unless_told_to_and_says_why() {
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(dir.path()).unwrap());
    storage
        .create_project(&ProjectConfig::new("plant", "Plant"))
        .unwrap();
    storage.activate_project("plant").unwrap();
    let segment = format!(
        "mythos_node_io_{}_{}",
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
        64,
        registry,
    );
    let node = Arc::new(Node::open(&storage.system_dir(), NodeRole::Edge).unwrap());
    let app = create_router(
        ApiState::<DefaultShm>::new(Arc::clone(&storage), catalog, None)
            .with_io_manager(Arc::clone(&io))
            .with_node(node),
    );

    let config = json!({"connections": [{
        "id": "plc1", "driver": "modbus", "poll_interval_ms": 50,
        "config": {"transport": "tcp", "host": "127.0.0.1", "port": sim.port(), "slave_id": 1,
            "timeout_ms": 300, "mappings": [{"uri": "external.Boiler.U1.p", "address": 0, "function_code": 3}]}
    }]});
    let (status, body) = send(
        &app,
        "PUT",
        "/api/v1/projects/plant/io",
        Some(config.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        until_running(&app).await.0.as_deref(),
        Some("RUNNING"),
        "an edge reads the field"
    );

    // Made a server: its connections stop at once, and the status says why instead.
    let (status, node) = send(&app, "PUT", "/api/v1/node", Some(json!({"role": "server"}))).await;
    assert_eq!(status, StatusCode::OK, "{node}");
    assert_eq!(node["field_io"], json!({"on": false, "from_role": true}));
    let (state, note) = connection_state(&app).await;
    assert_eq!(state, None, "no connection runs on a server");
    assert!(note.unwrap_or_default().contains("its role is server"));

    // Saving the configuration again does not start it behind the role's back.
    let (status, body) = send(&app, "PUT", "/api/v1/projects/plant/io", Some(config)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(connection_state(&app).await.0, None);

    // A server told to read the field does, at once.
    let (_, node) = send(&app, "PUT", "/api/v1/node", Some(json!({"field_io": true}))).await;
    assert_eq!(node["field_io"], json!({"on": true, "from_role": false}));
    assert_eq!(until_running(&app).await.0.as_deref(), Some("RUNNING"));

    // Back to what the role says.
    let (_, node) = send(&app, "PUT", "/api/v1/node", Some(json!({"field_io": null}))).await;
    assert_eq!(node["field_io"], json!({"on": false, "from_role": true}));
    assert_eq!(connection_state(&app).await.0, None);

    io.shutdown().await;
    let _ = DefaultShm::unlink(&segment);
}
