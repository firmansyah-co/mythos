//! The data module and the logic module together, as the server runs them (ADR-014 §2.8):
//! a user type and a data block made in the data module, programs that declare the block in
//! `VAR_EXTERNAL` built by the logic module, and what was built run by the real runtime module
//! against a Modbus simulator, the value passing from one program to the other through the
//! block.
//!
//! A program running alone is built as one runtime instance, and the core refuses a block in two
//! runtimes. The logic module builds one instance of several programs from a runtime configured
//! in it (ADR-013 §2.19, C8c): the tests at the end save that instance as it is answered. The
//! three tests before them compose such an instance by hand from two program builds, as the
//! core's own checks of a block shared in one runtime need no logic module.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use driver_modbus::sim::ModbusSimServer;
use mythos_core::api::{create_router, ApiState};
use mythos_core::auth::{Auth, Role};
use mythos_core::io::{DriverRegistry, IoManager};
use mythos_core::services::ServiceHost;
use mythos_core::storage::{ControllerStorage, ProjectConfig};
use mythos_core::tag::TagCatalog;
use mythos_module::host::{read_manifest, Fingerprint};
use mythos_module::ModuleBinary;
use mythos_pal::DefaultShm;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tower::util::ServiceExt;

const MODBUS: &str = env!("CARGO_BIN_EXE_modbus-module-under-test");
const RUNTIME_VM: &str = env!("CARGO_BIN_EXE_runtime-vm-module-under-test");
const LOGIC: &str = env!("CARGO_BIN_EXE_logic-module-under-test");
const DATA: &str = env!("CARGO_BIN_EXE_data-module-under-test");
const HMI: &str = env!("CARGO_BIN_EXE_hmi-module-under-test");
static SEQ: AtomicU32 = AtomicU32::new(0);

const LEVEL: &str = "external.Plant.LT1.level";
const SPEED: &str = "external.Plant.P1.speed";
const PROGRAMS: &str = "/api/v1/projects/plant/logic/programs";
const TYPES: &str = "/api/v1/projects/plant/data/types";
const BLOCKS: &str = "/api/v1/projects/plant/data/blocks";

const SHARED: &str = "TYPE Shared : STRUCT
  level : INT;
  spare : INT := 5;
END_STRUCT; END_TYPE
";

/// Writes `S.level` from the level tag; nothing else of S.
const WRITER: &str = "PROGRAM Writer
VAR_INPUT level : INT; level_ok : BOOL; END_VAR
VAR_EXTERNAL S : Shared; END_VAR
  IF level_ok THEN S.level := level; END_IF;
END_PROGRAM";

/// Drives the speed from `S.level` and `S.spare`, which no program writes.
const READER: &str = "PROGRAM Reader
VAR_OUTPUT speed : INT; END_VAR
VAR_EXTERNAL S : Shared; END_VAR
  speed := S.level * 10 + S.spare;
END_PROGRAM";

async fn module_at(path: &Path) -> Arc<ModuleBinary> {
    Arc::new(ModuleBinary {
        path: path.to_path_buf(),
        manifest: read_manifest(path)
            .await
            .expect("the module prints its manifest"),
        fingerprint: Fingerprint::of(path).unwrap(),
    })
}

/// The tag bus segment of one test, removed when the test ends.
struct Segment(String);

impl Drop for Segment {
    fn drop(&mut self) {
        let _ = DefaultShm::unlink(&self.0);
    }
}

struct Rig {
    app: Router,
    io: Arc<IoManager<DefaultShm>>,
    services: Arc<ServiceHost>,
    sim: ModbusSimServer,
    engineer: String,
    project: PathBuf,
    _root: tempfile::TempDir,
    _modules: tempfile::TempDir,
    _segment: Segment,
}

