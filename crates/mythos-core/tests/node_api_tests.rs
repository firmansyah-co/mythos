//! `/api/v1/node` (ADR-012 §2.1): anyone signed in sees what this node is; only an
//! administrator renames it or changes its role, and a role whose listening address differs
//! from where the server listens says a restart is needed.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use mythos_core::api::{create_router, ApiState};
use mythos_core::auth::{Auth, Role};
use mythos_core::node::{Node, NodeRole};
use mythos_core::storage::ControllerStorage;
use mythos_core::tag::TagCatalog;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::util::ServiceExt;

struct Rig {
    app: Router,
    node: Arc<Node>,
    system: std::path::PathBuf,
    admin: String,
    viewer: String,
    _dir: tempfile::TempDir,
}

fn rig() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(dir.path().join("projects")).unwrap());
    let system = storage.system_dir();
    let auth = Arc::new(Auth::open(&system).unwrap());
    let code = auth.setup_code().unwrap().to_string();
    let (_, admin) = auth.set_up(&code, "root", "", "root password 1").unwrap();
    auth.create_user("vera", "", Role::Viewer, "a good password")
        .unwrap();
    let (_, viewer) = auth.sign_in("vera", "a good password").unwrap();
    let node = Arc::new(Node::open(&system, NodeRole::Edge).unwrap());
    // As mythos-server does once it is listening.
    node.set_listening("127.0.0.1:8080".parse().unwrap());
    node.set_listening_tls("0.0.0.0:8443".parse().unwrap(), "ab".repeat(32));
    let state = ApiState::<mythos_pal::MemoryShm>::new(
        storage,
        Arc::new(RwLock::new(TagCatalog::new())),
        None,
    )
    .with_auth(auth)
    .with_node(Arc::clone(&node));
    Rig {
        app: create_router(state),
        node,
        system,
        admin,
        viewer,
        _dir: dir,
    }
}

impl Rig {
    async fn send(
        &self,
        session: Option<&str>,
        method: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut request = Request::builder().method(method).uri("/api/v1/node");
        if let Some(session) = session {
            request = request.header(header::COOKIE, format!("mythos_session={session}"));
        }
        let request = match body {
            Some(body) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        }
        .unwrap();
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
}

#[tokio::test]
async fn anyone_signed_in_sees_the_node_and_nobody_else_does() {
    let rig = rig();
    assert_eq!(
        rig.send(None, "GET", None).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (status, node) = rig.send(Some(&rig.viewer), "GET", None).await;
    assert_eq!(status, StatusCode::OK, "{node}");
    let identity = rig.node.identity();
    assert_eq!(node["id"], identity.id);
    assert_eq!(node["name"], identity.name);
    assert_eq!(node["role"], "edge");
    assert_eq!(
        node["listening"], "127.0.0.1:8080",
        "plain HTTP on this computer only"
    );
    assert_eq!(node["listen_address"], "127.0.0.1");
    assert_eq!(
        node["https"]["listening"], "0.0.0.0:8443",
        "HTTPS on the network"
    );
    assert_eq!(node["https"]["fingerprint"], "ab".repeat(32));
    assert_eq!(node["http_on_network"], false);
    assert_eq!(node["restart_required"], false);
    assert_eq!(node["module_protocol"], mythos_module::PROTOCOL_VERSION);
}

#[tokio::test]
async fn only_an_administrator_changes_it_and_a_new_listening_address_waits_for_a_restart() {
    let rig = rig();
    let change = json!({"name": "Boiler edge 1", "site": "Boiler house", "role": "workstation"});
    let (status, _) = rig
        .send(Some(&rig.viewer), "PUT", Some(change.clone()))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        rig.node.identity().role,
        NodeRole::Edge,
        "a refused change changes nothing"
    );

    let (status, node) = rig.send(Some(&rig.admin), "PUT", Some(change)).await;
    assert_eq!(status, StatusCode::OK, "{node}");
    assert_eq!(
        (&node["name"], &node["site"], &node["role"]),
        (
            &json!("Boiler edge 1"),
            &json!("Boiler house"),
            &json!("workstation")
        )
    );
    // A workstation answers HTTPS on this computer only, but this run still does everywhere.
    assert_eq!(node["https"]["listen_address"], "127.0.0.1");
    assert_eq!(node["https"]["listening"], "0.0.0.0:8443");
    assert_eq!(node["restart_required"], true);

    let stored: Value =
        serde_json::from_slice(&std::fs::read(rig.system.join("node.json")).unwrap()).unwrap();
    assert_eq!(stored["role"], "workstation");
    assert_eq!(stored["id"], node["id"], "the id never changes");
}

#[tokio::test]
async fn a_change_that_is_not_valid_is_refused_whole() {
    let rig = rig();
    let before = rig.node.identity();
    for change in [
        json!({"role": "plc"}),
        json!({"name": "   "}),
        json!({"name": "fine", "role": "plc"}),
    ] {
        let (status, body) = rig
            .send(Some(&rig.admin), "PUT", Some(change.clone()))
            .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{change} -> {body}"
        );
        assert!(body["error"].is_string(), "{body}");
    }
    assert_eq!(rig.node.identity(), before);
}
