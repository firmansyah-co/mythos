//! The logic authoring module (ADR-013 §2.6) as the server runs it: a real module process
//! behind the project routes, beside the real runtime and Modbus modules. An engineer writes a
//! program, checks it, binds it to tags, builds it, loads what was built into the project's
//! runtimes, and it runs the plant (a Modbus simulator).

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
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tower::util::ServiceExt;

const MODBUS: &str = env!("CARGO_BIN_EXE_modbus-module-under-test");
const RUNTIME_VM: &str = env!("CARGO_BIN_EXE_runtime-vm-module-under-test");
const LOGIC: &str = env!("CARGO_BIN_EXE_logic-module-under-test");
static SEQ: AtomicU32 = AtomicU32::new(0);

const LEVEL: &str = "external.Plant.LT1.level";
const SPEED: &str = "external.Plant.P1.speed";
const T_SETPOINT: &str = "internal.Plant.Tank.setpoint";
const PROGRAMS: &str = "/api/v1/projects/plant/logic/programs";

const TANK: &str = "PROGRAM Tank
VAR_INPUT level : INT; level_ok : BOOL; setpoint : INT := 50; END_VAR
VAR_OUTPUT speed : INT; END_VAR
  IF level_ok AND (level < setpoint) THEN
    speed := (setpoint - level) * 10;
  ELSE
    speed := 0;
  END_IF;
END_PROGRAM
";

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
    operator: String,
    viewer: String,
    project: PathBuf,
    _root: tempfile::TempDir,
    _modules: tempfile::TempDir,
    _segment: Segment,
}