async fn rig() -> Rig {
    let root_dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(root_dir.path().join("projects")).unwrap());
    storage
        .create_project(&ProjectConfig::new("plant", "plant"))
        .unwrap();
    storage.switch_active_project("plant").unwrap();
    let project = storage.project_dir("plant").unwrap();

    let modules = tempfile::tempdir().unwrap();
    for (source, name) in [
        (MODBUS, "mythos-module-modbus"),
        (RUNTIME_VM, "mythos-module-runtime-vm"),
    ] {
        std::fs::copy(
            source,
            modules
                .path()
                .join(format!("{name}{}", std::env::consts::EXE_SUFFIX)),
        )
        .unwrap();
    }

    let auth = Arc::new(Auth::open(&storage.system_dir()).unwrap());
    let code = auth.setup_code().unwrap().to_string();
    auth.set_up(&code, "root", "", "root password 1").unwrap();
    auth.create_user("erin", "", Role::Engineer, "a good password")
        .unwrap();
    let engineer = auth.sign_in("erin", "a good password").unwrap().1;

    let catalog = Arc::new(RwLock::new(TagCatalog::new()));
    let dirs = vec![modules.path().to_path_buf()];
    let shm = format!(
        "mythos_data_logic_test_{}_{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::SeqCst)
    );
    let io = IoManager::<DefaultShm>::with_module_dirs(
        Arc::clone(&storage),
        Arc::clone(&catalog),
        shm.clone(),
        32,
        DriverRegistry::discover(&dirs).await,
        dirs,
    );
    let state = ApiState::<DefaultShm>::new(storage, catalog, None)
        .with_io_manager(Arc::clone(&io))
        .with_auth(auth);

    let services = ServiceHost::new(state.hmi_events.clone());
    services
        .sync(vec![
            module_at(Path::new(LOGIC)).await,
            module_at(Path::new(DATA)).await,
        ])
        .await;

    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_input(0, 20).await;
    sim.set_holding(1, 999).await;
    Rig {
        app: create_router(state.with_services(Arc::clone(&services))),
        io,
        services,
        sim,
        engineer,
        project,
        _root: root_dir,
        _modules: modules,
        _segment: Segment(shm),
    }
}

