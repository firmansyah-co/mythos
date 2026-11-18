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
        let (status, body) = self
            .send(
                &self.viewer,
                "POST",
                "/api/v1/runtimes/counter/watch",
                Some(json!({"variables": ["total", "n"]})),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        (
            body["values"][0]["value"].as_i64().unwrap(),
            body["values"][1]["value"].as_i64().unwrap(),
            body["state"]["mode"].as_str().unwrap().to_string(),
        )
    }

    /// The counter, once `holds` says it is so.
    async fn counted_when(&self, what: &str, holds: impl Fn(i64, i64, &str) -> bool) -> (i64, i64) {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let (total, n, mode) = self.counted().await;
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

    // The server keeps the value, and a new module process starts from it: loading the
    // program again (another version) is a warm restart too.
    let kept = rig
        .kept_when("a value past the one it stopped with", |t| t > stopped)
        .await;
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
    let (total, n) = rig
        .counted_when("v2 scans", |_, n, mode| mode == "run" && n > 0)
        .await;
    assert!(
        total >= kept + n,
        "v2 went on from what was kept ({kept}): {total}, n {n}"
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
