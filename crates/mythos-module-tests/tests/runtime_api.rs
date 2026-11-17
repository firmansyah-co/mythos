//! Runtimes over the API (ADR-013 §2.5), on a server assembled as `server::assemble` builds one:
//! accounts, the module directory with the real Modbus and runtime module executables, and a
//! Modbus device (a simulator). An engineer loads a program, it runs the plant, an operator
//! works with it, and every refusal is the role's or the module's.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use driver_modbus::sim::ModbusSimServer;
use mythos_core::api::{create_router, ApiState};
use mythos_core::auth::{Auth, Role};
use mythos_core::io::{DriverRegistry, IoManager};
use mythos_core::storage::{ControllerStorage, ProjectConfig};
use mythos_core::tag::TagCatalog;
use mythos_logic_compiler::{compile, TargetProfile};
use mythos_pal::DefaultShm;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tower::util::ServiceExt;

const MODBUS: &str = env!("CARGO_BIN_EXE_modbus-module-under-test");
const RUNTIME_VM: &str = env!("CARGO_BIN_EXE_runtime-vm-module-under-test");
static SEQ: AtomicU32 = AtomicU32::new(0);

const LEVEL: &str = "external.Plant.LT1.level";
const SPEED: &str = "external.Plant.P1.speed";
const T_SETPOINT: &str = "internal.Plant.Tank.setpoint";
const SAFE_SPEED: u16 = 7;

const TANK: &str = "
PROGRAM Tank
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
    sim: ModbusSimServer,
    engineer: String,
    operator: String,
    viewer: String,
    _root: tempfile::TempDir,
    _modules: tempfile::TempDir,
    _segment: Segment,
}

