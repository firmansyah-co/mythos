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
use mythos_logic_compiler::{
    check_project, compile, compile_sources, DataBlock, Sources, TargetProfile,
};
use mythos_pal::DefaultShm;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tower::util::ServiceExt;

const MODBUS: &str = env!("CARGO_BIN_EXE_modbus-module-under-test");
const RUNTIME_VM: &str = env!("CARGO_BIN_EXE_runtime-vm-module-under-test");
const STUBBORN: &str = env!("CARGO_BIN_EXE_stubborn-runtime-under-test");
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
    rig_with(&[]).await
}

/// [`rig`], with the module executables `extra` (source, file name) in its module directory too.
async fn rig_with(extra: &[(&str, &str)]) -> Rig {
    let root_dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(root_dir.path().join("projects")).unwrap());
    storage
        .create_project(&ProjectConfig::new("plant", "Plant"))
        .unwrap();
    storage.switch_active_project("plant").unwrap();

    let modules = tempfile::tempdir().unwrap();
    for &(source, name) in [
        (MODBUS, "mythos-module-modbus"),
        (RUNTIME_VM, "mythos-module-runtime-vm"),
    ]
    .iter()
    .chain(extra)
    {
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
            {"variable": "level", "type": "INT", "value": 20, "error": null, "forced": false},
            {"variable": "speed", "type": "INT", "value": 300, "error": null, "forced": false},
            {"variable": "flow", "type": null, "value": null,
             "error": "the program has no variable flow", "forced": false}
        ]),
        "{body}"
    );
    assert!(
        body["age_ms"].as_u64().is_some_and(|age| age <= 1_000),
        "the values come with their age: {body}"
    );
    assert_eq!(
        body["fresh_ms"], 1_000,
        "and how long an answer is given as current: {body}"
    );
    // How long before the read the program last completed a scan: a program in RUN past the
    // watchdog's two seconds would be faulted, so a scanning one is well inside it. The view's
    // tighter bound (two intervals and the longest scan) is not asserted here: a CI runner may
    // hold the scan thread back for a moment, and that is the views' business to show.
    assert!(
        body["state"]["scan_age_us"]
            .as_u64()
            .is_some_and(|age| age < 2_000_000),
        "the values come with the age of the scan that left them: {body}"
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
async fn an_engineer_forces_a_variable_everyone_sees_it_and_stopping_releases_it() {
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
    let (status, body) = rig
        .send(
            &rig.engineer,
            "PUT",
            "/api/v1/projects/plant/runtimes",
            Some(runtimes("level")),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    rig.speed_is("the program commands 300", 300).await;
    let tank = rig
        .tank_when("nothing is forced", |t| t["forced"] == json!([]))
        .await;
    assert_eq!(tank["forced_tags"], json!([]), "{tank}");

    let force = |variable: &str, value: Value| json!({"variable": variable, "value": value});

    // Forcing overrides the program: an operator may not, nor a viewer.
    for who in [&rig.viewer, &rig.operator] {
        let (status, _) = rig
            .send(
                who,
                "POST",
                "/api/v1/runtimes/tank/force",
                Some(force("speed", json!(444))),
            )
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, _) = rig
            .send(
                who,
                "POST",
                "/api/v1/runtimes/tank/release",
                Some(json!({})),
            )
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    // The engineer forces the output: the device is written the forced value, not the
    // program's, and the answer says what is forced.
    let (status, body) = rig
        .send(
            &rig.engineer,
            "POST",
            "/api/v1/runtimes/tank/force",
            Some(force("speed", json!(444))),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["state"]["forced"],
        json!([{"variable": "speed", "value": 444}]),
        "{body}"
    );
    rig.speed_is("speed forced to 444", 444).await;

    // Everyone sees it: the status lists the variable and every tag bound to it, and a watch
    // marks the value as forced.
    let tank = rig
        .tank_when("the status says speed is forced", |t| {
            t["forced_tags"] == json!([SPEED])
        })
        .await;
    assert_eq!(
        tank["forced"],
        json!([{"variable": "speed", "value": 444}]),
        "{tank}"
    );
    let (status, body) = rig
        .send(
            &rig.viewer,
            "POST",
            "/api/v1/runtimes/tank/watch",
            Some(json!({"variables": ["speed", "level"]})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["values"][0]["value"], 444, "{body}");
    assert_eq!(body["values"][0]["forced"], true, "{body}");
    assert_eq!(body["values"][1]["forced"], false, "{body}");

    // A forced setpoint holds against the operator; forcing an input holds against the field.
    let (status, body) = rig
        .send(
            &rig.engineer,
            "POST",
            "/api/v1/runtimes/tank/force",
            Some(force("setpoint", json!(40))),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = rig
        .send(
            &rig.operator,
            "PUT",
            &format!("/api/v1/tags/{T_SETPOINT}/value"),
            Some(json!({"value": 30})),
        )
        .await;
    assert_ne!(status, StatusCode::OK, "{body}");
    assert!(
        body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("forced"),
        "the operator is told why: {body}"
    );
    let tank = rig
        .tank_when("speed and setpoint are forced", |t| {
            t["forced_tags"] == json!([SPEED, T_SETPOINT])
        })
        .await;
    assert_eq!(tank["forced"].as_array().unwrap().len(), 2, "{tank}");

    // What cannot be forced is refused, saying why, and nothing changes.
    for (body, why) in [
        (force("flow", json!(1)), "the program has no variable flow"),
        (force("speed", json!(70_000)), "is not a value of speed"),
        (
            force("speed", json!("fast")),
            "must be a number or a boolean",
        ),
    ] {
        let (status, answer) = rig
            .send(
                &rig.engineer,
                "POST",
                "/api/v1/runtimes/tank/force",
                Some(body),
            )
            .await;
        assert_ne!(status, StatusCode::OK, "{answer}");
        assert!(
            answer["error"].as_str().unwrap_or_default().contains(why),
            "{answer}"
        );
    }
    let (status, _) = rig
        .send(
            &rig.engineer,
            "POST",
            "/api/v1/runtimes/nobody/force",
            Some(force("speed", json!(1))),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, body) = rig
        .send(
            &rig.engineer,
            "POST",
            "/api/v1/runtimes/tank/release",
            Some(json!({"variable": "level"})),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"], "level is not forced", "{body}");

    // Released, the program decides again: setpoint back to what the program holds (40 was
    // written into it while forced, and stays until the program changes it), speed from it.
    let (status, body) = rig
        .send(
            &rig.engineer,
            "POST",
            "/api/v1/runtimes/tank/release",
            Some(json!({"variable": "speed"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["state"]["forced"],
        json!([{"variable": "setpoint", "value": 40}]),
        "{body}"
    );
    rig.speed_is(
        "speed released: setpoint 40, level 20, the program commands 200",
        200,
    )
    .await;

    // Stopping releases every force: run again, and the program starts cold with none.
    let (status, _) = rig
        .send(&rig.engineer, "POST", "/api/v1/runtimes/tank/stop", None)
        .await;
    assert_eq!(status, StatusCode::OK);
    rig.speed_is("stopped: the safe value", SAFE_SPEED).await;
    let tank = rig
        .tank_when("stopped, nothing is forced", |t| {
            t["mode"] == "stop" && t["forced"] == json!([])
        })
        .await;
    assert_eq!(tank["forced_tags"], json!([]), "{tank}");
    let (status, body) = rig
        .send(
            &rig.engineer,
            "POST",
            "/api/v1/runtimes/tank/force",
            Some(force("speed", json!(444))),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("the program is not running"),
        "{body}"
    );
    let (status, _) = rig
        .send(&rig.engineer, "POST", "/api/v1/runtimes/tank/run", None)
        .await;
    assert_eq!(status, StatusCode::OK);
    rig.speed_is(
        "running again from a cold start: setpoint 50, nothing forced",
        300,
    )
    .await;

    // A forced input holds against the field: the device says 20, the program reads 45.
    let (status, body) = rig
        .send(
            &rig.engineer,
            "POST",
            "/api/v1/runtimes/tank/force",
            Some(force("level", json!(45))),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    rig.speed_is("level forced to 45: the program commands 50", 50)
        .await;

    // Releasing every force at once.
    let (status, body) = rig
        .send(
            &rig.engineer,
            "POST",
            "/api/v1/runtimes/tank/force",
            Some(force("speed", json!(9))),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    rig.speed_is("speed forced to 9", 9).await;
    let tank = rig
        .tank_when("two forced, level bound to one of them", |t| {
            t["forced"].as_array().is_some_and(|f| f.len() == 2)
        })
        .await;
    assert_eq!(tank["forced_tags"], json!([LEVEL, SPEED]), "{tank}");
    let (status, body) = rig
        .send(
            &rig.engineer,
            "POST",
            "/api/v1/runtimes/tank/release",
            Some(json!({})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["state"]["forced"], json!([]), "{body}");
    rig.speed_is("every force released: the program commands 300", 300)
        .await;

    // Loading the program again releases every force too.
    let (status, body) = rig
        .send(
            &rig.engineer,
            "POST",
            "/api/v1/runtimes/tank/force",
            Some(force("speed", json!(11))),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    rig.speed_is("speed forced to 11", 11).await;
    let mut again = runtimes("level");
    again["instances"][0]["program"]["version"] = json!("v2");
    let (status, body) = rig
        .send(
            &rig.engineer,
            "PUT",
            "/api/v1/projects/plant/runtimes",
            Some(again),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    rig.speed_is(
        "loaded again: nothing forced, the program commands 300",
        300,
    )
    .await;
    let tank = rig
        .tank_when("v2 runs with nothing forced", |t| {
            t["version"] == "v2" && t["forced"] == json!([])
        })
        .await;
    assert_eq!(tank["forced_tags"], json!([]), "{tank}");

    rig.io.shutdown().await;
}

fn kill(pid: u32) {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
    let pid = sysinfo::Pid::from_u32(pid);
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing(),
    );
    let process = system.process(pid).expect("the process to kill is running");
    assert!(process.kill(), "could not kill process {pid}");
}

/// A program with one RETAIN variable, `total`, and one that is not, `n`; both count scans.
/// The compiler marks `total` in the symbol table (`"retain": true`), as its RETAIN section says.
fn counter(version: &str) -> Value {
    const COUNTER: &str = "
PROGRAM Counter
VAR RETAIN total : DINT; END_VAR
VAR n : DINT; END_VAR
  total := total + 1;
  n := n + 1;
END_PROGRAM
";
    let compiled = compile(COUNTER, &TargetProfile::pc()).unwrap_or_else(|d| panic!("{d:?}"));
    let symbols = serde_json::to_value(&compiled.symbols).unwrap();
    let retained: Vec<&Value> = symbols
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["retain"] == true)
        .map(|s| &s["path"])
        .collect();
    assert_eq!(retained, vec![&json!("total")], "{symbols}");
    json!({"instances": [{
        "id": "counter", "module": "vm", "run": true,
        "program": {
            "name": compiled.name, "version": version, "interval_ms": 20,
            "code": hex::encode(&compiled.bytecode), "symbols": symbols,
        }
    }]})
}

impl Rig {
    /// `total` and `n` of the counter as its last scan left them, and its mode.
    async fn counted(&self) -> (i64, i64, String) {
        let (status, body) = self.watch_counter().await;
        assert_eq!(status, StatusCode::OK, "{body}");
        Self::counts(&body)
    }

    async fn watch_counter(&self) -> (StatusCode, Value) {
        self.send(
            &self.viewer,
            "POST",
            "/api/v1/runtimes/counter/watch",
            Some(json!({"variables": ["total", "n"]})),
        )
        .await
    }

    fn counts(body: &Value) -> (i64, i64, String) {
        (
            body["values"][0]["value"].as_i64().unwrap(),
            body["values"][1]["value"].as_i64().unwrap(),
            body["state"]["mode"].as_str().unwrap().to_string(),
        )
    }

    /// The counter, once `holds` says it is so. A 502 on the way is an answer the server keeps
    /// for up to `WATCH_FRESH` (1 s) after the module stopped answering: right after its process
    /// was killed and started again, the runtimes list can already say RUN while the watch still
    /// gives the old process's failure. It is asked again until the deadline, and a 502 still
    /// standing then fails the test with it.
    async fn counted_when(&self, what: &str, holds: impl Fn(i64, i64, &str) -> bool) -> (i64, i64) {
        self.counted_of(None, what, holds).await
    }

    /// [`Self::counted_when`], of the program `version` only. The server keeps a watch answer for
    /// up to `WATCH_FRESH` (1 s), so right after a load the runtimes list can already name the new
    /// program while the watch still gives the answer of the one before, which its
    /// `state.version` says. Such an answer is asked for again until the deadline.
    async fn counted_of(
        &self,
        version: Option<&str>,
        what: &str,
        holds: impl Fn(i64, i64, &str) -> bool,
    ) -> (i64, i64) {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let (status, body) = self.watch_counter().await;
            if status == StatusCode::BAD_GATEWAY {
                assert!(Instant::now() < deadline, "never: {what}; {body}");
                tokio::time::sleep(Duration::from_millis(20)).await;
                continue;
            }
            assert_eq!(status, StatusCode::OK, "{body}");
            if version.is_some_and(|v| body["state"]["version"] != v) {
                assert!(Instant::now() < deadline, "never: {what}; {body}");
                tokio::time::sleep(Duration::from_millis(20)).await;
                continue;
            }
            let (total, n, mode) = Self::counts(&body);
            if holds(total, n, &mode) {
                return (total, n);
            }
            assert!(
                Instant::now() < deadline,
                "never: {what}; total {total}, n {n}, mode {mode}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// What the server keeps for the counter's RETAIN variables, once it is there.
    async fn kept_when(&self, what: &str, holds: impl Fn(i64) -> bool) -> i64 {
        let file = self
            ._root
            .path()
            .join("projects/.mythos-system/retain/plant/counter.json");
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Ok(text) = std::fs::read_to_string(&file) {
                let kept: Value = serde_json::from_str(&text).unwrap();
                assert_eq!(kept[0]["path"], "total", "{kept}");
                assert_eq!(kept[0]["type"], "DINT", "{kept}");
                assert_eq!(
                    kept.as_array().unwrap().len(),
                    1,
                    "only the RETAIN one: {kept}"
                );
                let bytes = hex::decode(kept[0]["bytes"].as_str().unwrap()).unwrap();
                let total = i64::from(i32::from_le_bytes(bytes.try_into().unwrap()));
                if holds(total) {
                    return total;
                }
            }
            assert!(Instant::now() < deadline, "never kept: {what}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

#[tokio::test]
async fn a_retain_variable_keeps_its_value_across_a_stop_and_a_new_process_and_a_cold_start_resets_it(
) {
    let rig = rig().await;
    let (status, body) = rig
        .send(
            &rig.engineer,
            "PUT",
            "/api/v1/projects/plant/runtimes",
            Some(counter("v1")),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    rig.counted_when("it counts past 60", |total, _, mode| {
        mode == "run" && total > 60
    })
    .await;

    // Stopped and run again: a warm restart. The RETAIN variable goes on from where it was,
    // the other starts from its initial value.
    let (status, _) = rig
        .send(&rig.engineer, "POST", "/api/v1/runtimes/counter/stop", None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let (stopped, _) = rig
        .counted_when("it is stopped", |_, _, mode| mode == "stop")
        .await;
    let (status, body) = rig
        .send(&rig.engineer, "POST", "/api/v1/runtimes/counter/run", None)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (total, n) = rig
        .counted_when("it runs again", |_, n, mode| mode == "run" && n > 0)
        .await;
    assert!(total > stopped, "total went on from {stopped}: {total}");
    assert!(
        n < stopped,
        "n started again from 0: {n}, stopped at {stopped}"
    );

    // The server keeps the value. Loading another version of the program while it runs is an
    // online change (C6b): both counters go on from the running values, neither restarts.
    let kept = rig
        .kept_when("a value past the one it stopped with", |t| t > stopped)
        .await;
    let (total_before, n_before, _) = rig.counted().await;
    let (status, body) = rig
        .send(
            &rig.engineer,
            "PUT",
            "/api/v1/projects/plant/runtimes",
            Some(counter("v2")),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let (_, body) = rig.send(&rig.viewer, "GET", "/api/v1/runtimes", None).await;
        if body["runtimes"][0]["version"] == "v2" && body["runtimes"][0]["mode"] == "run" {
            break;
        }
        assert!(Instant::now() < deadline, "v2 never ran: {body}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(body["changes"]["runtimes"][0]["start"], "online", "{body}");
    let (total, n) = rig
        .counted_when("v2 scans", |_, n, mode| mode == "run" && n > 0)
        .await;
    assert!(
        total >= total_before && n >= n_before,
        "v2 went on from the running values ({total_before}, {n_before}): {total}, n {n}"
    );

    // Its module process dies; the one started in its place takes the values kept last, not
    // those v2 was loaded with.
    let later = rig
        .kept_when("a value well past the one v2 was loaded with", |t| {
            t > kept + 100
        })
        .await;
    let (_, body) = rig.send(&rig.viewer, "GET", "/api/v1/runtimes", None).await;
    let pid = body["runtimes"][0]["pid"].as_u64().unwrap() as u32;
    kill(pid);
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let (_, body) = rig.send(&rig.viewer, "GET", "/api/v1/runtimes", None).await;
        let runtime = &body["runtimes"][0];
        if runtime["restarts"].as_u64() >= Some(1)
            && runtime["mode"] == "run"
            && runtime["pid"].as_u64().is_some_and(|p| p as u32 != pid)
        {
            break;
        }
        assert!(Instant::now() < deadline, "never started again: {body}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let (total, n) = rig
        .counted_when("the new process scans", |_, n, mode| mode == "run" && n > 0)
        .await;
    assert!(
        total >= later + n,
        "the new process went on from what was kept last ({later}): {total}, n {n}"
    );

    // Stopped, and loaded again in STOP: what is kept stays as it was while it waits, and
    // RUN takes it. (The engineer's last command holds across a load, so it is a stop.)
    let (status, _) = rig
        .send(&rig.engineer, "POST", "/api/v1/runtimes/counter/stop", None)
        .await;
    assert_eq!(status, StatusCode::OK);
    rig.counted_when("it is stopped", |_, _, mode| mode == "stop")
        .await;
    let mut stopped_load = counter("v3");
    stopped_load["instances"][0]["run"] = json!(false);
    let (status, body) = rig
        .send(
            &rig.engineer,
            "PUT",
            "/api/v1/projects/plant/runtimes",
            Some(stopped_load),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    tokio::time::sleep(Duration::from_millis(2_500)).await;
    let waiting = rig.kept_when("what v2 left", |t| t > later).await;
    tokio::time::sleep(Duration::from_millis(2_500)).await;
    assert_eq!(
        rig.kept_when("the same while it waits", |_| true).await,
        waiting,
        "a program that has not run yet does not replace what is kept"
    );
    let (status, _) = rig
        .send(&rig.engineer, "POST", "/api/v1/runtimes/counter/run", None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let (total, n) = rig
        .counted_when("v3 runs", |_, n, mode| mode == "run" && n > 0)
        .await;
    assert!(
        total >= waiting + n,
        "v3 went on from what was kept ({waiting}): {total}, n {n}"
    );

    // Only an engineer starts a program cold.
    for who in [&rig.viewer, &rig.operator] {
        let (status, _) = rig
            .send(who, "POST", "/api/v1/runtimes/counter/cold-start", None)
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    // A cold start, of a program in RUN: every variable from its initial value, and what is
    // kept follows.
    let before = rig.counted().await.0;
    let (status, body) = rig
        .send(
            &rig.engineer,
            "POST",
            "/api/v1/runtimes/counter/cold-start",
            Some(json!({})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["asked"], "cold start", "{body}");
    let (total, _) = rig
        .counted_when("it started cold", |total, _, mode| {
            mode == "run" && total < before
        })
        .await;
    assert!(total < before, "{total} < {before}");
    rig.kept_when("the cold start's count", |t| t < before)
        .await;
    let (status, _) = rig
        .send(
            &rig.engineer,
            "POST",
            "/api/v1/runtimes/nobody/cold-start",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    rig.io.shutdown().await;
}

/// The counter of an online change, in three builds. v1 and v2 are the program `Counter`: a
/// RETAIN counter `total`, a plain one `n`, an on-delay timer `t` timing an hour, and the
/// output `speed`, 300 more than `n`, so it is never the safe value. v1 also has `old_only`;
/// v2 has `added` in its place, starting at 42. `other_name` is v2 named `Tally`.
fn changing(build: &str, version: &str) -> Value {
    let (name, extra, step) = match build {
        "v1" => ("Counter", "old_only : INT;", "old_only := old_only + 1;"),
        "v2" => ("Counter", "added : INT := 42;", ""),
        "other_name" => ("Tally", "added : INT := 42;", ""),
        other => panic!("no build {other}"),
    };
    let source = format!(
        "
PROGRAM {name}
VAR RETAIN total : DINT; END_VAR
VAR n : DINT; t : TON; {extra} END_VAR
VAR_OUTPUT speed : INT; END_VAR
  total := total + 1;
  n := n + 1;
  {step}
  t(IN := TRUE, PT := T#1h);
  speed := 300 + DINT_TO_INT(n);
END_PROGRAM
"
    );
    let compiled = compile(&source, &TargetProfile::pc()).unwrap_or_else(|d| panic!("{d:?}"));
    json!({"instances": [{
        "id": "counter", "module": "vm", "run": true,
        "program": {
            "name": compiled.name, "version": version, "interval_ms": 20,
            "code": hex::encode(&compiled.bytecode), "symbols": compiled.symbols,
            "outputs": [{"variable": "speed", "tag": SPEED, "safe": SAFE_SPEED}],
        }
    }]})
}

impl Rig {
    /// Variables of the counter as its last scan left them, by name, or why there is none.
    async fn counter_values(&self, variables: &[&str]) -> Vec<Value> {
        let (status, body) = self
            .send(
                &self.viewer,
                "POST",
                "/api/v1/runtimes/counter/watch",
                Some(json!({ "variables": variables })),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["values"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["value"].clone())
            .collect()
    }

    /// Load `config`, sampling the speed register as often as it can from before the request
    /// until `version` has run 20 scans: every value the device held through the swap.
    async fn load_watching_the_output(
        &self,
        uri: &str,
        config: Value,
        version: &str,
    ) -> ((StatusCode, Value), Vec<Option<u16>>) {
        let done = std::sync::atomic::AtomicBool::new(false);
        let sampler = async {
            let mut seen = Vec::new();
            while !done.load(Ordering::SeqCst) {
                seen.push(self.sim.holding(1).await);
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            seen
        };
        let load = async {
            let answer = self.send(&self.engineer, "PUT", uri, Some(config)).await;
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                let (_, body) = self
                    .send(&self.viewer, "GET", "/api/v1/runtimes", None)
                    .await;
                let runtime = &body["runtimes"][0];
                if runtime["version"] == version
                    && runtime["mode"] == "run"
                    && runtime["scans"].as_u64() > Some(20)
                {
                    break;
                }
                if Instant::now() > deadline {
                    done.store(true, Ordering::SeqCst);
                    panic!("{version} never ran 20 scans: {body}; the load answered {answer:?}");
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            done.store(true, Ordering::SeqCst);
            answer
        };
        tokio::join!(load, sampler)
    }
}

#[tokio::test]
async fn a_new_build_of_a_running_program_is_changed_online_and_a_cold_load_or_another_program_starts_as_before(
) {
    const MS: i64 = 1_000_000;
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
    let (status, body) = rig
        .send(
            &rig.engineer,
            "PUT",
            "/api/v1/projects/plant/runtimes",
            Some(changing("v1", "v1")),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // A program loaded where none ran starts as it always did, and says why.
    let load = &body["changes"]["runtimes"][0];
    assert_eq!(load["runtime"], "counter", "{body}");
    assert_eq!(load["start"], "warm", "{body}");
    assert_eq!(load["why"], "no build of it was loaded", "{body}");
    let (total_v1, n_v1) = rig
        .counted_when("v1 counts past 100", |total, n, mode| {
            mode == "run" && total > 100 && n > 100
        })
        .await;
    let deadline = Instant::now() + Duration::from_secs(15);
    let speed_v1 = loop {
        match rig.sim.holding(1).await {
            Some(speed) if speed > 300 + 100 => break speed,
            other => assert!(
                Instant::now() < deadline,
                "v1 never commanded 300 + n: {other:?}"
            ),
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    let et_v1 = rig.counter_values(&["t.ET"]).await[0].as_i64().unwrap();
    assert!(
        et_v1 > 2_000 * MS,
        "the timer has timed two seconds: {et_v1}"
    );

    // A start that is not a way to start is refused, and nothing is saved or loaded.
    let (status, body) = rig
        .send(
            &rig.engineer,
            "PUT",
            "/api/v1/projects/plant/runtimes?start=hot",
            Some(changing("v2", "v2")),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["error"].as_str().unwrap().contains("start=hot"),
        "{body}"
    );

    // v2 while v1 runs: changed online.
    let ((status, body), seen) = rig
        .load_watching_the_output(
            "/api/v1/projects/plant/runtimes",
            changing("v2", "v2"),
            "v2",
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["changes"]["swapped"],
        json!(["runtime:counter"]),
        "{body}"
    );
    let load = &body["changes"]["runtimes"][0];
    assert_eq!(load["runtime"], "counter", "{body}");
    assert_eq!(load["start"], "online", "{body}");
    assert_eq!(load["why"], Value::Null, "{body}");
    assert_eq!(
        load["dropped"],
        json!([{"variable": "old_only", "why": "this build has no variable of that name"}]),
        "{body}"
    );
    assert_eq!(
        load["initialized"],
        json!([{"variable": "added", "why": "the running program had no variable of that name"}]),
        "{body}"
    );
    // total, n, speed and the seven variables of the timer.
    assert_eq!(load["carried"], 10, "{body}");

    // The device was never told the safe value: the new build took the output over from the
    // old one without a gap, and its first scan wrote it from the carried state.
    // Sampled from before the load was asked until the new build had run 20 scans of 20 ms, so
    // the samples span the swap. How many there are depends on the OS's timer: a 1 ms sleep
    // lasts about 15 ms on Windows (48 samples there, where Linux takes hundreds). Ten is
    // enough to say the window was watched, and is reached on both.
    assert!(
        seen.len() >= 10,
        "the register was sampled only {} times through the swap",
        seen.len()
    );
    assert!(
        seen.iter().all(|s| *s != Some(SAFE_SPEED)),
        "the safe value {SAFE_SPEED} was written during the swap: {seen:?}"
    );
    let lowest = seen.iter().flatten().min().copied().unwrap();
    assert!(
        lowest >= speed_v1,
        "the speed never went back below what v1 commanded before the load ({speed_v1}): {lowest}"
    );

    // Both counters went on, neither restarted; the timer timed on, on a clock that went on;
    // the new variable started from its initial value.
    let values = rig.counter_values(&["total", "n", "t.ET", "added"]).await;
    let (total, n, et) = (
        values[0].as_i64().unwrap(),
        values[1].as_i64().unwrap(),
        values[2].as_i64().unwrap(),
    );
    assert!(
        total > total_v1 + 20,
        "the RETAIN counter went on from {total_v1}: {total}"
    );
    assert!(n > n_v1 + 20, "the plain counter went on from {n_v1}: {n}");
    assert!(et > et_v1, "the timer went on from {et_v1}: {et}");
    assert_eq!(values[3], json!(42), "added took its initial value");

    // A cold load of v2 again: both counters from zero, the RETAIN one too, as a cold start
    // does, and the report says cold.
    let before = rig.counted().await;
    let ((status, body), seen) = rig
        .load_watching_the_output(
            "/api/v1/projects/plant/runtimes?start=cold",
            changing("v2", "v2-cold"),
            "v2-cold",
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let load = &body["changes"]["runtimes"][0];
    assert_eq!(load["start"], "cold", "{body}");
    assert_eq!(load["why"], "a cold load was asked for", "{body}");
    assert_eq!(load["carried"], Value::Null, "{body}");
    assert!(
        seen.iter().all(|s| *s != Some(SAFE_SPEED)),
        "a cold load hands the output over too: {seen:?}"
    );
    let (total, n) = rig
        .counted_when("it started cold", |_, n, mode| mode == "run" && n > 0)
        .await;
    assert!(
        total < before.0 && n < before.1,
        "both counters started again: total {total} (was {}), n {n} (was {})",
        before.0,
        before.1
    );
    assert!(
        total <= n + 1,
        "the RETAIN one from zero, as n: {total}, n {n}"
    );
    rig.kept_when("the cold load's count", |t| t < before.0)
        .await;

    // A program of another name in its place starts as before: the RETAIN counter from what
    // was kept for the runtime, the plain one from zero.
    let (running_total, _) = rig
        .counted_when("it counts past 100 again", |total, _, mode| {
            mode == "run" && total > 100
        })
        .await;
    let kept = rig.kept_when("past 100", |t| t > 100).await;
    let (status, body) = rig
        .send(
            &rig.engineer,
            "PUT",
            "/api/v1/projects/plant/runtimes",
            Some(changing("other_name", "tally")),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let load = &body["changes"]["runtimes"][0];
    assert_eq!(load["start"], "warm", "{body}");
    assert_eq!(
        load["why"], "the program running was 'Counter', not 'Tally'",
        "{body}"
    );
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let (_, body) = rig.send(&rig.viewer, "GET", "/api/v1/runtimes", None).await;
        if body["runtimes"][0]["version"] == "tally" && body["runtimes"][0]["mode"] == "run" {
            break;
        }
        assert!(Instant::now() < deadline, "Tally never ran: {body}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // Of Tally itself: the answer the server still keeps of Counter, for up to a second after
    // the load, has Counter's total and n (seen on Windows: "total 104; n 104" against 101 kept).
    let (total, n) = rig
        .counted_of(Some("tally"), "Tally scans", |_, n, mode| {
            mode == "run" && n > 0
        })
        .await;
    assert!(
        total >= kept + n && n < running_total,
        "a warm restart: total from what was kept ({kept}): {total}; n from zero: {n}"
    );

    rig.io.shutdown().await;
}

/// A program with a RETAIN on-delay timer, `kept`, and one that is not, `plain`, both timing
/// an hour from the first scan. Since L4 a timer's state (when it started, whether it runs) is
/// variables of the instance, so RETAIN reaches it (ADR-013 §2.13).
fn timers() -> Value {
    const TIMERS: &str = "
PROGRAM Timers
VAR RETAIN kept : TON; END_VAR
VAR plain : TON; END_VAR
  kept(IN := TRUE, PT := T#1h);
  plain(IN := TRUE, PT := T#1h);
END_PROGRAM
";
    let compiled = compile(TIMERS, &TargetProfile::pc()).unwrap_or_else(|d| panic!("{d:?}"));
    let symbols = serde_json::to_value(&compiled.symbols).unwrap();
    let retained: Vec<&str> = symbols
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["retain"] == true)
        .map(|s| s["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        retained,
        [
            "kept.IN",
            "kept.PT",
            "kept.Q",
            "kept.ET",
            "kept.RUNNING",
            "kept.START",
            "kept.CLOCK"
        ],
        "every member of the RETAIN instance, its internal state too"
    );
    json!({"instances": [{
        "id": "timers", "module": "vm", "run": true,
        "program": {
            "name": compiled.name, "version": "v1", "interval_ms": 20,
            "code": hex::encode(&compiled.bytecode), "symbols": symbols,
        }
    }]})
}

impl Rig {
    /// The ET of both timers, in nanoseconds, and the mode.
    async fn timed(&self) -> (i64, i64, String) {
        let (status, body) = self
            .send(
                &self.viewer,
                "POST",
                "/api/v1/runtimes/timers/watch",
                Some(json!({"variables": ["kept.ET", "plain.ET"]})),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        (
            body["values"][0]["value"].as_i64().unwrap(),
            body["values"][1]["value"].as_i64().unwrap(),
            body["state"]["mode"].as_str().unwrap().to_string(),
        )
    }

    async fn timed_when(&self, what: &str, holds: impl Fn(i64, i64, &str) -> bool) -> (i64, i64) {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let (kept, plain, mode) = self.timed().await;
            if holds(kept, plain, &mode) {
                return (kept, plain);
            }
            assert!(
                Instant::now() < deadline,
                "never: {what}; kept {kept}, plain {plain}, mode {mode}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

#[tokio::test]
async fn a_retain_timer_keeps_its_elapsed_time_across_a_stop_and_run() {
    const MS: i64 = 1_000_000;
    let rig = rig().await;
    let (status, body) = rig
        .send(
            &rig.engineer,
            "PUT",
            "/api/v1/projects/plant/runtimes",
            Some(timers()),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    rig.timed_when("it has timed past 1.5 s", |kept, _, mode| {
        mode == "run" && kept > 1_500 * MS
    })
    .await;

    // Stopped long enough that the core keeps what it last read (every 2 s), and then run
    // again: a warm restart, on a scan clock that starts again from zero.
    let (status, _) = rig
        .send(&rig.engineer, "POST", "/api/v1/runtimes/timers/stop", None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let (stopped, _) = rig
        .timed_when("it is stopped", |_, _, mode| mode == "stop")
        .await;
    tokio::time::sleep(Duration::from_millis(2_500)).await;
    let (status, body) = rig
        .send(&rig.engineer, "POST", "/api/v1/runtimes/timers/run", None)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (kept, plain) = rig
        .timed_when("it runs again", |_, plain, mode| mode == "run" && plain > 0)
        .await;
    // The RETAIN timer carries on from its ET; the time the program was stopped is not timed
    // (the scan clock does not run then), and nothing is negative or past PT.
    assert!(
        kept >= stopped + plain && kept < stopped + plain + 500 * MS,
        "kept went on from {stopped}: {kept}, with plain at {plain}"
    );
    // The other timer started again from zero with its IN.
    assert!(plain < stopped, "plain timed again from 0: {plain}");

    // A cold start resets the RETAIN timer too.
    let (status, body) = rig
        .send(
            &rig.engineer,
            "POST",
            "/api/v1/runtimes/timers/cold-start",
            Some(json!({})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    rig.timed_when("it started cold", |kept, _, mode| {
        mode == "run" && kept < stopped
    })
    .await;

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

/// The runtime `counter` of the programs `sources`, each in a cyclic task of its own named
/// after it; the first program, Count, drives the speed.
fn grouped(sources: &[&str], version: &str) -> Value {
    let mut programs = Vec::new();
    let mut tasks = Vec::new();
    for (k, source) in sources.iter().enumerate() {
        let compiled = compile(source, &TargetProfile::pc()).unwrap_or_else(|d| panic!("{d:?}"));
        let outputs = if k == 0 {
            json!([{"variable": "speed", "tag": SPEED, "safe": SAFE_SPEED}])
        } else {
            json!([])
        };
        tasks.push(json!({
            "name": format!("t{k}"), "kind": {"type": "cyclic", "interval_ms": 20 + 10 * k as u64},
            "priority": k, "programs": [compiled.name.clone()]
        }));
        programs.push(json!({
            "name": compiled.name, "version": version, "interval_ms": 20,
            "code": hex::encode(&compiled.bytecode), "symbols": compiled.symbols,
            "outputs": outputs
        }));
    }
    if programs.len() == 1 {
        return json!({"instances": [{
            "id": "counter", "module": "vm", "run": true, "program": programs[0]
        }]});
    }
    json!({"instances": [{
        "id": "counter", "module": "vm", "run": true, "programs": programs, "tasks": tasks
    }]})
}

const COUNT: &str = "
PROGRAM Count
VAR n : DINT; END_VAR
VAR_OUTPUT speed : INT; END_VAR
  n := n + 1;
  speed := 300 + DINT_TO_INT(n MOD 100);
END_PROGRAM
";
const IDLE: &str = "PROGRAM Idle VAR k : DINT; END_VAR k := k + 1; END_PROGRAM";
const OTHER: &str = "PROGRAM Other VAR k : DINT; END_VAR k := k + 2; END_PROGRAM";

impl Rig {
    /// Load `config` into the active project, and how the runtime started.
    async fn load_counter(&self, config: Value) -> Value {
        let (status, body) = self
            .send(
                &self.engineer,
                "PUT",
                "/api/v1/projects/plant/runtimes",
                Some(config),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let load = body["changes"]["runtimes"][0].clone();
        assert_eq!(load["runtime"], "counter", "{body}");
        load
    }

    /// Wait until the runtime of `version` runs and has scanned past 20.
    async fn runs(&self, version: &str) {
        self.tank_when(&format!("{version} runs"), |t| {
            t["version"] == version && t["mode"] == "run" && t["scans"].as_u64() > Some(20)
        })
        .await;
    }
}

#[tokio::test]
async fn an_online_change_needs_the_same_programs() {
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

    let load = rig.load_counter(grouped(&[COUNT, IDLE], "v1")).await;
    assert_eq!(
        (&load["start"], &load["why"]),
        (&json!("warm"), &json!("no build of it was loaded")),
        "{load}"
    );
    rig.runs("v1, v1").await;
    let tank = rig.tank().await;
    assert_eq!(tank["program"], "Count, Idle", "{tank}");
    assert_eq!(tank["tasks"].as_array().map(Vec::len), Some(2), "{tank}");

    // A new build of the same two programs: changed online.
    let load = rig.load_counter(grouped(&[COUNT, IDLE], "v2")).await;
    assert_eq!(load["start"], "online", "{load}");
    assert_eq!(load["why"], Value::Null, "{load}");
    assert!(load["carried"].as_u64() > Some(0), "{load}");
    rig.runs("v2, v2").await;

    // Another program in place of one of them: not the same programs, so as at a warm start,
    // and it says which programs ran.
    let load = rig.load_counter(grouped(&[COUNT, OTHER], "v3")).await;
    assert_eq!(load["start"], "warm", "{load}");
    assert_eq!(
        load["why"], "the program running was 'Count,Idle', not 'Count,Other'",
        "{load}"
    );
    rig.runs("v3, v3").await;

    // One program of the two alone: its name is its key.
    let load = rig.load_counter(grouped(&[COUNT], "v4")).await;
    assert_eq!(load["start"], "warm", "{load}");
    assert_eq!(
        load["why"], "the program running was 'Count,Other', not 'Count'",
        "{load}"
    );
    rig.runs("v4").await;
    rig.io.shutdown().await;
}

#[tokio::test]
async fn a_save_for_an_older_runtime_module_is_refused_and_says_why() {
    let rig = rig_with(&[(STUBBORN, "mythos-module-stubborn")]).await;
    let mut config = grouped(&[COUNT, IDLE], "v1");
    config["instances"][0]["module"] = json!("stubborn");
    let (status, body) = rig
        .send(
            &rig.engineer,
            "PUT",
            "/api/v1/projects/plant/runtimes",
            Some(config),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let message = body["problems"][0]["message"].as_str().unwrap_or("");
    assert!(
        message.contains("runtime 'counter' has 2 programs; the runtime module 'stubborn' (version 0.0.0) runs one program in one cyclic task and does not say it runs tasks: install a newer build of it"),
        "{body}"
    );
    // Nothing was saved.
    let (status, body) = rig
        .send(
            &rig.engineer,
            "GET",
            "/api/v1/projects/plant/runtimes",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["config"], Value::Null, "{body}");
    rig.io.shutdown().await;
}

// -----------------------------------------------------------------------------
// Data blocks shared by the programs of one runtime (ADR-014, D2)
// -----------------------------------------------------------------------------

const COUNT_TYPES: &str = "TYPE Count : STRUCT v : DINT; END_STRUCT; END_TYPE";
const COUNT_BLOCK: DataBlock = DataBlock {
    name: "S",
    ty: "Count",
};
/// Counts in the block (`S.v`), and in a variable of its own that is never kept (`n`).
const BUMPER: &str = "PROGRAM Bumper
VAR n : DINT; END_VAR
VAR_EXTERNAL S : Count; END_VAR
  S.v := S.v + 1;
  n := n + 1;
END_PROGRAM";
/// Takes what Bumper handed over.
const TAKER: &str = "PROGRAM Taker
VAR got : DINT; END_VAR
VAR_EXTERNAL S : Count; END_VAR
  got := S.v;
END_PROGRAM";

fn count_program(source: &str, version: &str) -> Value {
    let c = compile_sources(
        &Sources {
            st: Some(("main.st", source)),
            library: &[("types.st", COUNT_TYPES)],
            data: &[COUNT_BLOCK],
            ..Default::default()
        },
        &TargetProfile::pc(),
    )
    .unwrap_or_else(|d| panic!("{d:?}"));
    json!({
        "name": c.name, "version": version, "interval_ms": 20,
        "code": hex::encode(&c.bytecode), "symbols": c.symbols, "data": c.data,
    })
}

/// The block S as the project lays it out, kept across a warm restart when `retain`.
fn count_block(retain: bool) -> Value {
    let l = check_project(&Sources {
        library: &[("types.st", COUNT_TYPES)],
        data: &[COUNT_BLOCK],
        ..Default::default()
    })
    .unwrap_or_else(|d| panic!("{d:?}"))
    .remove(0);
    json!({
        "name": l.name, "type": l.ty, "layout": l.layout, "size": l.size, "retain": retain,
        "members": l.members.iter().map(|m| json!({
            "name": m.name, "type": m.ty, "offset": m.offset, "size": m.size,
        })).collect::<Vec<_>>(),
    })
}

/// The runtime `shared`: Bumper every 20 ms, Taker every 30 ms, sharing S.
fn sharing(version: &str, retain: bool) -> Value {
    json!({"instances": [{
        "id": "shared", "module": "vm", "run": true,
        "programs": [count_program(BUMPER, version), count_program(TAKER, version)],
        "tasks": [
            {"name": "b", "kind": {"type": "cyclic", "interval_ms": 20}, "priority": 1, "programs": ["Bumper"]},
            {"name": "t", "kind": {"type": "cyclic", "interval_ms": 30}, "priority": 2, "programs": ["Taker"]}
        ],
        "data": [count_block(retain)]
    }]})
}

impl Rig {
    async fn save(&self, config: Value) -> (StatusCode, Value) {
        self.send(
            &self.engineer,
            "PUT",
            "/api/v1/projects/plant/runtimes",
            Some(config),
        )
        .await
    }

    /// `Bumper.S.v`, `Bumper.n` and `Taker.got` as their last runs left them, once `holds`
    /// says so. A 502 on the way (the old process's failure, kept up to 1 s) is asked again.
    async fn shared_when(
        &self,
        what: &str,
        holds: impl Fn(i64, i64, i64) -> bool,
    ) -> (i64, i64, i64) {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let (status, body) = self
                .send(
                    &self.viewer,
                    "POST",
                    "/api/v1/runtimes/shared/watch",
                    Some(json!({"variables": ["Bumper.S.v", "Bumper.n", "Taker.got"]})),
                )
                .await;
            if status == StatusCode::OK && body["state"]["mode"] == "run" {
                let at = |k: usize| body["values"][k]["value"].as_i64();
                if let (Some(v), Some(n), Some(got)) = (at(0), at(1), at(2)) {
                    if holds(v, n, got) {
                        return (v, n, got);
                    }
                }
            }
            assert!(Instant::now() < deadline, "never: {what}; {status} {body}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// What the server keeps for the block S of `shared`, once `holds` says so.
    async fn kept_block_when(
        &self,
        what: &str,
        layout: &Value,
        holds: impl Fn(i64) -> bool,
    ) -> i64 {
        let file = self
            ._root
            .path()
            .join("projects/.mythos-system/retain/plant/shared.json");
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Ok(text) = std::fs::read_to_string(&file) {
                let kept: Value = serde_json::from_str(&text).unwrap();
                let block = kept
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|k| k["path"] == "S")
                    .unwrap_or_else(|| panic!("the block is kept: {kept}"));
                assert_eq!(block["type"], "Count", "{kept}");
                assert_eq!(&block["layout"], layout, "{kept}");
                let bytes = hex::decode(block["bytes"].as_str().unwrap()).unwrap();
                let v = i64::from(i32::from_le_bytes(bytes.try_into().unwrap()));
                if holds(v) {
                    return v;
                }
            }
            assert!(Instant::now() < deadline, "never kept: {what}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

#[tokio::test]
async fn a_save_with_a_data_mistake_is_refused_with_its_message_and_nothing_is_saved() {
    let rig = rig().await;
    // Taker writes S.v too: two writers of one member.
    let mut two_writers = sharing("v1", false);
    two_writers["instances"][0]["programs"][1]["data"][0]["members"][0]["written"] = json!(true);
    // The block S in a second runtime too.
    let mut two_runtimes = sharing("v1", false);
    let mut other = two_runtimes["instances"][0].clone();
    other["id"] = json!("other");
    two_runtimes["instances"]
        .as_array_mut()
        .unwrap()
        .push(other);
    for (config, message) in [
        (two_writers, "runtime 'shared': data block 'S': member 'v' is written by programs 'Bumper' and 'Taker'; a member has one writer"),
        (two_runtimes, "data block 'S' is in runtimes 'shared' and 'other'; a data block shared between runtimes is not built yet"),
    ] {
        let (status, body) = rig.save(config).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(body["error"], message, "{body}");
    }
    let (status, body) = rig
        .send(
            &rig.engineer,
            "GET",
            "/api/v1/projects/plant/runtimes",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["config"], Value::Null, "{body}");
    rig.io.shutdown().await;
}

#[tokio::test]
async fn a_save_of_a_program_sent_without_the_data_its_code_declares_is_refused() {
    let rig = rig().await;
    // Taker sent with its code and symbols only. The core does not read symbols (they are the
    // module's own); the save asks the module, which loads the program and refuses it.
    let mut config = sharing("v1", false);
    config["instances"][0]["programs"][1]
        .as_object_mut()
        .unwrap()
        .remove("data");
    let (status, body) = rig.save(config).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        body["error"], "the runtimes configuration has problems and was not saved",
        "{body}"
    );
    let message = body["problems"][0]["message"].as_str().unwrap_or("");
    assert!(
        message.contains("program 'Taker': its code declares data block 'S' (S")
            && message.contains("), and its data does not"),
        "{body}"
    );
    let (status, body) = rig
        .send(
            &rig.engineer,
            "GET",
            "/api/v1/projects/plant/runtimes",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["config"], Value::Null, "{body}");
    rig.io.shutdown().await;
}

#[tokio::test]
async fn a_save_with_data_blocks_for_an_older_runtime_module_is_refused_and_says_why() {
    let rig = rig_with(&[(STUBBORN, "mythos-module-stubborn")]).await;
    let mut config = grouped(&[COUNT], "v1");
    config["instances"][0]["module"] = json!("stubborn");
    config["instances"][0]["data"] = json!([count_block(false)]);
    let (status, body) = rig.save(config).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let message = body["problems"][0]["message"].as_str().unwrap_or("");
    assert!(
        message.contains("runtime 'counter' has data blocks (S); the runtime module 'stubborn' (version 0.0.0) does not say it hands data blocks over: install a newer build of it"),
        "{body}"
    );
    rig.io.shutdown().await;
}

#[tokio::test]
async fn a_retained_block_survives_a_killed_process() {
    let rig = rig().await;
    let (status, body) = rig.save(sharing("v1", true)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let layout = count_block(true)["layout"].clone();
    rig.shared_when("Bumper counts past 60 and Taker follows", |v, _, got| {
        v > 60 && got > 0
    })
    .await;
    let tank = rig.tank().await;
    assert_eq!(tank["data"][0]["retain"], true, "{tank}");
    assert_eq!(tank["data"][0]["members"][0]["writer"], "Bumper", "{tank}");

    // The server keeps the block's master, under its layout.
    let kept = rig
        .kept_block_when("the block counted past 60", &layout, |v| v > 60)
        .await;
    let pid = tank["pid"].as_u64().unwrap() as u32;
    kill(pid);
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let runtime = rig.tank().await;
        if runtime["restarts"].as_u64() >= Some(1)
            && runtime["mode"] == "run"
            && runtime["pid"].as_u64().is_some_and(|p| p as u32 != pid)
        {
            break;
        }
        assert!(Instant::now() < deadline, "never started again: {runtime}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // Bumper's count went on from what was kept (n, never kept, says how far since), and
    // Taker took it.
    let (v, n, _) = rig
        .shared_when("the new process counts", |_, n, _| n > 0)
        .await;
    assert!(
        v >= kept + n,
        "the block went on from what was kept ({kept}): {v}, n {n}"
    );
    rig.shared_when("Taker takes the count that went on", |_, _, got| got > kept)
        .await;
    rig.io.shutdown().await;
}

#[tokio::test]
async fn an_online_change_carries_a_block_through_the_core() {
    let rig = rig().await;
    // Not RETAIN: only the online change can carry it on.
    let (status, body) = rig.save(sharing("v1", false)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (before, _, _) = rig
        .shared_when("Bumper counts past 60", |v, _, got| v > 60 && got > 0)
        .await;
    let (status, body) = rig.save(sharing("v2", false)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["changes"]["runtimes"][0]["start"], "online", "{body}");
    rig.tank_when("v2 runs", |t| {
        t["version"] == "v2, v2" && t["mode"] == "run"
    })
    .await;
    // Its counter goes on: not reset to 0 by the new build, in the writer and in the reader.
    let (v, n, got) = rig
        .shared_when("v2 counts", |_, n, got| n > before && got > 0)
        .await;
    assert!(
        v >= before && got >= before,
        "went on from {before}: S.v {v}, got {got}, n {n}"
    );
    rig.io.shutdown().await;
}