async fn rig() -> Rig {
    let root_dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(root_dir.path().join("projects")).unwrap());
    for id in ["plant", "other"] {
        storage.create_project(&ProjectConfig::new(id, id)).unwrap();
    }
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
    for (name, role) in [
        ("erin", Role::Engineer),
        ("otto", Role::Operator),
        ("vera", Role::Viewer),
    ] {
        auth.create_user(name, "", role, "a good password").unwrap();
    }
    let session = |name: &str| auth.sign_in(name, "a good password").unwrap().1;
    let (engineer, operator, viewer) = (session("erin"), session("otto"), session("vera"));

    let catalog = Arc::new(RwLock::new(TagCatalog::new()));
    let dirs = vec![modules.path().to_path_buf()];
    let shm = format!(
        "mythos_logic_test_{}_{}",
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

    // The logic module, mounted as the server mounts a service module.
    let path = Path::new(LOGIC);
    let module = Arc::new(ModuleBinary {
        path: path.to_path_buf(),
        manifest: read_manifest(path)
            .await
            .expect("the logic module prints its manifest"),
        fingerprint: Fingerprint::of(path).unwrap(),
    });
    let services = ServiceHost::new(state.hmi_events.clone());
    services.sync(vec![module]).await;

    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_input(0, 20).await;
    sim.set_holding(1, 999).await;
    Rig {
        app: create_router(state.with_services(Arc::clone(&services))),
        io,
        services,
        sim,
        engineer,
        operator,
        viewer,
        project,
        _root: root_dir,
        _modules: modules,
        _segment: Segment(shm),
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
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 22)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    /// As the engineer, expecting `expected`.
    async fn eng(&self, method: &str, uri: &str, body: Option<Value>, expected: u16) -> Value {
        let (status, answer) = self.send(&self.engineer, method, uri, body).await;
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

    async fn tank(&self) -> Value {
        self.eng("GET", "/api/v1/runtimes", None, 200).await["runtimes"][0].clone()
    }

    /// The runtime's status once `holds` says it is so. A runtime reports what the device
    /// confirmed when its exchange ends, so the device can show a value a moment before the
    /// status does: a test waits for the status, never assumes both change at once.
    async fn tank_when(&self, what: &str, holds: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let tank = self.tank().await;
            if holds(&tank) {
                return tank;
            }
            assert!(Instant::now() < deadline, "never: {what}; runtime {tank}");
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
                self.tank().await
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn close(self) {
        self.io.shutdown().await;
        self.services.shutdown().await;
    }
}

fn settings() -> Value {
    json!({
        "interval_ms": 20, "runtime": "vm", "target": "pc", "run": true,
        "inputs": [
            {"variable": "level", "tag": LEVEL},
            {"variable": "level_ok", "tag": LEVEL, "of": "good"}
        ],
        "outputs": [{"variable": "speed", "tag": SPEED, "safe": 7}],
        "published": [{"variable": "setpoint", "tag": T_SETPOINT, "writable": true}]
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_program_is_written_checked_built_loaded_and_runs_the_plant() {
    let rig = rig().await;
    let tank = format!("{PROGRAMS}/Tank");

    // A new program is an empty PROGRAM that compiles, and is not set to run.
    let created = rig
        .eng("POST", PROGRAMS, Some(json!({"name": "Tank"})), 201)
        .await;
    assert_eq!(created["program"]["run"], false, "{created}");
    assert_eq!(created["program"]["interval_ms"], 100, "{created}");
    let checked = rig.eng("POST", &format!("{tank}/check"), None, 200).await;
    assert_eq!(checked["ok"], true, "{checked}");
    assert_eq!(checked["variables"], json!([]), "{checked}");

    // A text is checked without being saved: the diagnostics have a line and a column.
    let wrong = "PROGRAM Tank\nVAR a : INT; END_VAR\n  a := b;\nEND_PROGRAM";
    let checked = rig
        .eng(
            "POST",
            &format!("{tank}/check"),
            Some(json!({"source": wrong})),
            200,
        )
        .await;
    assert_eq!(checked["ok"], false, "{checked}");
    assert_eq!(
        checked["diagnostics"],
        json!([{"line": 3, "col": 8, "message": "b is not declared"}]),
    );
    let read = rig.eng("GET", &tank, None, 200).await;
    assert_eq!(read["source"], created["source"], "the check saved nothing");

    // The source and the bindings, saved; the check then knows the variables and finds no
    // binding wrong.
    rig.eng(
        "PUT",
        &format!("{tank}/source"),
        Some(json!({"source": TANK})),
        200,
    )
    .await;
    let saved = rig
        .eng("PUT", &format!("{tank}/settings"), Some(settings()), 200)
        .await;
    assert_eq!(
        saved["program"]["outputs"][0]["safe"], 7,
        "the settings are what was sent: {saved}"
    );
    let checked = rig.eng("POST", &format!("{tank}/check"), None, 200).await;
    assert_eq!(checked["binding_problems"], json!([]), "{checked}");
    let names: Vec<&str> = checked["variables"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["path"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["level", "level_ok", "setpoint", "speed"]);

    // Built before the field connection exists: the tags are not there, which is said.
    let built = rig.eng("POST", &format!("{tank}/build"), None, 200).await;
    assert_eq!(
        built["warnings"],
        json!([
            format!(
                "input {LEVEL} is not a tag of this controller now: level will not take a value"
            ),
            format!(
                "input {LEVEL} is not a tag of this controller now: level_ok will not take a value"
            ),
            format!("output {SPEED} is not a tag of this controller now: writes to it will fail"),
        ]),
        "{built}"
    );
    rig.eng(
        "PUT",
        "/api/v1/projects/plant/io",
        Some(rig.io_config()),
        200,
    )
    .await;
    let built = rig.eng("POST", &format!("{tank}/build"), None, 200).await;
    assert_eq!(built["warnings"], json!([]), "{built}");

    // What was built is what the core loads: nothing ran until it was saved into the runtimes.
    assert_eq!(rig.sim.holding(1).await, Some(999), "nothing wrote yet");
    assert_eq!(
        rig.eng("GET", "/api/v1/runtimes", None, 200).await["runtimes"],
        json!([])
    );
    let applied = rig
        .eng(
            "PUT",
            "/api/v1/projects/plant/runtimes",
            Some(json!({"instances": [built["instance"]]})),
            200,
        )
        .await;
    assert_eq!(
        applied["changes"]["started"],
        json!(["runtime:Tank"]),
        "{applied}"
    );
    // Level 20, setpoint 50.
    rig.speed_is("the program commands 300", 300).await;
    let running = rig
        .tank_when("the status names the built version", |t| {
            t["version"] == built["version"]
        })
        .await;
    assert_eq!(running["program"], "Tank", "{running}");
    assert_eq!(running["version"], built["version"], "{running}");

    // An edit changes nothing that runs until it is built and loaded; then it is another version.
    rig.eng(
        "PUT",
        &format!("{tank}/source"),
        Some(json!({"source": TANK.replace("* 10", "* 20")})),
        200,
    )
    .await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(rig.sim.holding(1).await, Some(300), "the old program runs");
    let rebuilt = rig.eng("POST", &format!("{tank}/build"), None, 200).await;
    assert_ne!(rebuilt["version"], built["version"]);
    rig.eng(
        "PUT",
        "/api/v1/projects/plant/runtimes",
        Some(json!({"instances": [rebuilt["instance"]]})),
        200,
    )
    .await;
    rig.speed_is("the new program commands 600", 600).await;
    // The replacement's status is published at the end of its first exchange, which writes 600.
    rig.tank_when("the status names the rebuilt version", |t| {
        t["version"] == rebuilt["version"]
    })
    .await;

    // A program that does not compile, or binds a variable it lost, is not built.
    rig.eng(
        "PUT",
        &format!("{tank}/source"),
        Some(json!({"source": wrong})),
        200,
    )
    .await;
    let refused = rig.eng("POST", &format!("{tank}/build"), None, 400).await;
    assert_eq!(refused["diagnostics"][0]["line"], 3, "{refused}");
    rig.eng(
        "PUT",
        &format!("{tank}/source"),
        Some(json!({"source": TANK.replace("level_ok", "fresh")})),
        200,
    )
    .await;
    let refused = rig.eng("POST", &format!("{tank}/build"), None, 400).await;
    assert_eq!(
        refused["binding_problems"],
        json!([format!(
            "input {LEVEL}: the program has no variable level_ok"
        )]),
        "{refused}"
    );

    // Removing the program from the project does not stop what runs.
    rig.eng("DELETE", &tank, None, 200).await;
    rig.eng("GET", &tank, None, 404).await;
    assert_eq!(rig.tank().await["mode"], "run");
    assert_eq!(rig.sim.holding(1).await, Some(600));

    rig.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn each_role_does_what_it_may_and_what_is_wrong_is_refused_and_not_saved() {
    let rig = rig().await;
    let tank = format!("{PROGRAMS}/Tank");

    // Writing programs is engineering; reading and checking them is not.
    for who in [&rig.viewer, &rig.operator] {
        let (status, _) = rig
            .send(who, "POST", PROGRAMS, Some(json!({"name": "Tank"})))
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    rig.eng("POST", PROGRAMS, Some(json!({"name": "Tank"})), 201)
        .await;
    for (method, uri, body) in [
        ("PUT", format!("{tank}/source"), json!({"source": TANK})),
        ("PUT", format!("{tank}/settings"), settings()),
        ("POST", format!("{tank}/build"), Value::Null),
        ("DELETE", tank.clone(), Value::Null),
    ] {
        let (status, _) = rig.send(&rig.operator, method, &uri, Some(body)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}");
    }
    let (status, listed) = rig.send(&rig.viewer, "GET", PROGRAMS, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["programs"][0]["name"], "Tank", "{listed}");
    let (status, checked) = rig
        .send(&rig.viewer, "POST", &format!("{tank}/check"), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(checked["ok"], true, "{checked}");

    // Names: one per project whatever its case, and nothing that is not a name.
    let twice = rig
        .eng("POST", PROGRAMS, Some(json!({"name": "TANK"})), 409)
        .await;
    assert_eq!(twice["error"], "the project already has a program 'TANK'");
    for bad in ["2pump", "a b", "a.b", ""] {
        rig.eng("POST", PROGRAMS, Some(json!({"name": bad})), 400)
            .await;
    }
    rig.eng("GET", &format!("{PROGRAMS}/..%2F..%2Fproject"), None, 400)
        .await;
    // A program's name becomes a file name, so what is not a name is refused as one, before
    // anything is looked up: not "no such program".
    let refused = rig.eng("GET", &format!("{PROGRAMS}/a.b"), None, 400).await;
    assert_eq!(refused["error"], "'a.b' is not a program name");
    rig.eng("GET", &format!("{PROGRAMS}/Pump"), None, 404).await;
    rig.eng(
        "PUT",
        &format!("{PROGRAMS}/Pump/source"),
        Some(json!({"source": TANK})),
        404,
    )
    .await;

    // Settings that are wrong are refused, and what was saved before stays.
    rig.eng("PUT", &format!("{tank}/settings"), Some(settings()), 200)
        .await;
    for (change, said) in [
        (json!({"interval_ms": 0}), "at least 1"),
        (json!({"target": "plc-9000"}), "is not a target profile"),
        (
            json!({"published": [{"variable": "setpoint", "tag": "external.Plant.X.y"}]}),
            "must start with 'internal.'",
        ),
        (
            json!({"outputs": [{"variable": "speed", "tag": SPEED}]}),
            "safe",
        ),
        (json!({"colour": "red"}), "colour"),
    ] {
        let mut wrong = settings();
        wrong
            .as_object_mut()
            .unwrap()
            .extend(change.as_object().unwrap().clone());
        let refused = rig
            .eng("PUT", &format!("{tank}/settings"), Some(wrong), 400)
            .await;
        assert!(
            refused["error"].as_str().unwrap().contains(said),
            "{said}: {refused}"
        );
    }
    let read = rig.eng("GET", &tank, None, 200).await;
    assert_eq!(read["program"]["interval_ms"], 20, "{read}");
    assert_eq!(read["program"]["outputs"][0]["safe"], 7, "{read}");

    // Where it is kept: the module's directory in the project, one file per source. Another
    // project has none of it.
    let data = rig.project.join("logic");
    let list: Value =
        serde_json::from_str(&std::fs::read_to_string(data.join("programs.json")).unwrap())
            .unwrap();
    assert_eq!(list["programs"][0]["name"], "Tank");
    assert!(data.join("programs/Tank.st").is_file());
    let other = rig
        .eng("GET", "/api/v1/projects/other/logic/programs", None, 200)
        .await;
    assert_eq!(other["programs"], json!([]));

    // A list that cannot be read is said. It is not taken for an empty one and written over.
    std::fs::write(data.join("programs.json"), "{ not json").unwrap();
    let unread = rig.eng("GET", PROGRAMS, None, 500).await;
    assert!(
        unread["error"]
            .as_str()
            .unwrap()
            .contains("cannot be read as the project's programs"),
        "{unread}"
    );
    rig.eng("POST", PROGRAMS, Some(json!({"name": "Pump"})), 500)
        .await;
    assert_eq!(
        std::fs::read_to_string(data.join("programs.json")).unwrap(),
        "{ not json"
    );
    assert!(!data.join("programs/Pump.st").exists());

    rig.close().await;
}

/// Every route is an MCP tool (ADR-010): an agent writes, checks and builds a program with them.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_writes_checks_and_builds_a_program_over_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(dir.path()).unwrap());
    storage
        .create_project(&ProjectConfig::new("plant", "Plant"))
        .unwrap();
    let path = Path::new(LOGIC);
    let module = Arc::new(ModuleBinary {
        path: path.to_path_buf(),
        manifest: read_manifest(path).await.unwrap(),
        fingerprint: Fingerprint::of(path).unwrap(),
    });
    let state = ApiState::<mythos_pal::MemoryShm>::new(
        storage,
        Arc::new(RwLock::new(TagCatalog::new())),
        None,
    );
    let services = ServiceHost::new(state.hmi_events.clone());
    services.sync(vec![module]).await;
    let app = create_router(state.with_services(Arc::clone(&services)));

    let rpc = |method: &'static str, params: Value| {
        let app = app.clone();
        async move {
            let request = Request::builder()
                .method("POST")
                .uri("/mcp")
                .header(header::HOST, "127.0.0.1:8080")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::ACCEPT, "application/json, text/event-stream")
                .body(Body::from(
                    json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
                        .to_string(),
                ))
                .unwrap();
            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            serde_json::from_slice::<Value>(&bytes).unwrap()
        }
    };
    let tool = |name: &'static str, arguments: Value| {
        let call = rpc("tools/call", json!({"name": name, "arguments": arguments}));
        async move {
            let answer = call.await;
            let result = &answer["result"];
            assert!(result.is_object(), "no result: {answer}");
            (
                result["isError"].as_bool().unwrap(),
                result["structuredContent"]["body"].clone(),
            )
        }
    };

    let listed = rpc("tools/list", json!({})).await;
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .filter(|n| n.contains("logic"))
        .collect();
    assert_eq!(
        names,
        [
            "list_logic_programs",
            "create_logic_program",
            "get_logic_program",
            "save_logic_source",
            "save_logic_settings",
            "delete_logic_program",
            "check_logic_program",
            "build_logic_program",
        ]
    );

    let (error, created) = tool(
        "create_logic_program",
        json!({"project_id": "plant", "body": {"name": "Tank", "interval_ms": 50}}),
    )
    .await;
    assert!(!error, "{created}");
    let (error, saved) = tool(
        "save_logic_source",
        json!({"project_id": "plant", "program": "Tank", "body": {"source": TANK}}),
    )
    .await;
    assert!(!error, "{saved}");
    // The body the settings tool documents is the one the module takes.
    let schema = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "save_logic_settings")
        .unwrap()["inputSchema"]["properties"]["body"]
        .clone();
    assert_eq!(
        schema["required"],
        json!(["interval_ms", "runtime", "target", "run"])
    );
    let (error, saved) = tool(
        "save_logic_settings",
        json!({"project_id": "plant", "program": "Tank", "body": settings()}),
    )
    .await;
    assert!(!error, "{saved}");
    let (error, checked) = tool(
        "check_logic_program",
        json!({"project_id": "plant", "program": "Tank", "body": {}}),
    )
    .await;
    assert!(!error, "{checked}");
    assert_eq!(checked["ok"], true, "{checked}");
    assert_eq!(checked["binding_problems"], json!([]), "{checked}");
    let (error, built) = tool(
        "build_logic_program",
        json!({"project_id": "plant", "program": "Tank"}),
    )
    .await;
    assert!(!error, "{built}");
    assert_eq!(built["instance"]["id"], "Tank", "{built}");
    assert_eq!(built["instance"]["program"]["interval_ms"], 20, "{built}");
    let (error, refused) = tool(
        "build_logic_program",
        json!({"project_id": "plant", "program": "Pump"}),
    )
    .await;
    assert!(error, "{refused}");
    assert_eq!(refused["error"], "the project has no program 'Pump'");

    services.shutdown().await;
}