impl Rig {
    /// As the engineer, expecting `expected`.
    async fn eng(&self, method: &str, uri: &str, body: Option<Value>, expected: u16) -> Value {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::COOKIE, format!("mythos_session={}", self.engineer));
        let request = match body {
            Some(b) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string())),
            None => request.body(Body::empty()),
        }
        .unwrap();
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 22)
            .await
            .unwrap();
        let answer = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        assert_eq!(status.as_u16(), expected, "{method} {uri}: {answer}");
        answer
    }

    fn io_config(&self) -> Value {
        json!({"connections": [{
            "id": "plc", "driver": "modbus", "poll_interval_ms": 20,
            "config": {"transport": "tcp", "host": "127.0.0.1", "port": self.sim.port(),
                       "timeout_ms": 300, "mappings": [
                {"uri": LEVEL, "address": 0, "function_code": 4, "data_type": "i16"},
                {"uri": SPEED, "address": 1, "function_code": 3}
            ]}
        }]})
    }

    /// The type `Shared` and the block `S` (not retained), made in the data module.
    async fn shared(&self) -> Value {
        self.eng(
            "PUT",
            &format!("{TYPES}/Shared"),
            Some(json!({ "source": SHARED })),
            200,
        )
        .await;
        self.eng(
            "PUT",
            &format!("{BLOCKS}/S"),
            Some(json!({ "type": "Shared", "retain": false })),
            200,
        )
        .await
    }

    /// A program made in the logic module with `source` and `settings`.
    async fn program(&self, name: &str, source: &str, settings: Value) {
        self.eng("POST", PROGRAMS, Some(json!({ "name": name })), 201)
            .await;
        self.eng(
            "PUT",
            &format!("{PROGRAMS}/{name}/source"),
            Some(json!({ "source": source })),
            200,
        )
        .await;
        self.eng(
            "PUT",
            &format!("{PROGRAMS}/{name}/settings"),
            Some(settings),
            200,
        )
        .await;
    }

    async fn writer_and_reader(&self) {
        self.program(
            "Writer",
            WRITER,
            json!({"interval_ms": 20, "runtime": "vm", "target": "pc", "run": true,
                   "inputs": [{"variable": "level", "tag": LEVEL},
                              {"variable": "level_ok", "tag": LEVEL, "of": "good"}]}),
        )
        .await;
        self.program(
            "Reader",
            READER,
            json!({"interval_ms": 20, "runtime": "vm", "target": "pc", "run": true,
                   "outputs": [{"variable": "speed", "tag": SPEED, "safe": 7}]}),
        )
        .await;
    }

    async fn build(&self, name: &str) -> Value {
        self.eng("POST", &format!("{PROGRAMS}/{name}/build"), None, 200)
            .await
    }

    async fn runtime(&self) -> Value {
        self.eng("GET", "/api/v1/runtimes", None, 200).await["runtimes"][0].clone()
    }

    async fn runtime_when(&self, what: &str, holds: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let runtime = self.runtime().await;
            if holds(&runtime) {
                return runtime;
            }
            assert!(
                Instant::now() < deadline,
                "never: {what}; runtime {runtime}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn speed_is(&self, what: &str, speed: u16) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while self.sim.holding(1).await != Some(speed) {
            assert!(
                Instant::now() < deadline,
                "never: {what}; speed register {:?}, runtime {}",
                self.sim.holding(1).await,
                self.runtime().await
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn close(self) {
        self.io.shutdown().await;
        self.services.shutdown().await;
    }
}

/// The data module's block as a runtime's data block writes it: `retain` absent when false.
fn as_runtime_block(block: &Value) -> Value {
    let mut b = block.clone();
    if b["retain"] == false {
        b.as_object_mut().unwrap().remove("retain");
    }
    b
}

/// An instance of several programs composed by hand from program builds: the programs in one
/// cyclic task, and the blocks either declares, each once. The logic module builds such an
/// instance itself from a configured runtime (`build_logic_runtime`); this is kept for the
/// tests of what the core checks of a block shared inside one runtime.
fn composed(id: &str, built: &[&Value]) -> Value {
    let mut data: Vec<Value> = Vec::new();
    for b in built {
        for block in b["instance"]["data"].as_array().unwrap() {
            if !data.iter().any(|d| d["name"] == block["name"]) {
                data.push(block.clone());
            }
        }
    }
    json!({
        "id": id, "module": "vm", "run": true,
        "programs": built.iter().map(|b| b["instance"]["program"].clone()).collect::<Vec<_>>(),
        "tasks": [{"name": "main", "kind": {"type": "cyclic", "interval_ms": 20}, "priority": 1,
                   "programs": built.iter().map(|b| b["instance"]["program"]["name"].clone()).collect::<Vec<_>>()}],
        "data": data,
    })
}

/// Every file under `root` with its bytes, relative to it.
fn files(root: &Path) -> BTreeSet<(PathBuf, Vec<u8>)> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeSet<(PathBuf, Vec<u8>)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries {
            let path = e.unwrap().path();
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            if path.is_dir() {
                out.insert((relative, Vec::new()));
                walk(root, &path, out);
            } else {
                out.insert((relative, std::fs::read(&path).unwrap()));
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(root, root, &mut out);
    out
}

#[tokio::test(flavor = "multi_thread")]
async fn a_type_and_a_block_made_in_the_data_module_run_in_two_programs_built_by_the_logic_module()
{
    let rig = rig().await;
    rig.shared().await;
    rig.writer_and_reader().await;
    rig.eng(
        "PUT",
        "/api/v1/projects/plant/io",
        Some(rig.io_config()),
        200,
    )
    .await;
    let w = rig.build("Writer").await;
    let r = rig.build("Reader").await;
    // What the logic module built with is what the data module stored and laid out.
    let listed = rig.eng("GET", BLOCKS, None, 200).await;
    assert_eq!(listed["ok"], true, "{listed}");
    assert_eq!(
        w["instance"]["data"],
        json!([as_runtime_block(&listed["blocks"][0])]),
        "{w}"
    );
    assert_eq!(r["instance"]["data"], w["instance"]["data"], "{r}");

    // Composing the instance of two programs is C8c's; the test does it here.
    let applied = rig
        .eng(
            "PUT",
            "/api/v1/projects/plant/runtimes",
            Some(json!({ "instances": [composed("tank", &[&w, &r])] })),
            200,
        )
        .await;
    assert_eq!(
        applied["changes"]["started"],
        json!(["runtime:tank"]),
        "{applied}"
    );
    // Level 20 reaches Reader only through S: 20 * 10 + the unwritten spare's 5.
    rig.speed_is("Reader drives 205 from what Writer handed over", 205)
        .await;
    rig.sim.set_input(0, 31).await;
    rig.speed_is("the speed follows the level, through the block", 315)
        .await;
    let runtime = rig
        .runtime_when("the status names the block", |t| t["data"].is_array())
        .await;
    assert_eq!(
        runtime["data"][0]["members"],
        json!([
            {"name": "level", "type": "INT", "writer": "Writer"},
            {"name": "spare", "type": "INT", "writer": null}
        ]),
        "{runtime}"
    );
    rig.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_single_program_with_a_block_loads_as_built() {
    let rig = rig().await;
    rig.shared().await;
    rig.writer_and_reader().await;
    let built = rig.build("Writer").await;
    assert_eq!(built["instance"]["data"][0]["name"], "S", "{built}");
    assert_eq!(built["instance"]["program"]["data"][0]["name"], "S");
    let applied = rig
        .eng(
            "PUT",
            "/api/v1/projects/plant/runtimes",
            Some(json!({ "instances": [built["instance"]] })),
            200,
        )
        .await;
    assert_eq!(
        applied["changes"]["started"],
        json!(["runtime:Writer"]),
        "{applied}"
    );
    rig.runtime_when("it runs the built version", |t| {
        t["version"] == built["version"] && t["mode"] == "run"
    })
    .await;
    rig.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_changed_type_is_refused_to_a_program_built_before_it() {
    let rig = rig().await;
    rig.shared().await;
    rig.writer_and_reader().await;
    let a = rig.build("Writer").await;
    let changed = rig
        .eng(
            "PUT",
            &format!("{TYPES}/Shared"),
            Some(json!({ "source": SHARED.replace("spare : INT := 5;", "spare : INT := 5; more : INT;") })),
            200,
        )
        .await;
    assert_eq!(changed["changed"], json!(["S"]), "{changed}");
    let b = rig.build("Reader").await;
    assert_ne!(a["instance"]["data"], b["instance"]["data"]);
    let refused = rig
        .eng(
            "PUT",
            "/api/v1/projects/plant/runtimes",
            Some(json!({ "instances": [composed("tank", &[&b, &a])] })),
            400,
        )
        .await;
    let said = refused["error"].as_str().unwrap_or_default();
    assert!(
        said.contains("program 'Writer' was built with data block 'S' laid out as")
            && said.contains("build 'Writer' again"),
        "{refused}"
    );
    rig.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_program_builds_with_the_data_module_not_running() {
    let rig = rig().await;
    rig.shared().await;
    rig.writer_and_reader().await;
    let before = rig.build("Writer").await;
    // The data module is unloaded; its files stay.
    rig.services
        .sync(vec![module_at(Path::new(LOGIC)).await])
        .await;
    assert!(rig.services.get("data").is_none());
    rig.eng("GET", BLOCKS, None, 404).await;
    let after = rig.build("Writer").await;
    assert_eq!(after["instance"]["data"], before["instance"]["data"]);
    assert_eq!(after["version"], before["version"]);
    rig.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_logic_module_writes_nothing_under_the_data_directory() {
    let rig = rig().await;
    rig.shared().await;
    rig.writer_and_reader().await;
    let data = rig.project.join("data");
    let before = files(&data);
    assert!(!before.is_empty());
    rig.eng("POST", &format!("{PROGRAMS}/Writer/check"), None, 200)
        .await;
    rig.build("Writer").await;
    rig.eng(
        "PUT",
        "/api/v1/projects/plant/logic/library/Motors",
        Some(json!({"source": "FUNCTION Twice : INT\nVAR_INPUT x : INT; END_VAR\n  Twice := x * 2;\nEND_FUNCTION\n"})),
        200,
    )
    .await;
    rig.eng(
        "POST",
        "/api/v1/projects/plant/logic/library/check",
        None,
        200,
    )
    .await;
    rig.build("Reader").await;
    assert_eq!(files(&data), before);
    rig.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_tools_of_every_module_have_distinct_names() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(dir.path()).unwrap());
    storage
        .create_project(&ProjectConfig::new("plant", "Plant"))
        .unwrap();
    let state = ApiState::<mythos_pal::MemoryShm>::new(
        storage,
        Arc::new(RwLock::new(TagCatalog::new())),
        None,
    );
    let services = ServiceHost::new(state.hmi_events.clone());
    services
        .sync(vec![
            module_at(Path::new(LOGIC)).await,
            module_at(Path::new(DATA)).await,
            module_at(Path::new(HMI)).await,
        ])
        .await;
    let app = create_router(state.with_services(Arc::clone(&services)));
    let request = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::HOST, "127.0.0.1")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json, text/event-stream")
        .body(Body::from(
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}).to_string(),
        ))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let listed: Value = serde_json::from_slice(&bytes).unwrap();
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    let mut seen = BTreeSet::new();
    let twice: Vec<&str> = names.iter().copied().filter(|n| !seen.insert(*n)).collect();
    assert!(twice.is_empty(), "named twice: {twice:?}");
    // Each module's tools are there: the list is not empty for a wrong reason.
    for name in [
        "build_logic_program",
        "check_logic_library",
        "save_data_type",
        "check_data",
    ] {
        assert!(names.contains(&name), "{name} in {names:?}");
    }
    assert!(services.get("hmi").is_some());
    services.shutdown().await;
}

const RUNTIMES: &str = "/api/v1/projects/plant/logic/runtimes";
const CORE_RUNTIMES: &str = "/api/v1/projects/plant/runtimes";

/// Runtime `Line` as the logic module stores it: Writer in `fast` (priority 1, 20 ms), Reader in
/// `slow` (priority `slow`, 50 ms).
fn line(slow: u8) -> Value {
    json!({ "module": "vm", "target": "pc", "run": true, "tasks": [
        { "name": "fast", "kind": { "type": "cyclic", "interval_ms": 20 }, "priority": 1, "programs": ["Writer"] },
        { "name": "slow", "kind": { "type": "cyclic", "interval_ms": 50 }, "priority": slow, "programs": ["Reader"] }
    ]})
}

impl Rig {
    /// Configure runtime `id` in the logic module and build it.
    async fn build_runtime(&self, id: &str, runtime: Value) -> Value {
        self.eng("PUT", &format!("{RUNTIMES}/{id}"), Some(runtime), 200)
            .await;
        self.eng("POST", &format!("{RUNTIMES}/{id}/build"), None, 200)
            .await
    }

    /// Save a built runtime as a caller of `build_logic_runtime` does: the core's instances
    /// without those named in `replaces` and without the one of the same id, then the answered
    /// instance, unedited.
    async fn load_built(&self, built: &Value, expected: u16) -> Value {
        let config = self.eng("GET", CORE_RUNTIMES, None, 200).await;
        // `null` while the project has no runtimes.
        assert!(
            config
                .get("config")
                .is_some_and(|c| c.is_null() || c["instances"].is_array()),
            "{config}"
        );
        let replaced: Vec<&Value> = built["replaces"].as_array().unwrap().iter().collect();
        let mut instances: Vec<Value> = config["config"]["instances"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|i| i["id"] != built["instance"]["id"] && !replaced.contains(&&i["id"]))
            .collect();
        instances.push(built["instance"].clone());
        self.eng(
            "PUT",
            CORE_RUNTIMES,
            Some(json!({ "instances": instances })),
            expected,
        )
        .await
    }
}

/// The versions the core reports for a runtime of several programs: each program's, in the
/// instance's order.
fn versions_of(built: &Value) -> String {
    built["instance"]["programs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["version"].as_str().unwrap())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Runs the version `built` gave and has scanned at least once.
fn runs(built: &Value) -> impl Fn(&Value) -> bool {
    let version = versions_of(built);
    move |t: &Value| t["version"] == version && t["mode"] == "run" && t["scans"].as_u64() >= Some(1)
}

#[tokio::test(flavor = "multi_thread")]
async fn two_programs_sharing_a_data_block_are_built_and_loaded_by_the_logic_module_alone() {
    let rig = rig().await;
    rig.shared().await;
    rig.writer_and_reader().await;
    rig.eng(
        "PUT",
        "/api/v1/projects/plant/io",
        Some(rig.io_config()),
        200,
    )
    .await;
    let built = rig.build_runtime("Line", line(5)).await;
    assert_eq!(built["replaces"], json!(["Writer", "Reader"]), "{built}");
    // The answered instance, unedited, is the only one.
    let applied = rig
        .eng(
            "PUT",
            CORE_RUNTIMES,
            Some(json!({ "instances": [built["instance"]] })),
            200,
        )
        .await;
    assert_eq!(
        applied["changes"]["started"],
        json!(["runtime:Line"]),
        "{applied}"
    );
    // Level 20 reaches Reader only through S: 20 * 10 + the unwritten spare's 5.
    rig.speed_is("Reader drives 205 from what Writer handed over", 205)
        .await;
    rig.sim.set_input(0, 31).await;
    rig.speed_is("the speed follows the level, through the block", 315)
        .await;
    let runtime = rig
        .runtime_when("it runs the built programs with the block", |t| {
            runs(&built)(t) && t["data"].is_array()
        })
        .await;
    assert_eq!(runtime["id"], "Line", "{runtime}");
    // The build's version is the one the core reports of the runtime it loaded.
    assert_eq!(runtime["version"], built["version"], "{runtime}");
    assert_eq!(
        runtime["data"][0]["members"],
        json!([
            {"name": "level", "type": "INT", "writer": "Writer"},
            {"name": "spare", "type": "INT", "writer": null}
        ]),
        "{runtime}"
    );
    // A variable of a runtime of several programs is named by its program.
    let watched = rig
        .eng(
            "POST",
            "/api/v1/runtimes/Line/watch",
            Some(json!({ "variables": ["Reader.speed"] })),
            200,
        )
        .await;
    assert_eq!(watched["values"][0]["value"], 315, "{watched}");
    rig.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn programs_sharing_a_block_loaded_alone_refuse_their_runtime_until_the_replaced_instances_go(
) {
    let rig = rig().await;
    rig.shared().await;
    rig.writer_and_reader().await;
    rig.eng(
        "PUT",
        "/api/v1/projects/plant/io",
        Some(rig.io_config()),
        200,
    )
    .await;
    let writer = rig.build("Writer").await;
    let reader = rig.build("Reader").await;
    // Each alone is a runtime of its own: the block in two runtimes is refused.
    let refused = rig
        .eng(
            "PUT",
            CORE_RUNTIMES,
            Some(json!({ "instances": [writer["instance"], reader["instance"]] })),
            400,
        )
        .await;
    assert!(
        refused
            .to_string()
            .contains("data block 'S' is in runtimes 'Writer' and 'Reader'"),
        "{refused}"
    );
    let applied = rig
        .eng(
            "PUT",
            CORE_RUNTIMES,
            Some(json!({ "instances": [writer["instance"]] })),
            200,
        )
        .await;
    assert_eq!(
        applied["changes"]["started"],
        json!(["runtime:Writer"]),
        "{applied}"
    );

    // The runtime of both, saved beside the instance it replaces: refused.
    let built = rig.build_runtime("Line", line(5)).await;
    assert_eq!(built["replaces"], json!(["Writer", "Reader"]), "{built}");
    let refused = rig
        .eng(
            "PUT",
            CORE_RUNTIMES,
            Some(json!({ "instances": [writer["instance"], built["instance"]] })),
            400,
        )
        .await;
    assert!(
        refused
            .to_string()
            .contains("data block 'S' is in runtimes 'Writer' and 'Line'"),
        "{refused}"
    );
    // Without the instances it names in `replaces`: started.
    let applied = rig.load_built(&built, 200).await;
    assert_eq!(
        applied["changes"]["started"],
        json!(["runtime:Line"]),
        "{applied}"
    );
    let config = rig.eng("GET", CORE_RUNTIMES, None, 200).await;
    let ids: Vec<&str> = config["config"]["instances"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["Line"], "{config}");
    rig.speed_is("Reader drives 205 from what Writer handed over", 205)
        .await;
    rig.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_task_change_of_a_runtime_built_by_the_logic_module_is_an_online_change() {
    let rig = rig().await;
    rig.shared().await;
    rig.writer_and_reader().await;
    rig.eng(
        "PUT",
        "/api/v1/projects/plant/io",
        Some(rig.io_config()),
        200,
    )
    .await;
    let first = rig.build_runtime("Line", line(5)).await;
    rig.load_built(&first, 200).await;
    rig.runtime_when("the first build runs and has scanned", runs(&first))
        .await;

    // Another priority for `slow`: the same programs, so changed online.
    let second = rig.build_runtime("Line", line(7)).await;
    assert_ne!(versions_of(&second), versions_of(&first));
    assert_eq!(second["instance"]["tasks"][1]["priority"], 7);
    let applied = rig.load_built(&second, 200).await;
    let load = &applied["changes"]["runtimes"][0];
    assert_eq!(load["runtime"], "Line", "{applied}");
    assert_eq!(load["start"], "online", "why: {}; {applied}", load["why"]);
    rig.runtime_when("the second build runs and has scanned", runs(&second))
        .await;
    rig.speed_is("Reader drives 205 after the change", 205)
        .await;

    // A third program: not the same programs, so a warm start, and the core says why.
    rig.program(
        "Extra",
        "PROGRAM Extra\nVAR n : DINT; END_VAR\n  n := n + 1;\nEND_PROGRAM",
        json!({"interval_ms": 20, "runtime": "vm", "target": "pc", "run": true}),
    )
    .await;
    let mut three = line(7);
    three["tasks"][1]["programs"] = json!(["Reader", "Extra"]);
    let third = rig.build_runtime("Line", three).await;
    assert_eq!(
        third["replaces"],
        json!(["Writer", "Reader", "Extra"]),
        "{third}"
    );
    let applied = rig.load_built(&third, 200).await;
    let load = &applied["changes"]["runtimes"][0];
    assert_eq!(
        (&load["start"], &load["why"]),
        (
            &json!("warm"),
            &json!("the program running was 'Reader,Writer', not 'Extra,Reader,Writer'")
        ),
        "{applied}"
    );
    rig.runtime_when("the third build runs and has scanned", runs(&third))
        .await;
    rig.close().await;
}

/// The file in which the core keeps runtime `id`'s RETAIN values, once it has one.
fn retain_file_of(root: &Path, id: &str) -> Option<PathBuf> {
    fn walk(dir: &Path, name: &str) -> Option<PathBuf> {
        for e in std::fs::read_dir(dir).ok()? {
            let p = e.ok()?.path();
            if p.is_dir() {
                if let Some(f) = walk(&p, name) {
                    return Some(f);
                }
            } else if p.file_name().is_some_and(|n| n == name)
                && p.components().any(|c| c.as_os_str() == "retain")
            {
                return Some(p);
            }
        }
        None
    }
    walk(root, &format!("{id}.json"))
}

/// The core keeps RETAIN values per runtime id, not per program (the core's and the runtime
/// module's, not fixed in the logic module). So when runtime `Line`'s one program changes from A
/// to B, a warm load gives B's RETAIN `total` what was kept under `Line` for A's `total`, and a
/// cold load does not. The build of B says exactly that: this test is the check that its warning
/// is true, and it goes red when the core stops handing one program's values to another, which
/// is when the warning must change with it.
#[tokio::test(flavor = "multi_thread")]
async fn a_runtime_id_given_another_program_warns_of_the_values_a_warm_load_hands_it() {
    let rig = rig().await;
    let settings = json!({"interval_ms": 20, "runtime": "vm", "target": "pc", "run": true});
    rig.program(
        "A",
        "PROGRAM A\nVAR RETAIN total : DINT; END_VAR\n  total := total + 1;\nEND_PROGRAM",
        settings.clone(),
    )
    .await;
    rig.program(
        "B",
        "PROGRAM B\nVAR RETAIN total : DINT; END_VAR\nVAR n : DINT; END_VAR\n  n := n + 1;\nEND_PROGRAM",
        settings,
    )
    .await;
    let one = |p: &str| {
        json!({ "module": "vm", "target": "pc", "run": true, "tasks": [
            { "name": "main", "kind": { "type": "cyclic", "interval_ms": 20 }, "priority": 1, "programs": [p] }
        ]})
    };
    let first = rig.build_runtime("Line", one("A")).await;
    // One program is answered in `program`, as a runtime of today, and never as a list of one.
    assert!(
        first["instance"]["program"].is_object() && first["instance"].get("programs").is_none(),
        "{first}"
    );
    rig.load_built(&first, 200).await;
    // Until the core has kept a total of A's above its initial value under `Line`.
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let kept = retain_file_of(rig._root.path(), "Line")
            .and_then(|f| std::fs::read_to_string(f).ok())
            .unwrap_or_default();
        if kept.contains("total") && !kept.contains("00000000") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the core kept no total of A's under Line: {kept:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let second = rig.build_runtime("Line", one("B")).await;
    assert_eq!(
        second["warnings"],
        json!([
            "program 'B': RETAIN values are kept per runtime id, not per program: a warm load of runtime 'Line' puts into each RETAIN variable of 'B' the value kept under runtime 'Line' for a variable of the same name, type and size, whichever program ran in runtime 'Line' before, and none of the values kept while 'B' ran alone as runtime 'B'; a cold load starts every RETAIN variable from its initial value"
        ]),
        "{second}"
    );
    let total = || async {
        let watched = rig
            .eng(
                "POST",
                "/api/v1/runtimes/Line/watch",
                Some(json!({ "variables": ["total"] })),
                200,
            )
            .await;
        (watched["values"][0]["value"].clone(), watched)
    };
    let applied = rig.load_built(&second, 200).await;
    rig.runtime_when("B runs warm and has scanned", |t| {
        t["program"] == "B" && t["mode"] == "run" && t["scans"].as_u64() >= Some(1)
    })
    .await;
    let (warm, watched) = total().await;
    assert!(
        warm.as_i64().is_some_and(|v| v >= 1),
        "a warm load gives B what was kept under Line for A, as the warning says: {watched}; the core answered {applied}"
    );
    // Cold: from its initial value, as the warning says (B never writes its total).
    let asked = rig
        .eng("POST", "/api/v1/runtimes/Line/cold-start", None, 200)
        .await;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let (cold, watched) = total().await;
        if cold == 0 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "a cold start leaves B's total at its initial value 0: {watched}; the core answered {asked}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    rig.close().await;
}
