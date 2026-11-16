//! The discovery module as the server runs it (ADR-012 §2.2): always on, told its node, it
//! announces an edge and not a workstation, follows a rename, and lists other nodes as
//! candidates.
//!
//! Hearing another node needs multicast on the machine's network, which a container may not
//! have: that part runs when MYTHOS_TEST_MDNS=1 (CI's Windows and macOS runners, and by hand).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use mythos_core::api::{create_router, ApiState};
use mythos_core::node::{Node, NodeChange, NodeRole};
use mythos_core::services::ServiceHost;
use mythos_core::storage::ControllerStorage;
use mythos_core::tag::TagCatalog;
use mythos_module::host::{read_manifest, Fingerprint};
use mythos_module::ModuleBinary;
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tower::ServiceExt;

const DISCOVERY: &str = env!("CARGO_BIN_EXE_discovery-module-under-test");

struct Station {
    app: Router,
    node: Arc<Node>,
    services: Arc<ServiceHost>,
    _dir: tempfile::TempDir,
}

async fn station(role: NodeRole, port: u16) -> Station {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(dir.path()).unwrap());
    let node = Arc::new(Node::open(&storage.system_dir(), role).unwrap());
    node.set_listening(format!("0.0.0.0:{port}").parse().unwrap());
    let path = Path::new(DISCOVERY);
    let module = Arc::new(ModuleBinary {
        path: path.to_path_buf(),
        manifest: read_manifest(path).await.unwrap(),
        fingerprint: Fingerprint::of(path).unwrap(),
    });
    let state = ApiState::<mythos_pal::MemoryShm>::new(
        storage,
        Arc::new(RwLock::new(TagCatalog::new())),
        None,
    )
    .with_node(Arc::clone(&node));
    let services = ServiceHost::new(state.hmi_events.clone());
    {
        let node = Arc::clone(&node);
        services.set_node(Arc::new(move || Some(node.wire())));
    }
    services.sync(vec![module]).await;
    Station {
        app: create_router(state.with_services(Arc::clone(&services))),
        node,
        services,
        _dir: dir,
    }
}

impl Station {
    async fn nodes(&self) -> Value {
        let response = self
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/m/discovery/nodes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    /// Wait until `check` holds for what the module says, or say what it said last.
    async fn until(
        &self,
        within: Duration,
        check: impl Fn(&Value) -> bool,
    ) -> Result<Value, Value> {
        let deadline = Instant::now() + within;
        loop {
            let said = self.nodes().await;
            if check(&said) {
                return Ok(said);
            }
            if Instant::now() > deadline {
                return Err(said);
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn it_runs_unasked_knows_its_node_and_announces_only_what_the_node_says() {
    let edge = station(NodeRole::Edge, 18501).await;
    // Always on: its process runs before any request was made (a request would start it).
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let statuses = edge.services.statuses().await;
        let running = statuses
            .iter()
            .any(|s| s.provides == "discovery" && s.pid.is_some());
        if running {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the discovery module was not started unasked: {statuses:?}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let id = edge.node.identity().id;
    let said = edge
        .until(Duration::from_secs(10), |v| {
            v["self"] == Value::String(id.clone())
        })
        .await
        .unwrap_or_else(|v| panic!("the module was not told its node: {v}"));
    assert_eq!(said["listening"], true, "{said}");
    assert_eq!(said["announcing"], true, "an edge announces itself: {said}");

    // Told through the API not to announce, it stops: the server tells the module.
    let response = edge
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/node")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"announce": false}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    edge.until(Duration::from_secs(5), |v| v["announcing"] == false)
        .await
        .unwrap_or_else(|v| panic!("it went on announcing: {v}"));

    let workstation = station(NodeRole::Workstation, 18502).await;
    let said = workstation
        .until(Duration::from_secs(10), |v| v["self"].is_string())
        .await
        .unwrap();
    assert_eq!(
        said["announcing"], false,
        "a workstation does not announce: {said}"
    );
    workstation.services.shutdown().await;
    edge.services.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn two_nodes_hear_each_other_as_candidates() {
    if std::env::var("MYTHOS_TEST_MDNS").as_deref() != Ok("1") {
        eprintln!(
            "SKIP two_nodes_hear_each_other_as_candidates: needs multicast; set MYTHOS_TEST_MDNS=1 \
             where the network has it (CI's Windows and macOS runners)"
        );
        return;
    }
    let a = station(NodeRole::Edge, 18511).await;
    let b = station(NodeRole::Edge, 18512).await;
    let b_id = b.node.identity().id;
    let b_name = b.node.identity().name;
    let heard = a
        .until(Duration::from_secs(30), |v| {
            v["nodes"]
                .as_array()
                .is_some_and(|nodes| nodes.iter().any(|n| n["id"] == Value::String(b_id.clone())))
        })
        .await
        .unwrap_or_else(|v| panic!("node a did not hear node b ({b_id}): {v}"));
    let node = heard["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == Value::String(b_id.clone()))
        .unwrap()
        .clone();
    assert_eq!(node["name"], Value::String(b_name), "{node}");
    assert_eq!(node["role"], "edge");
    assert_eq!(node["port"], 18512);
    assert_eq!(node["candidate"], true, "heard is never trusted");
    assert!(
        heard["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|n| n["id"] != heard["self"]),
        "a node does not list itself: {heard}"
    );

    // b stops announcing: a forgets it.
    b.node
        .update(NodeChange {
            announce: Some(Some(false)),
            ..Default::default()
        })
        .unwrap();
    b.services.node_changed().await;
    a.until(Duration::from_secs(30), |v| {
        v["nodes"]
            .as_array()
            .is_some_and(|nodes| nodes.iter().all(|n| n["id"] != Value::String(b_id.clone())))
    })
    .await
    .unwrap_or_else(|v| panic!("node a still lists node b after it stopped announcing: {v}"));
    a.services.shutdown().await;
    b.services.shutdown().await;
}