async fn rig() -> Rig {
    let root_dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(root_dir.path().join("projects")).unwrap());
    storage
        .create_project(&ProjectConfig::new("plant", "Plant"))
        .unwrap();
    storage.switch_active_project("plant").unwrap();

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
        "mythos_rtapi_test_{}_{}",
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

    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_input(0, 20).await;
    sim.set_holding(1, 999).await;
    Rig {
        app: create_router(state),
        io,
        sim,
        engineer,
        operator,
        viewer,
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

    /// The first runtime of `GET /api/v1/runtimes`, as the engineer sees it.
    async fn tank(&self) -> Value {
        let (status, body) = self
            .send(&self.engineer, "GET", "/api/v1/runtimes", None)
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["runtimes"][0].clone()
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
}

fn runtimes(variable_of_level: &str) -> Value {
    let compiled = compile(TANK, &TargetProfile::pc()).unwrap_or_else(|d| panic!("{d:?}"));
    json!({"instances": [{
        "id": "tank", "module": "vm", "run": true,
        "program": {
            "name": compiled.name, "version": "v1", "interval_ms": 20,
            "code": hex::encode(&compiled.bytecode), "symbols": compiled.symbols,
            "inputs": [
                {"variable": variable_of_level, "tag": LEVEL},
                {"variable": "level_ok", "tag": LEVEL, "of": "good"}
            ],
            "outputs": [{"variable": "speed", "tag": SPEED, "safe": SAFE_SPEED}],
            "published": [{"variable": "setpoint", "tag": T_SETPOINT, "writable": true}]
        }
    }]})
}

#[tokio::test]
async fn an_engineer_loads_a_program_it_runs_the_plant_and_each_role_does_what_it_may() {
    let rig = rig().await;
    let (status, body) = rig
        .send(
            &rig.engineer,
            "PUT",
            "/api/v1/projects/plant/io",
            Some(rig.io_config()),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // The runtime module is listed with what it can run, before anything is loaded.
    let (_, body) = rig.send(&rig.viewer, "GET", "/api/v1/runtimes", None).await;
    assert_eq!(body["runtimes"], json!([]), "{body}");
    assert_eq!(body["modules"][0]["provides"], "vm", "{body}");
    assert_eq!(body["modules"][0]["profile"]["name"], "pc", "{body}");

    // A program bound to a variable it does not have is refused by the module, and not saved.
    let (status, body) = rig
        .send(
            &rig.engineer,
            "PUT",
            "/api/v1/projects/plant/runtimes",
            Some(runtimes("flow")),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        body["problems"][0]["connection_id"], "runtime:tank",
        "{body}"
    );
    assert!(
        body["problems"][0]["message"]
            .as_str()
            .unwrap()
            .contains("the program has no variable flow"),
        "{body}"
    );
    let (_, body) = rig
        .send(
            &rig.engineer,
            "GET",
            "/api/v1/projects/plant/runtimes",
            None,
        )
        .await;
    assert_eq!(body["config"], Value::Null, "nothing was saved: {body}");

    // What the configuration itself gets wrong is refused before any module is asked.
    let mut external = runtimes("level");
    external["instances"][0]["program"]["published"][0]["tag"] = json!("external.Plant.X.y");
    let (status, body) = rig
        .send(
            &rig.engineer,
            "PUT",
            "/api/v1/projects/plant/runtimes",
            Some(external),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("must start with 'internal.'"),
        "{body}"
    );
    let mut no_safe = runtimes("level");
    no_safe["instances"][0]["program"]["outputs"][0]
        .as_object_mut()
        .unwrap()
        .remove("safe");
    let (status, body) = rig
        .send(
            &rig.engineer,
            "PUT",
            "/api/v1/projects/plant/runtimes",
            Some(no_safe),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["error"].as_str().unwrap().contains("safe"),
        "an output has no default safe value: {body}"
    );

    // Loading a program is engineering.
    for who in [&rig.viewer, &rig.operator] {
        let (status, _) = rig
            .send(
                who,
                "PUT",
                "/api/v1/projects/plant/runtimes",
                Some(runtimes("level")),
            )
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    let (status, body) = rig
        .send(
            &rig.engineer,
            "PUT",
            "/api/v1/projects/plant/runtimes",
            Some(runtimes("level")),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["applied"], true, "{body}");
    assert_eq!(
        body["changes"]["started"],
        json!(["runtime:tank"]),
        "{body}"
    );
    assert_eq!(
        body["changes"]["kept"],
        json!(["plc"]),
        "the field connection was not touched: {body}"
    );

    // Level 20, setpoint 50.
    rig.speed_is("the program commands 300", 300).await;
    let tank = rig
        .tank_when("the status says 300 was confirmed", |t| {
            t["outputs"][0]["written"] == 300
        })
        .await;
    assert_eq!(tank["mode"], "run", "{tank}");
    assert_eq!(tank["program"], "Tank");
    assert_eq!(tank["outputs"][0]["written"], 300, "{tank}");

    // Anybody signed in reads the program's variables, bound to a tag or not: reading is not
    // engineering, and it changes nothing.
    let (status, body) = rig
        .send(
            &rig.viewer,
            "POST",
            "/api/v1/runtimes/tank/watch",
            Some(json!({"variables": ["level", "speed", "flow"]})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["id"], "tank", "{body}");
    assert_eq!(body["state"]["mode"], "run", "{body}");
    assert_eq!(body["state"]["program"], "Tank", "{body}");
    assert_eq!(
        body["values"],
        json!([
            {"variable": "level", "type": "INT", "value": 20, "error": null},
            {"variable": "speed", "type": "INT", "value": 300, "error": null},
            {"variable": "flow", "type": null, "value": null,
             "error": "the program has no variable flow"}
        ]),
        "{body}"
    );
    assert!(
        body["age_ms"].as_u64().is_some_and(|age| age <= 1_000),
        "the values come with their age: {body}"
    );
    let (status, body) = rig
        .send(
            &rig.viewer,
            "POST",
            "/api/v1/runtimes/tank/watch",
            Some(json!({"variables": []})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    // A watch is bounded: the runtime is asked for all of it at every exchange.
    let many: Vec<String> = (0..257).map(|i| format!("v{i}")).collect();
    let (status, body) = rig
        .send(
            &rig.viewer,
            "POST",
            "/api/v1/runtimes/tank/watch",
            Some(json!({ "variables": many })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        body["error"], "a watch names between 1 and 256 variables",
        "{body}"
    );
    let (status, body) = rig
        .send(
            &rig.viewer,
            "POST",
            "/api/v1/runtimes/nobody/watch",
            Some(json!({"variables": ["level"]})),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"], "no runtime 'nobody' is loaded on this node");
    rig.speed_is("watched, the program still commands 300", 300)
        .await;

    // An operator cannot write the output the program owns, and can change its setpoint.
    let (status, body) = rig
        .send(
            &rig.operator,
            "PUT",
            &format!("/api/v1/tags/{SPEED}/value"),
            Some(json!({"value": 5})),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["written"], false, "{body}");
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("an output of runtime 'tank'"),
        "{body}"
    );
    let (status, body) = rig
        .send(
            &rig.operator,
            "PUT",
            &format!("/api/v1/tags/{T_SETPOINT}/value"),
            Some(json!({"value": 30})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    rig.speed_is("setpoint 30: the program commands 100", 100)
        .await;

    // Stopping and starting a program is engineering too.
    for who in [&rig.viewer, &rig.operator] {
        let (status, _) = rig
            .send(who, "POST", "/api/v1/runtimes/tank/stop", None)
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    let (status, body) = rig
        .send(&rig.engineer, "POST", "/api/v1/runtimes/tank/stop", None)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    rig.speed_is("stopped: the safe value", SAFE_SPEED).await;
    let deadline = Instant::now() + Duration::from_secs(10);
    while rig.tank().await["mode"] != "stop" {
        assert!(
            Instant::now() < deadline,
            "never reported stop: {}",
            rig.tank().await
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(rig.tank().await["outputs_safe"], true);
    let (status, _) = rig
        .send(&rig.engineer, "POST", "/api/v1/runtimes/nobody/run", None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = rig
        .send(&rig.engineer, "POST", "/api/v1/runtimes/tank/run", None)
        .await;
    assert_eq!(status, StatusCode::OK);
    rig.speed_is("running again, from a cold start: setpoint 50", 300)
        .await;

    // The saved configuration is what was loaded, and unloading it makes the output safe.
    let (_, body) = rig
        .send(&rig.viewer, "GET", "/api/v1/projects/plant/runtimes", None)
        .await;
    assert_eq!(body["config"]["instances"][0]["id"], "tank", "{body}");
    let (status, body) = rig
        .send(
            &rig.engineer,
            "PUT",
            "/api/v1/projects/plant/runtimes",
            Some(json!({"instances": []})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["changes"]["stopped"],
        json!(["runtime:tank"]),
        "{body}"
    );
    assert_eq!(rig.sim.holding(1).await, Some(SAFE_SPEED));
    let (_, body) = rig.send(&rig.viewer, "GET", "/api/v1/runtimes", None).await;
    assert_eq!(body["runtimes"], json!([]), "{body}");

    rig.io.shutdown().await;
}

#[tokio::test]
async fn a_program_for_a_runtime_module_that_is_not_installed_is_refused_naming_what_is() {
    let rig = rig().await;
    let mut other = runtimes("level");
    other["instances"][0]["module"] = json!("plc-9000");
    let (status, body) = rig
        .send(
            &rig.engineer,
            "PUT",
            "/api/v1/projects/plant/runtimes",
            Some(other),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        body["problems"][0]["message"], "no runtime module provides 'plc-9000' (available: vm)",
        "{body}"
    );
    rig.io.shutdown().await;
}
