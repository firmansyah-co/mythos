//! A service module (ADR-011) behind the real router with accounts: its routes, the role each
//! declares, its data directory, its events, its MCP tools, and its process dying and being
//! replaced. Runs on Linux, macOS and Windows alike.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use mythos_core::api::{create_router, ApiState};
use mythos_core::auth::{Auth, Role};
use mythos_core::io::{DriverRegistry, IoManager};
use mythos_core::services::ServiceHost;
use mythos_core::storage::{ControllerStorage, ProjectConfig};
use mythos_core::tag::TagCatalog;
use mythos_pal::DefaultShm;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tower::util::ServiceExt;

const NOTES: &str = env!("CARGO_BIN_EXE_service-module-under-test");
const FIXTURE: &str = env!("CARGO_BIN_EXE_module-fixture");
static SEQ: AtomicU32 = AtomicU32::new(0);

fn exe_name(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

/// Install `source` as the notes module in `dir`, the way an installer does (see modbus_module).
fn install(dir: &Path, source: &Path) {
    static OLD: AtomicU32 = AtomicU32::new(0);
    let target = dir.join(exe_name("mythos-module-notes"));
    let staged = dir.join(".staged");
    std::fs::copy(source, &staged).unwrap();
    if target.exists() {
        let old = dir.join(format!(
            "replaced-{}.old",
            OLD.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::rename(&target, old).unwrap();
    }
    std::fs::rename(&staged, &target).unwrap();
}

struct Rig {
    app: Router,
    io: Arc<IoManager<DefaultShm>>,
    services: Arc<ServiceHost>,
    events: tokio::sync::broadcast::Sender<mythos_core::api::state::HmiEvent>,
    root: PathBuf,
    modules: tempfile::TempDir,
    admin: String,
    engineer: String,
    viewer: String,
    _root: tempfile::TempDir,
}

/// A server as `server::assemble` builds one, with the notes module in its module directory.
async fn rig() -> Rig {
    let root_dir = tempfile::tempdir().unwrap();
    let root = root_dir.path().join("projects");
    let storage = Arc::new(ControllerStorage::new(&root).unwrap());
    storage
        .create_project(&ProjectConfig::new("plant", "Plant"))
        .unwrap();
    let modules = tempfile::tempdir().unwrap();
    install(modules.path(), Path::new(NOTES));

    let auth = Arc::new(Auth::open(&storage.system_dir()).unwrap());
    let code = auth.setup_code().unwrap().to_string();
    let (_, admin) = auth.set_up(&code, "root", "", "root password 1").unwrap();
    for (name, role) in [("erin", Role::Engineer), ("vera", Role::Viewer)] {
        auth.create_user(name, "", role, "a good password").unwrap();
    }
    let (_, engineer) = auth.sign_in("erin", "a good password").unwrap();
    let (_, viewer) = auth.sign_in("vera", "a good password").unwrap();

    let catalog = Arc::new(RwLock::new(TagCatalog::new()));
    let dirs = vec![modules.path().to_path_buf()];
    let io = IoManager::<DefaultShm>::with_module_dirs(
        Arc::clone(&storage),
        Arc::clone(&catalog),
        format!(
            "mythos_service_test_{}_{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        ),
        16,
        DriverRegistry::discover(&dirs).await,
        dirs,
    );
    let state = ApiState::<DefaultShm>::new(storage, catalog, None);
    let services = ServiceHost::new(state.hmi_events.clone());
    services.sync(io.service_modules()).await;
    {
        let services = Arc::clone(&services);
        io.when_services_change(Arc::new(move |modules| {
            let services = Arc::clone(&services);
            tokio::spawn(async move { services.sync(modules).await });
        }));
    }
    let events = state.hmi_events.clone();
    let state = state
        .with_io_manager(Arc::clone(&io))
        .with_auth(auth)
        .with_services(Arc::clone(&services));
    Rig {
        app: create_router(state),
        io,
        services,
        events,
        root,
        modules,
        admin,
        engineer,
        viewer,
        _root: root_dir,
    }
}

impl Rig {
    async fn send(
        &self,
        session: &str,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::COOKIE, format!("mythos_session={session}"));
        let request = match body {
            Some(b) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string())),
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

    async fn bearer(
        &self,
        session: &str,
        method: &str,
        uri: &str,
        body: Value,
    ) -> (StatusCode, Value) {
        let (_, made) = self
            .send(
                session,
                "POST",
                "/api/v1/auth/me/tokens",
                Some(json!({"name": "mcp"})),
            )
            .await;
        let token = made["secret"].as_str().unwrap().to_string();
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
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

    async fn pid(&self) -> u64 {
        let (status, pid) = self
            .send(&self.viewer, "GET", "/api/v1/m/notes/pid", None)
            .await;
        assert_eq!(status, StatusCode::OK, "{pid}");
        pid.as_u64().unwrap()
    }
}

const NOTE: &str = "/api/v1/projects/plant/notes/notes/boiler";

#[tokio::test(flavor = "multi_thread")]
async fn each_route_needs_the_role_its_module_declares_and_works_in_the_projects_directory() {
    let rig = rig().await;
    let (status, list) = rig
        .send(
            &rig.viewer,
            "GET",
            "/api/v1/projects/plant/notes/notes",
            None,
        )
        .await;
    assert_eq!((status, list), (StatusCode::OK, json!([])));

    let (status, body) = rig
        .send(
            &rig.viewer,
            "PUT",
            NOTE,
            Some(json!({"text": "check the burner"})),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(body["error"].as_str().unwrap().contains("engineer"));
    assert!(!rig
        .root
        .join("plant")
        .join("notes")
        .join("boiler.txt")
        .exists());

    let (status, _) = rig
        .send(
            &rig.engineer,
            "PUT",
            NOTE,
            Some(json!({"text": "check the burner"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        std::fs::read_to_string(rig.root.join("plant").join("notes").join("boiler.txt")).unwrap(),
        "check the burner",
        "the module keeps its data in <project>/<its name>/"
    );
    let (_, note) = rig.send(&rig.viewer, "GET", NOTE, None).await;
    assert_eq!(note["text"], "check the burner");

    // A route the module does not declare, a project that does not exist, a module nobody has.
    assert_eq!(
        rig.send(&rig.admin, "DELETE", NOTE, None).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        rig.send(
            &rig.viewer,
            "GET",
            "/api/v1/projects/nowhere/notes/notes",
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        rig.send(&rig.viewer, "GET", "/api/v1/projects/plant/nothing/x", None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );

    // A route that declares no role is for administrators only, and the module is told who asks.
    assert_eq!(
        rig.send(&rig.engineer, "GET", "/api/v1/m/notes/whoami", None)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let (status, me) = rig
        .send(&rig.admin, "GET", "/api/v1/m/notes/whoami", None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me, json!({"username": "root", "role": "admin"}));

    // Nobody signed in reaches no module.
    let anonymous = rig
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/projects/plant/notes/notes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_path_parameter_cannot_reach_outside_the_modules_directory() {
    let rig = rig().await;
    for note in ["..", "..%2F..%2Fescaped", "a%5Cb"] {
        let uri = format!("/api/v1/projects/plant/notes/notes/{note}");
        let (status, body) = rig
            .send(&rig.engineer, "PUT", &uri, Some(json!({"text": "x"})))
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{note}: {body}");
    }
    // An encoded separator becomes two segments, which no declared route has.
    let (status, _) = rig
        .send(
            &rig.engineer,
            "PUT",
            "/api/v1/projects/plant/notes/notes/a%2Fb",
            Some(json!({"text": "x"})),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let written: Vec<_> = std::fs::read_dir(rig.root.join("plant").join("notes"))
        .map(|d| d.flatten().map(|e| e.file_name()).collect())
        .unwrap_or_default();
    assert!(written.is_empty(), "nothing may be written: {written:?}");
    assert!(!rig.root.join("escaped.txt").exists());
    assert!(!rig.root.join("plant").join("escaped.txt").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_modules_events_reach_the_telemetry_stream() {
    let rig = rig().await;
    let mut events = rig.events.subscribe();
    rig.send(&rig.engineer, "PUT", NOTE, Some(json!({"text": "x"})))
        .await;
    let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event.event, "notes_changed");
    assert_eq!(event.data, json!({"project": "plant", "note": "boiler"}));
}

#[tokio::test(flavor = "multi_thread")]
async fn each_declared_route_is_an_mcp_tool_that_runs_with_the_callers_role() {
    let rig = rig().await;
    let (_, list) = rig
        .bearer(
            &rig.viewer,
            "POST",
            "/mcp",
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
        )
        .await;
    let tools = list["result"]["tools"].as_array().unwrap();
    let write = tools
        .iter()
        .find(|t| t["name"] == "write_note")
        .expect("write_note is listed");
    assert_eq!(
        write["inputSchema"]["required"],
        json!(["project_id", "note", "body"])
    );
    assert!(write["description"].as_str().unwrap().contains("engineer"));

    let call = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
        "name": "write_note",
        "arguments": {"project_id": "plant", "note": "pump", "body": {"text": "seal leaks"}}
    }});
    let (_, refused) = rig.bearer(&rig.viewer, "POST", "/mcp", call.clone()).await;
    assert_eq!(
        refused["result"]["structuredContent"]["status"], 403,
        "{refused}"
    );
    let (_, done) = rig.bearer(&rig.engineer, "POST", "/mcp", call).await;
    assert_eq!(done["result"]["structuredContent"]["status"], 200, "{done}");
    assert!(rig
        .root
        .join("plant")
        .join("notes")
        .join("pump.txt")
        .exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_module_process_that_dies_is_started_again() {
    let rig = rig().await;
    let first = rig.pid().await;
    let (status, _) = rig
        .send(&rig.viewer, "POST", "/api/v1/m/notes/crash", None)
        .await;
    assert_eq!(
        status,
        StatusCode::BAD_GATEWAY,
        "the request it died on is answered, as an error"
    );
    let second = rig.pid().await;
    assert_ne!(first, second, "a new process serves the next request");
    let (_, modules) = rig.send(&rig.viewer, "GET", "/api/v1/modules", None).await;
    assert_eq!(modules["services"][0]["starts"], 2, "{modules}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_build_is_swapped_in_and_a_removed_module_stops_serving() {
    let rig = rig().await;
    let before = rig.pid().await;
    install(rig.modules.path(), Path::new(NOTES));
    rig.io.rescan().await;
    let deadline = Instant::now() + Duration::from_secs(15);
    let after = loop {
        let pid = rig.pid().await;
        if pid != before {
            break pid;
        }
        assert!(Instant::now() < deadline, "the new build never took over");
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_ne!(after, before);
    // The data survives the swap: it is in files the module owns.
    rig.send(&rig.engineer, "PUT", NOTE, Some(json!({"text": "kept"})))
        .await;

    std::fs::remove_file(rig.modules.path().join(exe_name("mythos-module-notes"))).unwrap();
    rig.io.rescan().await;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let (status, _) = rig.send(&rig.viewer, "GET", NOTE, None).await;
        if status == StatusCode::NOT_FOUND && rig.services.get("notes").is_none() {
            break;
        }
        assert!(Instant::now() < deadline, "the removed module still serves");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(
        std::fs::read_to_string(rig.root.join("plant").join("notes").join("boiler.txt")).unwrap(),
        "kept",
        "removing a module leaves its data"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_build_that_cannot_start_leaves_the_running_one_serving() {
    let rig = rig().await;
    let before = rig.pid().await;

    // A "new build" that says it is the notes module but exits as soon as it is started.
    let output = std::process::Command::new(NOTES)
        .arg("--manifest")
        .output()
        .unwrap();
    let manifest = String::from_utf8(output.stdout).unwrap();
    let broken_dir = tempfile::tempdir().unwrap();
    let broken = broken_dir.path().join(exe_name("broken"));
    std::fs::copy(FIXTURE, &broken).unwrap();
    let mut sidecar = broken.as_os_str().to_owned();
    sidecar.push(".fixture.json");
    std::fs::write(
        &sidecar,
        json!({"manifest": manifest, "manifest_exit": 0, "serve_exit": 1}).to_string(),
    )
    .unwrap();
    let mut installed_sidecar = rig
        .modules
        .path()
        .join(exe_name("mythos-module-notes"))
        .into_os_string();
    installed_sidecar.push(".fixture.json");
    std::fs::copy(&sidecar, installed_sidecar).unwrap();
    install(rig.modules.path(), &broken);
    rig.io.rescan().await;
    tokio::time::sleep(Duration::from_secs(3)).await;

    assert_eq!(rig.pid().await, before, "the running build kept serving");
}
