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

/// The tank in Ladder Diagram (ADR-013 §2.15): `speed := level * 10` through a MUL block, and
/// a contact on `level_ok` to the coil `running`. `{contact}` is the contact's variable.
const MIXER: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<project xmlns="http://www.plcopen.org/xml/tc6_0201">
  <types><dataTypes/><pous>
    <pou name="Mixer" pouType="program">
      <interface>
        <inputVars>
          <variable name="level"><type><INT/></type></variable>
          <variable name="level_ok"><type><BOOL/></type></variable>
        </inputVars>
        <outputVars>
          <variable name="speed"><type><INT/></type></variable>
          <variable name="running"><type><BOOL/></type></variable>
        </outputVars>
      </interface>
      <body><LD>
        <leftPowerRail localId="1"><position x="0" y="0"/><connectionPointOut formalParameter=""/></leftPowerRail>
        <contact localId="10"><position x="40" y="20"/><connectionPointIn><connection refLocalId="1"/></connectionPointIn><connectionPointOut/><variable>{contact}</variable></contact>
        <coil localId="11"><position x="200" y="20"/><connectionPointIn><connection refLocalId="10"/></connectionPointIn><connectionPointOut/><variable>running</variable></coil>
        <inVariable localId="20"><position x="40" y="100"/><connectionPointOut/><expression>level</expression></inVariable>
        <block localId="21" typeName="MUL"><position x="120" y="100"/>
          <inputVariables>
            <variable formalParameter="IN1"><connectionPointIn><connection refLocalId="20"/></connectionPointIn></variable>
            <variable formalParameter="IN2"><connectionPointIn><expression>10</expression></connectionPointIn></variable>
          </inputVariables>
          <inOutVariables/>
          <outputVariables><variable formalParameter="OUT"><connectionPointOut/></variable></outputVariables>
        </block>
        <outVariable localId="22"><position x="200" y="100"/><connectionPointIn><connection refLocalId="21"/></connectionPointIn><expression>speed</expression></outVariable>
        <rightPowerRail localId="2"><position x="400" y="0"/><connectionPointIn><connection refLocalId="11"/></connectionPointIn></rightPowerRail>
      </LD></body>
    </pou>
  </pous></types>
  <instances><configurations/></instances>
</project>
"#;

/// The tank in Function Block Diagram (ADR-013 §2.16): `speed := level * 10` through a MUL
/// block, and `running := level_ok AND NOT halt`, the NOT a negated input. `{level}` is the
/// variable the first input variable reads.
const BLENDER: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<project xmlns="http://www.plcopen.org/xml/tc6_0201">
  <types><dataTypes/><pous>
    <pou name="Blender" pouType="program">
      <interface>
        <inputVars>
          <variable name="level"><type><INT/></type></variable>
          <variable name="level_ok"><type><BOOL/></type></variable>
        </inputVars>
        <outputVars>
          <variable name="speed"><type><INT/></type></variable>
          <variable name="running"><type><BOOL/></type></variable>
        </outputVars>
        <localVars>
          <variable name="halt"><type><BOOL/></type></variable>
        </localVars>
      </interface>
      <body><FBD>
        <inVariable localId="20"><position x="0" y="0"/><connectionPointOut/><expression>{level}</expression></inVariable>
        <block localId="21" typeName="MUL"><position x="120" y="0"/>
          <inputVariables>
            <variable formalParameter="IN1"><connectionPointIn><connection refLocalId="20"/></connectionPointIn></variable>
            <variable formalParameter="IN2"><connectionPointIn><expression>10</expression></connectionPointIn></variable>
          </inputVariables>
          <inOutVariables/>
          <outputVariables><variable formalParameter="OUT"><connectionPointOut/></variable></outputVariables>
        </block>
        <outVariable localId="22"><position x="240" y="0"/><connectionPointIn><connection refLocalId="21"/></connectionPointIn><expression>speed</expression></outVariable>
        <inVariable localId="30"><position x="0" y="100"/><connectionPointOut/><expression>level_ok</expression></inVariable>
        <inVariable localId="31"><position x="0" y="140"/><connectionPointOut/><expression>halt</expression></inVariable>
        <block localId="32" typeName="AND"><position x="120" y="100"/>
          <inputVariables>
            <variable formalParameter="IN1"><connectionPointIn><connection refLocalId="30"/></connectionPointIn></variable>
            <variable formalParameter="IN2" negated="true"><connectionPointIn><connection refLocalId="31"/></connectionPointIn></variable>
          </inputVariables>
          <inOutVariables/>
          <outputVariables><variable formalParameter="OUT"><connectionPointOut/></variable></outputVariables>
        </block>
        <outVariable localId="33"><position x="240" y="100"/><connectionPointIn><connection refLocalId="32"/></connectionPointIn><expression>running</expression></outVariable>
      </FBD></body>
    </pou>
  </pous></types>
  <instances><configurations/></instances>
</project>
"#;

/// The tank as a Sequential Function Chart (ADR-013 §2.17): Wait, the initial step, until
/// `level_ok AND level > 5`; then Drive, whose action `Pump` (statements) commands
/// `speed := level * 10` and whose N action `running` is a BOOL variable; back to Wait by a
/// jump when the level is not good. `{level}` is what the first condition reads.
const FILLER: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<project xmlns="http://www.plcopen.org/xml/tc6_0201" xmlns:xhtml="http://www.w3.org/1999/xhtml">
  <types><dataTypes/><pous>
    <pou name="Filler" pouType="program">
      <interface>
        <inputVars>
          <variable name="level"><type><INT/></type></variable>
          <variable name="level_ok"><type><BOOL/></type></variable>
        </inputVars>
        <outputVars>
          <variable name="speed"><type><INT/></type></variable>
          <variable name="running"><type><BOOL/></type></variable>
        </outputVars>
      </interface>
      <actions>
        <action name="Pump"><body><ST><xhtml:p>speed := level * 10;</xhtml:p></ST></body></action>
      </actions>
      <body><SFC>
        <step localId="1" name="Wait" initialStep="true"><position x="100" y="0"/></step>
        <transition localId="2"><position x="100" y="40"/><connectionPointIn><connection refLocalId="1"/></connectionPointIn>
          <condition><inline name=""><ST><xhtml:p>level_ok AND {level} &gt; 5</xhtml:p></ST></inline></condition></transition>
        <step localId="3" name="Drive"><position x="100" y="80"/><connectionPointIn><connection refLocalId="2"/></connectionPointIn></step>
        <transition localId="4"><position x="100" y="120"/><connectionPointIn><connection refLocalId="3"/></connectionPointIn>
          <condition><inline name=""><ST><xhtml:p>NOT level_ok</xhtml:p></ST></inline></condition></transition>
        <jumpStep localId="5" targetName="Wait"><position x="100" y="160"/><connectionPointIn><connection refLocalId="4"/></connectionPointIn></jumpStep>
        <actionBlock localId="6"><position x="200" y="80"/><connectionPointIn><connection refLocalId="3"/></connectionPointIn>
          <action localId="7" qualifier="N"><relPosition x="0" y="0"/><reference name="Pump"/></action>
          <action localId="8" qualifier="N"><relPosition x="0" y="20"/><reference name="running"/></action>
        </actionBlock>
      </SFC></body>
    </pou>
  </pous></types>
  <instances><configurations/></instances>
</project>
"#;

fn filler(level: &str) -> String {
    FILLER.replace("{level}", level)
}

fn blender(level: &str) -> String {
    BLENDER.replace("{level}", level)
}

fn mixer(contact: &str) -> String {
    MIXER.replace("{contact}", contact)
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

/// A library package file (ADR-015 step 1): one source and its manifest, `checksums.json`
/// right, built with the zip writer.
fn package_file() -> Vec<u8> {
    let source =
        "FUNCTION Twice : INT\nVAR_INPUT x : INT; END_VAR\nTwice := x * 2;\nEND_FUNCTION\n";
    let manifest = json!({
        "format": 1, "id": "org.example.base", "name": "Base blocks", "version": "1.0.0",
        "form": "source", "languages": ["st"], "files": ["src/twice.st"], "depends": [],
        "license": "MIT"
    })
    .to_string();
    let checksums = json!({
        "manifest.json": mythos_core::auth::sha256_hex(&manifest),
        "src/twice.st": mythos_core::auth::sha256_hex(source),
    })
    .to_string();
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, text) in [
        ("manifest.json", manifest.as_str()),
        ("src/twice.st", source),
        ("checksums.json", checksums.as_str()),
    ] {
        writer
            .start_file(name, zip::write::FileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut writer, text.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

/// Base64 of the standard alphabet with padding, as the package routes take a file.
fn base64_of(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let at = |i: usize| u32::from(chunk.get(i).copied().unwrap_or(0));
        let n = (at(0) << 16) | (at(1) << 8) | at(2);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[(n >> (18 - 6 * i)) as usize & 63]));
            } else {
                out.push('=');
            }
        }
    }
    out
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

    // A text is checked without being saved: a diagnostic says where what it is about starts
    // and ends (the end exclusive), so an editor can underline `b`.
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
        json!([{"line": 3, "col": 8, "end_line": 3, "end_col": 9, "message": "b is not declared"}]),
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

/// A Ladder Diagram program (ADR-013 L3b-1): created as a TC6 document, its source read and
/// saved as XML, checked with a diagnostic at its element, built, loaded, and it runs the
/// plant; an ST program saved before programs had a language goes on as it was.
#[tokio::test(flavor = "multi_thread")]
async fn a_ladder_diagram_program_is_written_checked_built_loaded_and_runs_the_plant() {
    let rig = rig().await;
    let data = rig.project.join("logic");

    // An ST program as C3 stored it: its entry has no language, its source is Tank.st.
    std::fs::create_dir_all(data.join("programs")).unwrap();
    std::fs::write(
        data.join("programs.json"),
        json!({"programs": [{"name": "Tank", "interval_ms": 20, "runtime": "vm",
                             "target": "pc", "run": true}]})
        .to_string(),
    )
    .unwrap();
    std::fs::write(data.join("programs/Tank.st"), TANK).unwrap();
    let listed = rig.eng("GET", PROGRAMS, None, 200).await;
    assert_eq!(listed["programs"][0]["language"], "st", "{listed}");
    let checked = rig
        .eng("POST", &format!("{PROGRAMS}/Tank/check"), None, 200)
        .await;
    assert_eq!(checked["ok"], true, "{checked}");
    assert!(checked.get("power").is_none(), "{checked}");

    // A new LD program is a TC6 document that compiles, in its own file.
    let mix = format!("{PROGRAMS}/Mixer");
    let created = rig
        .eng(
            "POST",
            PROGRAMS,
            Some(json!({"name": "Mixer", "language": "ld"})),
            201,
        )
        .await;
    assert_eq!(created["program"]["language"], "ld", "{created}");
    let source = created["source"].as_str().unwrap();
    assert!(
        source.contains(r#"<project xmlns="http://www.plcopen.org/xml/tc6_0201">"#),
        "{source}"
    );
    assert!(data.join("programs/Mixer.xml").is_file());
    assert!(!data.join("programs/Mixer.st").exists());
    assert_eq!(rig.eng("GET", &mix, None, 200).await["source"], source);
    let checked = rig.eng("POST", &format!("{mix}/check"), None, 200).await;
    assert_eq!(checked["ok"], true, "{checked}");
    assert_eq!(checked["power"], json!([]), "{checked}");
    let refused = rig
        .eng(
            "POST",
            PROGRAMS,
            Some(json!({"name": "Pump", "language": "il"})),
            400,
        )
        .await;
    assert!(
        refused["error"]
            .as_str()
            .unwrap()
            .contains("'il' is not a language"),
        "{refused}"
    );

    // A diagnostic of an unsaved document names the element and where it is drawn.
    let checked = rig
        .eng(
            "POST",
            &format!("{mix}/check"),
            Some(json!({"source": mixer("levl_ok")})),
            200,
        )
        .await;
    assert_eq!(checked["ok"], false, "{checked}");
    assert_eq!(
        checked["diagnostics"][0]["message"],
        "levl_ok is not declared"
    );
    assert_eq!(
        checked["diagnostics"][0]["element"],
        json!({"pou": "Mixer", "local_id": 10, "x": 40.0, "y": 20.0,
               "text": {"field": "variable", "col": 1, "end_col": 8}}),
        "{checked}"
    );

    // Saved as XML, bound, built and loaded: it runs the plant (level 20).
    rig.eng(
        "PUT",
        &format!("{mix}/source"),
        Some(json!({"source": mixer("level_ok")})),
        200,
    )
    .await;
    assert_eq!(
        std::fs::read_to_string(data.join("programs/Mixer.xml")).unwrap(),
        mixer("level_ok")
    );
    // The tank's bindings but its setpoint, which the mixer does not have.
    let mut bound = settings();
    bound["published"] = json!([]);
    rig.eng("PUT", &format!("{mix}/settings"), Some(bound), 200)
        .await;
    assert_eq!(
        rig.eng("GET", &mix, None, 200).await["program"]["language"],
        "ld",
        "settings do not change the language"
    );
    let checked = rig.eng("POST", &format!("{mix}/check"), None, 200).await;
    assert_eq!(checked["binding_problems"], json!([]), "{checked}");
    assert_eq!(
        checked["power"],
        json!([
            {"local_id": 1, "output": "", "on": true},
            {"local_id": 10, "output": "", "variable": "__LD_10", "negated": false},
            {"local_id": 10, "network": [10, 11], "variable": "__NET_10", "negated": false},
            {"local_id": 11, "output": "", "variable": "__LD_11", "negated": false},
            {"local_id": 20, "network": [20, 21, 22], "variable": "__NET_20", "negated": false},
        ]),
        "each contact's and coil's power, and each rung's flag; the MUL rung carries values"
    );
    rig.eng(
        "PUT",
        "/api/v1/projects/plant/io",
        Some(rig.io_config()),
        200,
    )
    .await;
    let built = rig.eng("POST", &format!("{mix}/build"), None, 200).await;
    assert_eq!(built["warnings"], json!([]), "{built}");
    assert_eq!(built["instance"]["program"]["name"], "Mixer");
    rig.eng(
        "PUT",
        "/api/v1/projects/plant/runtimes",
        Some(json!({"instances": [built["instance"]]})),
        200,
    )
    .await;
    rig.speed_is("the ladder commands 200", 200).await;
    let watched = rig
        .eng(
            "POST",
            "/api/v1/runtimes/Mixer/watch",
            Some(json!({"variables": ["running", "level_ok"]})),
            200,
        )
        .await;
    assert_eq!(watched["values"][0]["value"], true, "{watched}");
    assert_eq!(watched["values"][1]["value"], true, "{watched}");
    // The power the check named, read from the running module process as any variable is.
    let power = rig
        .eng(
            "POST",
            "/api/v1/runtimes/Mixer/watch",
            Some(json!({"variables": ["__NET_10", "__LD_10", "__LD_11", "__NET_20"]})),
            200,
        )
        .await;
    for k in 0..4 {
        assert_eq!(power["values"][k]["type"], "BOOL", "{power}");
        assert_eq!(power["values"][k]["value"], true, "{power}");
    }

    // The ST program is untouched by all of it, and is still written back as ST.
    let checked = rig
        .eng("POST", &format!("{PROGRAMS}/Tank/check"), None, 200)
        .await;
    assert_eq!(checked["ok"], true, "{checked}");
    let list: Value =
        serde_json::from_str(&std::fs::read_to_string(data.join("programs.json")).unwrap())
            .unwrap();
    assert_eq!(list["programs"][0]["language"], "st", "{list}");
    assert_eq!(list["programs"][1]["language"], "ld", "{list}");

    // Removing it removes its document.
    rig.eng("DELETE", &mix, None, 200).await;
    assert!(!data.join("programs/Mixer.xml").exists());
    assert!(data.join("programs/Tank.st").is_file());

    rig.close().await;
}

/// A Function Block Diagram program (ADR-013 L3c-1): created as a TC6 document, checked with a
/// diagnostic at its element and with where each value is kept, built, loaded, and it runs the
/// plant; its kept values are read from the running module process, a number among them; an
/// ST program beside it is unaffected.
#[tokio::test(flavor = "multi_thread")]
async fn a_function_block_diagram_program_is_written_checked_built_loaded_and_runs_the_plant() {
    let rig = rig().await;
    let data = rig.project.join("logic");
    rig.eng("POST", PROGRAMS, Some(json!({"name": "Tank"})), 201)
        .await;
    rig.eng(
        "PUT",
        &format!("{PROGRAMS}/Tank/source"),
        Some(json!({"source": TANK})),
        200,
    )
    .await;

    let blend = format!("{PROGRAMS}/Blender");
    let created = rig
        .eng(
            "POST",
            PROGRAMS,
            Some(json!({"name": "Blender", "language": "fbd"})),
            201,
        )
        .await;
    assert_eq!(created["program"]["language"], "fbd", "{created}");
    let source = created["source"].as_str().unwrap();
    assert!(source.contains("<FBD/>"), "{source}");
    assert!(data.join("programs/Blender.xml").is_file());
    let checked = rig.eng("POST", &format!("{blend}/check"), None, 200).await;
    assert_eq!(checked["ok"], true, "{checked}");
    assert_eq!(checked["values"], json!([]), "{checked}");
    assert!(checked.get("power").is_none(), "{checked}");

    // A diagnostic of an unsaved document names the element and where it is drawn.
    let checked = rig
        .eng(
            "POST",
            &format!("{blend}/check"),
            Some(json!({"source": blender("levl")})),
            200,
        )
        .await;
    assert_eq!(checked["ok"], false, "{checked}");
    assert_eq!(checked["diagnostics"][0]["message"], "levl is not declared");
    assert_eq!(
        checked["diagnostics"][0]["element"],
        json!({"pou": "Blender", "local_id": 20, "x": 0.0, "y": 0.0,
               "text": {"field": "expression", "col": 1, "end_col": 5}}),
        "{checked}"
    );

    rig.eng(
        "PUT",
        &format!("{blend}/source"),
        Some(json!({"source": blender("level")})),
        200,
    )
    .await;
    let mut bound = settings();
    bound["published"] = json!([]);
    rig.eng("PUT", &format!("{blend}/settings"), Some(bound), 200)
        .await;
    let checked = rig.eng("POST", &format!("{blend}/check"), None, 200).await;
    assert_eq!(checked["binding_problems"], json!([]), "{checked}");
    assert_eq!(
        checked["values"],
        json!([
            {"local_id": 20, "output": "", "variable": "__FBD_20", "negated": false},
            {"local_id": 20, "network": [20, 21, 22], "variable": "__NET_20", "negated": false},
            {"local_id": 21, "output": "OUT", "variable": "__FBD_21", "negated": false},
            {"local_id": 30, "output": "", "variable": "__FBD_30", "negated": false},
            {"local_id": 30, "network": [30, 32, 33, 31], "variable": "__NET_30", "negated": false},
            {"local_id": 31, "output": "", "variable": "__FBD_31", "negated": false},
            {"local_id": 32, "output": "OUT", "variable": "__FBD_32", "negated": false},
        ]),
        "every output's value and each network's flag"
    );
    rig.eng(
        "PUT",
        "/api/v1/projects/plant/io",
        Some(rig.io_config()),
        200,
    )
    .await;
    let built = rig.eng("POST", &format!("{blend}/build"), None, 200).await;
    assert_eq!(built["warnings"], json!([]), "{built}");
    rig.eng(
        "PUT",
        "/api/v1/projects/plant/runtimes",
        Some(json!({"instances": [built["instance"]]})),
        200,
    )
    .await;
    rig.speed_is("the diagram commands 200", 200).await;
    // What the check named, read from the running module process as any variable is: the
    // MUL's result a number, in its own type.
    let kept = rig
        .eng(
            "POST",
            "/api/v1/runtimes/Blender/watch",
            Some(json!({"variables": ["__FBD_21", "__FBD_32", "__NET_20", "running"]})),
            200,
        )
        .await;
    assert_eq!(kept["values"][0]["type"], "INT", "{kept}");
    assert_eq!(kept["values"][0]["value"], 200, "{kept}");
    assert_eq!(kept["values"][1]["value"], true, "{kept}");
    assert_eq!(kept["values"][2]["value"], true, "{kept}");
    assert_eq!(kept["values"][3]["value"], true, "{kept}");

    // The ST program is untouched.
    let checked = rig
        .eng("POST", &format!("{PROGRAMS}/Tank/check"), None, 200)
        .await;
    assert_eq!(checked["ok"], true, "{checked}");
    assert!(checked.get("values").is_none(), "{checked}");
    let list: Value =
        serde_json::from_str(&std::fs::read_to_string(data.join("programs.json")).unwrap())
            .unwrap();
    assert_eq!(list["programs"][0]["language"], "st", "{list}");
    assert_eq!(list["programs"][1]["language"], "fbd", "{list}");
    rig.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sequential_function_chart_program_is_written_checked_built_loaded_and_runs_the_plant() {
    let rig = rig().await;
    let data = rig.project.join("logic");
    let fill = format!("{PROGRAMS}/Filler");
    let created = rig
        .eng(
            "POST",
            PROGRAMS,
            Some(json!({"name": "Filler", "language": "sfc"})),
            201,
        )
        .await;
    assert_eq!(created["program"]["language"], "sfc", "{created}");
    let source = created["source"].as_str().unwrap();
    assert!(
        source.contains(r#"name="Start" initialStep="true""#),
        "{source}"
    );
    assert!(data.join("programs/Filler.xml").is_file());
    let checked = rig.eng("POST", &format!("{fill}/check"), None, 200).await;
    assert_eq!(checked["ok"], true, "{checked}");
    assert_eq!(
        checked["chart"],
        json!([
            {"local_id": 1, "step": "X", "variable": "Start.X", "negated": false},
            {"local_id": 1, "step": "T", "variable": "Start.T", "negated": false},
        ]),
        "{checked}"
    );

    // A diagnostic of an unsaved chart names the transition and the characters of its
    // condition.
    let checked = rig
        .eng(
            "POST",
            &format!("{fill}/check"),
            Some(json!({"source": filler("levl")})),
            200,
        )
        .await;
    assert_eq!(checked["ok"], false, "{checked}");
    assert_eq!(checked["diagnostics"][0]["message"], "levl is not declared");
    assert_eq!(
        checked["diagnostics"][0]["element"],
        json!({"pou": "Filler", "local_id": 2, "x": 100.0, "y": 40.0,
               "text": {"field": "condition", "col": 14, "end_col": 18, "line": 1}}),
        "{checked}"
    );

    rig.eng(
        "PUT",
        &format!("{fill}/source"),
        Some(json!({"source": filler("level")})),
        200,
    )
    .await;
    let mut bound = settings();
    bound["published"] = json!([]);
    rig.eng("PUT", &format!("{fill}/settings"), Some(bound), 200)
        .await;
    let checked = rig.eng("POST", &format!("{fill}/check"), None, 200).await;
    assert_eq!(checked["binding_problems"], json!([]), "{checked}");
    let chart = checked["chart"].as_array().unwrap();
    for want in [
        json!({"local_id": 3, "step": "X", "variable": "Drive.X", "negated": false}),
        json!({"local_id": 2, "transition": true, "variable": "__SFC_2", "negated": false, "ran": "__SFC_2_EN"}),
        json!({"local_id": 7, "action": "Q", "variable": "Pump.Q", "negated": false}),
        json!({"local_id": 8, "action": "Q", "variable": "running", "negated": false}),
    ] {
        assert!(chart.contains(&want), "{want} in {checked}");
    }
    rig.eng(
        "PUT",
        "/api/v1/projects/plant/io",
        Some(rig.io_config()),
        200,
    )
    .await;
    let built = rig.eng("POST", &format!("{fill}/build"), None, 200).await;
    assert_eq!(built["warnings"], json!([]), "{built}");
    rig.eng(
        "PUT",
        "/api/v1/projects/plant/runtimes",
        Some(json!({"instances": [built["instance"]]})),
        200,
    )
    .await;
    rig.speed_is("the chart's Drive commands 200", 200).await;
    // What the check named, read from the running module process as any variable is.
    let kept = rig
        .eng(
            "POST",
            "/api/v1/runtimes/Filler/watch",
            Some(json!({"variables": ["Drive.X", "Wait.X", "__SFC_2", "__SFC_4_EN", "Pump.Q", "running", "Drive.T"]})),
            200,
        )
        .await;
    let v = |k: usize| kept["values"][k]["value"].clone();
    assert_eq!(v(0), true, "{kept}");
    assert_eq!(v(1), false, "{kept}");
    assert_eq!(v(2), true, "the condition that cleared, kept: {kept}");
    assert_eq!(v(3), true, "Drive's transition is enabled: {kept}");
    assert_eq!(v(4), true, "{kept}");
    assert_eq!(v(5), true, "{kept}");
    assert_eq!(kept["values"][6]["type"], "TIME", "{kept}");
    let list: Value =
        serde_json::from_str(&std::fs::read_to_string(data.join("programs.json")).unwrap())
            .unwrap();
    assert_eq!(list["programs"][0]["language"], "sfc", "{list}");
    rig.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_source_made_to_exhaust_the_stack_is_refused_and_the_module_answers_on() {
    // The compiler runs on a thread of the module's runtime. Nested thousands deep, or with a
    // structure that holds itself, a source once overflowed that thread's stack and killed
    // the module: here it is told, and the same process answers the next request.
    let rig = rig().await;
    let deep = format!("{PROGRAMS}/Deep");
    rig.eng("POST", PROGRAMS, Some(json!({"name": "Deep"})), 201)
        .await;
    let nested = |body: String| {
        format!("PROGRAM Deep\nVAR x : INT; b : BOOL; END_VAR\n{body}\nEND_PROGRAM\n")
    };
    let sources = [
        nested(format!("x := {}x{};", "(".repeat(5_000), ")".repeat(5_000))),
        nested(format!("x := x{};", " + x".repeat(5_000))),
        nested(format!("{} x := 1; {}", "IF b THEN ".repeat(5_000), "END_IF; ".repeat(5_000))),
        "TYPE L : STRUCT a : INT; r : L; END_STRUCT; END_TYPE\nPROGRAM Deep\nVAR x : L; END_VAR\nx.a := 1;\nEND_PROGRAM\n".to_string(),
    ];
    for source in sources {
        let checked = rig
            .eng(
                "POST",
                &format!("{deep}/check"),
                Some(json!({"source": source})),
                200,
            )
            .await;
        assert_eq!(checked["ok"], false, "{checked}");
        let told = checked["diagnostics"][0]["message"]
            .as_str()
            .unwrap_or_default();
        assert!(
            told.contains("nested more than") || told.contains("contains itself"),
            "{checked}"
        );
        rig.eng(
            "PUT",
            &format!("{deep}/source"),
            Some(json!({"source": source})),
            200,
        )
        .await;
        let refused = rig.eng("POST", &format!("{deep}/build"), None, 400).await;
        assert_eq!(refused["diagnostics"][0]["message"], told, "{refused}");
    }
    rig.eng(
        "PUT",
        &format!("{deep}/source"),
        Some(json!({"source": TANK.replace("Tank", "Deep")})),
        200,
    )
    .await;
    let checked = rig.eng("POST", &format!("{deep}/check"), None, 200).await;
    assert_eq!(checked["ok"], true, "the module still answers: {checked}");
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

    // Library packages: reading and checking them is a viewer's, adding and removing an
    // engineer's.
    let packages = "/api/v1/projects/plant/logic/packages";
    let base = format!("{packages}/org.example.base");
    let file = json!({"file": base64_of(&package_file())});
    for who in [&rig.viewer, &rig.operator] {
        let (status, _) = rig.send(who, "POST", packages, Some(file.clone())).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    let (status, checked) = rig
        .send(
            &rig.viewer,
            "POST",
            &format!("{packages}/check"),
            Some(file.clone()),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(checked["ok"], true, "{checked}");
    assert!(!data.join("packages.json").exists());
    rig.eng("POST", packages, Some(file), 201).await;
    for who in [&rig.viewer, &rig.operator] {
        let (status, _) = rig.send(who, "DELETE", &base, None).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    let (status, listed) = rig.send(&rig.viewer, "GET", packages, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["packages"][0]["id"], "org.example.base", "{listed}");
    let (status, got) = rig.send(&rig.viewer, "GET", &base, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got["sources"][0]["path"], "src/twice.st", "{got}");
    assert!(data.join("packages/org.example.base/1.0.0.zip").is_file());
    rig.eng("DELETE", &base, None, 200).await;
    assert!(!data.join("packages/org.example.base/1.0.0.zip").exists());

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
            "list_logic_packages",
            "get_logic_package",
            "check_logic_package",
            "add_logic_package",
            "remove_logic_package",
            "list_logic_library",
            "get_logic_library_source",
            "save_logic_library_source",
            "delete_logic_library_source",
            "check_logic_library",
            "list_logic_runtimes",
            "get_logic_runtime",
            "save_logic_runtime",
            "delete_logic_runtime",
            "check_logic_runtime",
            "build_logic_runtime",
        ]
    );

    // Each tool that takes or gives a source says which languages.
    let description = |name: &str| {
        listed["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == name)
            .unwrap()["description"]
            .as_str()
            .unwrap()
            .to_string()
    };
    for name in [
        "list_logic_programs",
        "create_logic_program",
        "get_logic_program",
        "save_logic_source",
        "check_logic_program",
        "build_logic_program",
    ] {
        let said = description(name);
        assert!(
            said.contains("Structured Text")
                && said.contains("Ladder Diagram")
                && said.contains("Function Block Diagram")
                && said.contains("Sequential Function Chart"),
            "{name}: {said}"
        );
    }
    // The check says what a chart keeps for monitoring.
    assert!(description("check_logic_program").contains("'chart'"));

    // A chart, written and checked over MCP alone.
    let (error, created) = tool(
        "create_logic_program",
        json!({"project_id": "plant", "body": {"name": "Filler", "language": "sfc"}}),
    )
    .await;
    assert!(!error, "{created}");
    assert_eq!(created["program"]["language"], "sfc", "{created}");
    let (error, saved) = tool(
        "save_logic_source",
        json!({"project_id": "plant", "program": "Filler", "body": {"source": filler("level")}}),
    )
    .await;
    assert!(!error, "{saved}");
    let (error, checked) = tool(
        "check_logic_program",
        json!({"project_id": "plant", "program": "Filler", "body": {}}),
    )
    .await;
    assert!(!error, "{checked}");
    assert_eq!(checked["ok"], true, "{checked}");
    assert!(
        checked["chart"].as_array().unwrap().contains(
            &json!({"local_id": 3, "step": "X", "variable": "Drive.X", "negated": false})
        ),
        "{checked}"
    );

    let (error, created) = tool(
        "create_logic_program",
        json!({"project_id": "plant", "body": {"name": "Tank", "interval_ms": 50}}),
    )
    .await;
    assert!(!error, "{created}");
    assert_eq!(created["program"]["language"], "st", "{created}");
    let (error, created) = tool(
        "create_logic_program",
        json!({"project_id": "plant", "body": {"name": "Mixer", "language": "ld"}}),
    )
    .await;
    assert!(!error, "{created}");
    let (error, saved) = tool(
        "save_logic_source",
        json!({"project_id": "plant", "program": "Mixer", "body": {"source": mixer("level_ok")}}),
    )
    .await;
    assert!(!error, "{saved}");
    let (error, checked) = tool(
        "check_logic_program",
        json!({"project_id": "plant", "program": "Mixer", "body": {}}),
    )
    .await;
    assert!(!error, "{checked}");
    assert_eq!(checked["ok"], true, "{checked}");
    assert_eq!(checked["program"], "Mixer", "{checked}");
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

/// A project of today (no library, no data) answers as it did before D3: the check has no
/// `data` or `project_problems`, the built instance and its program no `data`, and the project
/// is given no data directory by the logic module.
#[tokio::test(flavor = "multi_thread")]
async fn a_project_of_today_answers_without_any_data_key() {
    let rig = rig().await;
    let tank = format!("{PROGRAMS}/Tank");
    rig.eng("POST", PROGRAMS, Some(json!({"name": "Tank"})), 201)
        .await;
    rig.eng(
        "PUT",
        &format!("{tank}/source"),
        Some(json!({"source": TANK})),
        200,
    )
    .await;
    rig.eng("PUT", &format!("{tank}/settings"), Some(settings()), 200)
        .await;
    let checked = rig.eng("POST", &format!("{tank}/check"), None, 200).await;
    assert_eq!(checked["ok"], true, "{checked}");
    assert!(checked.get("data").is_none(), "{checked}");
    assert!(checked.get("project_problems").is_none(), "{checked}");
    let built = rig.eng("POST", &format!("{tank}/build"), None, 200).await;
    assert!(built.get("project_problems").is_none(), "{built}");
    let instance = built["instance"].as_object().unwrap();
    let keys: Vec<&str> = instance.keys().map(String::as_str).collect();
    assert_eq!(keys, ["id", "module", "program", "run"], "{built}");
    assert!(
        built["instance"]["program"].get("data").is_none(),
        "{built}"
    );
    assert!(
        !rig.project.join("data").exists(),
        "the logic module made a data directory"
    );
    rig.close().await;
}

/// A project of today through the server, with no task configuration: the program's check and
/// build answer today's keys, the runtime route of its implied runtime answers the program's
/// build itself, and no call writes `logic/tasks.json`.
#[tokio::test(flavor = "multi_thread")]
async fn a_project_of_today_builds_with_exactly_todays_keys_and_writes_no_task_configuration() {
    let rig = rig().await;
    let tank = format!("{PROGRAMS}/Tank");
    rig.eng("POST", PROGRAMS, Some(json!({"name": "Tank"})), 201)
        .await;
    rig.eng(
        "PUT",
        &format!("{tank}/source"),
        Some(json!({"source": TANK})),
        200,
    )
    .await;
    rig.eng("PUT", &format!("{tank}/settings"), Some(settings()), 200)
        .await;
    let file = rig.project.join("logic").join("tasks.json");
    let checked = rig.eng("POST", &format!("{tank}/check"), None, 200).await;
    let mut keys: Vec<&str> = checked
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "binding_problems",
            "code_size",
            "diagnostics",
            "memory_size",
            "ok",
            "program",
            "variables",
            "version"
        ],
        "{checked}"
    );
    let built = rig.eng("POST", &format!("{tank}/build"), None, 200).await;
    let mut keys: Vec<&str> = built
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "code_size",
            "instance",
            "memory_size",
            "version",
            "warnings"
        ],
        "{built}"
    );
    let instance: Vec<&str> = built["instance"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(instance, ["id", "module", "program", "run"], "{built}");
    assert_eq!(built["version"], checked["version"]);
    let runtime = rig
        .eng("POST", &format!("{RUNTIMES}/Tank/build"), None, 200)
        .await;
    assert_eq!(runtime, built);
    let runtime_check = rig
        .eng("POST", &format!("{RUNTIMES}/Tank/check"), None, 200)
        .await;
    assert_eq!(
        runtime_check["version"], checked["version"],
        "{runtime_check}"
    );
    rig.eng("GET", RUNTIMES, None, 200).await;
    assert!(!file.exists(), "a read wrote logic/tasks.json");
    rig.close().await;
}

/// The library through the server: an engineer writes a source, a viewer reads and checks it,
/// an operator may not write it, and a name that is not one, or is a device of Windows, is
/// refused before anything is looked up.
#[tokio::test(flavor = "multi_thread")]
async fn the_library_is_written_read_checked_and_deleted_through_the_server() {
    let rig = rig().await;
    let library = "/api/v1/projects/plant/logic/library";
    let motors =
        "FUNCTION Twice : INT\nVAR_INPUT x : INT; END_VAR\n  Twice := x * 2;\nEND_FUNCTION\n";
    let (status, _) = rig
        .send(
            &rig.operator,
            "PUT",
            &format!("{library}/Motors"),
            Some(json!({"source": motors})),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let saved = rig
        .eng(
            "PUT",
            &format!("{library}/Motors"),
            Some(json!({"source": motors})),
            200,
        )
        .await;
    assert_eq!(saved, json!({"name": "Motors", "created": true}));
    assert_eq!(
        std::fs::read_to_string(rig.project.join("logic").join("library").join("Motors.st"))
            .unwrap(),
        motors
    );
    let (status, listed) = rig.send(&rig.viewer, "GET", library, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed, json!({"sources": [{"name": "Motors"}]}));
    let (status, checked) = rig
        .send(&rig.viewer, "POST", &format!("{library}/check"), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    // A library that checks answers its blocks with their pins, for the editors' palettes.
    assert_eq!(
        checked,
        json!({"ok": true, "diagnostics": [], "blocks": [{
            "name": "Twice",
            "kind": "function",
            "inputs": [{"name": "x", "type": "INT"}],
            "outputs": [],
            "result": "INT"
        }]})
    );

    // Every program is compiled with it.
    let tank = format!("{PROGRAMS}/Tank");
    rig.eng("POST", PROGRAMS, Some(json!({"name": "Tank"})), 201)
        .await;
    let twice = TANK.replace("(setpoint - level) * 10", "Twice(setpoint - level) * 5");
    rig.eng(
        "PUT",
        &format!("{tank}/source"),
        Some(json!({"source": twice})),
        200,
    )
    .await;
    let checked = rig.eng("POST", &format!("{tank}/check"), None, 200).await;
    assert_eq!(checked["ok"], true, "{checked}");

    for (name, said) in [
        ("CoN", "'CoN' cannot be a library source name: it is a device name on Windows (CON, PRN, AUX, NUL, COM0 to COM9, LPT0 to LPT9)"),
        ("lpt1", "'lpt1' cannot be a library source name: it is a device name on Windows (CON, PRN, AUX, NUL, COM0 to COM9, LPT0 to LPT9)"),
        ("a.b", "'a.b' is not a library source name: a letter, then letters, digits or _, at most 64"),
    ] {
        let refused = rig
            .eng(
                "PUT",
                &format!("{library}/{name}"),
                Some(json!({"source": motors})),
                400,
            )
            .await;
        assert_eq!(refused["error"], said);
    }
    rig.eng("GET", &format!("{library}/..%2Fprograms.json"), None, 400)
        .await;
    rig.eng("GET", &format!("{library}/Pumps"), None, 404).await;
    let deleted = rig
        .eng("DELETE", &format!("{library}/MOTORS"), None, 200)
        .await;
    assert_eq!(deleted, json!({"deleted": "Motors"}));
    // Without it the program no longer compiles.
    let checked = rig.eng("POST", &format!("{tank}/check"), None, 200).await;
    assert_eq!(checked["ok"], false, "{checked}");
    let listed: Vec<_> = std::fs::read_dir(rig.project.join("logic").join("library"))
        .unwrap()
        .collect();
    assert!(listed.is_empty(), "{listed:?}");
    rig.close().await;
}

const RUNTIMES: &str = "/api/v1/projects/plant/logic/runtimes";

/// A runtime of two tasks over the server: Writer fast, Reader slow.
fn line() -> Value {
    json!({ "module": "vm", "target": "pc", "run": true, "tasks": [
        { "name": "fast", "kind": { "type": "cyclic", "interval_ms": 20 }, "priority": 1, "programs": ["Writer"] },
        { "name": "slow", "kind": { "type": "cyclic", "interval_ms": 50 }, "priority": 5, "programs": ["reader"] }
    ]})
}

/// The task configuration through the server: a viewer reads it, an operator and a viewer may
/// not write it, an engineer saves and deletes a runtime, refusals carry the module's status
/// and words, and the file is the module's own `logic/tasks.json`, in this project alone.
#[tokio::test(flavor = "multi_thread")]
async fn runtimes_are_configured_listed_and_deleted_through_the_server() {
    let rig = rig().await;
    for name in ["Writer", "Reader"] {
        rig.eng("POST", PROGRAMS, Some(json!({"name": name})), 201)
            .await;
    }
    let file = rig.project.join("logic").join("tasks.json");
    let (status, listed) = rig.send(&rig.viewer, "GET", RUNTIMES, None).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed["runtimes"], json!([]));
    assert_eq!(listed["problems"], json!([]));
    let ids: Vec<&str> = listed["implied"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["Writer", "Reader"]);
    assert!(!file.exists(), "a read made tasks.json");

    let uri = format!("{RUNTIMES}/Line");
    for session in [&rig.viewer, &rig.operator] {
        let (status, _) = rig.send(session, "PUT", &uri, Some(line())).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, _) = rig.send(session, "DELETE", &uri, None).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    assert!(!file.exists());
    let saved = rig.eng("PUT", &uri, Some(line()), 200).await;
    assert_eq!(saved["created"], true, "{saved}");
    assert_eq!(saved["runtime"]["tasks"][1]["programs"], json!(["Reader"]));
    let stored: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(stored["runtimes"][0], saved["runtime"]);
    let (status, one) = rig
        .send(&rig.viewer, "GET", &format!("{RUNTIMES}/line"), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(one, json!({"runtime": saved["runtime"], "implied": false}));

    // The module's refusals, with its status and words.
    for (id, status, said) in [
        ("CON", 400, "'CON' cannot be a runtime id: it is a device name on Windows (CON, PRN, AUX, NUL, COM0 to COM9, LPT0 to LPT9)"),
        ("a.b", 400, "'a.b' is not a runtime id: use 1 to 64 letters, digits, '-' or '_'"),
        ("writer", 409, "'writer' is a program of the project; a runtime is not named like a program"),
        ("LINE", 409, "the project already has a runtime 'Line'; names that differ only in case are one name"),
    ] {
        let refused = rig
            .eng("PUT", &format!("{RUNTIMES}/{id}"), Some(line()), status)
            .await;
        assert_eq!(refused, json!({"error": said}));
    }
    let refused = rig
        .eng("DELETE", &format!("{PROGRAMS}/Writer"), None, 409)
        .await;
    assert_eq!(
        refused["error"],
        "program 'Writer' is in task 'fast' of runtime 'Line': take it out of the runtime first"
    );
    let refused = rig
        .eng("POST", PROGRAMS, Some(json!({"name": "line"})), 409)
        .await;
    assert_eq!(
        refused["error"],
        "the project has a runtime 'Line'; a program is not named like a runtime"
    );

    // Another project has none.
    let other = rig
        .eng("GET", "/api/v1/projects/other/logic/runtimes", None, 200)
        .await;
    assert_eq!(
        other,
        json!({"runtimes": [], "implied": [], "problems": []})
    );

    let deleted = rig.eng("DELETE", &uri, None, 200).await;
    assert_eq!(deleted, json!({"deleted": "Line"}));
    rig.eng("DELETE", &uri, None, 404).await;
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "{\n  \"runtimes\": []\n}\n"
    );
    let listed = rig.eng("GET", RUNTIMES, None, 200).await;
    assert_eq!(listed["implied"].as_array().unwrap().len(), 2, "{listed}");
    rig.close().await;
}

/// An agent configures a runtime over MCP alone; the runtime is named by `runtime_id`, which
/// every runtime tool requires.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_configures_a_runtime_over_mcp() {
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
        async move { call.await["result"].clone() }
    };

    let listed = rpc("tools/list", json!({})).await;
    let schema = |name: &str| {
        listed["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == name)
            .unwrap_or_else(|| panic!("no tool {name}"))["inputSchema"]
            .clone()
    };
    assert_eq!(
        schema("list_logic_runtimes")["required"],
        json!(["project_id"])
    );
    for name in ["get_logic_runtime", "delete_logic_runtime"] {
        assert_eq!(
            schema(name)["required"],
            json!(["project_id", "runtime_id"]),
            "{name}"
        );
    }
    assert_eq!(
        schema("save_logic_runtime")["required"],
        json!(["project_id", "runtime_id", "body"])
    );

    for name in ["Writer", "Reader"] {
        let created = tool(
            "create_logic_program",
            json!({"project_id": "plant", "body": {"name": name}}),
        )
        .await;
        assert_eq!(created["isError"], false, "{created}");
    }
    let without = tool(
        "save_logic_runtime",
        json!({"project_id": "plant", "body": line()}),
    )
    .await;
    assert_eq!(without["isError"], true, "{without}");
    assert_eq!(without["content"][0]["text"], "'runtime_id' is required");
    let saved = tool(
        "save_logic_runtime",
        json!({"project_id": "plant", "runtime_id": "Line", "body": line()}),
    )
    .await;
    assert_eq!(saved["isError"], false, "{saved}");
    assert_eq!(
        saved["structuredContent"]["body"]["created"], true,
        "{saved}"
    );
    let got = tool(
        "get_logic_runtime",
        json!({"project_id": "plant", "runtime_id": "Line"}),
    )
    .await;
    assert_eq!(got["structuredContent"]["body"]["implied"], false, "{got}");
    let all = tool("list_logic_runtimes", json!({"project_id": "plant"})).await;
    assert_eq!(
        all["structuredContent"]["body"]["runtimes"][0]["id"], "Line",
        "{all}"
    );
    let refused = tool(
        "save_logic_runtime",
        json!({"project_id": "plant", "runtime_id": "Reader", "body": line()}),
    )
    .await;
    assert_eq!(refused["isError"], true, "{refused}");
    assert_eq!(refused["structuredContent"]["status"], 409, "{refused}");
    // Checked and built over MCP alone, the runtime named by `runtime_id`.
    for name in ["check_logic_runtime", "build_logic_runtime"] {
        let required = schema(name)["required"].clone();
        assert!(
            required.as_array().unwrap().contains(&json!("runtime_id")),
            "{name}: {required}"
        );
    }
    let checked = tool(
        "check_logic_runtime",
        json!({"project_id": "plant", "runtime_id": "Line", "body": {}}),
    )
    .await;
    assert_eq!(checked["isError"], false, "{checked}");
    assert_eq!(
        checked["structuredContent"]["body"]["ok"], true,
        "{checked}"
    );
    let built = tool(
        "build_logic_runtime",
        json!({"project_id": "plant", "runtime_id": "Line"}),
    )
    .await;
    assert_eq!(built["isError"], false, "{built}");
    let body = &built["structuredContent"]["body"];
    assert_eq!(body["instance"]["id"], "Line", "{built}");
    assert_eq!(body["replaces"], json!(["Writer", "Reader"]), "{built}");
    assert_eq!(
        body["version"],
        checked["structuredContent"]["body"]["version"]
    );
    let alone = tool(
        "build_logic_program",
        json!({"project_id": "plant", "program": "Writer"}),
    )
    .await;
    assert_eq!(alone["isError"], true, "{alone}");
    assert_eq!(alone["structuredContent"]["status"], 409, "{alone}");
    let deleted = tool(
        "delete_logic_runtime",
        json!({"project_id": "plant", "runtime_id": "Line"}),
    )
    .await;
    assert_eq!(
        deleted["structuredContent"]["body"],
        json!({"deleted": "Line"}),
        "{deleted}"
    );
    services.shutdown().await;
}

/// The library packages over MCP alone (ADR-010): checked, added, listed, read and removed, each
/// tool saying that programs are not compiled with a package yet.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_adds_lists_reads_and_removes_a_package_over_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(dir.path()).unwrap());
    storage
        .create_project(&ProjectConfig::new("plant", "Plant"))
        .unwrap();
    let project = storage.project_dir("plant").unwrap();
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
        async move { call.await["result"].clone() }
    };

    let listed = rpc("tools/list", json!({})).await;
    let found = |name: &str| {
        listed["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == name)
            .unwrap_or_else(|| panic!("no tool {name}"))
            .clone()
    };
    for name in [
        "list_logic_packages",
        "get_logic_package",
        "check_logic_package",
        "add_logic_package",
        "remove_logic_package",
    ] {
        let said = found(name)["description"].as_str().unwrap().to_string();
        assert!(
            said.contains("programs are not compiled with it yet"),
            "{name}: {said}"
        );
    }
    for name in ["check_logic_package", "add_logic_package"] {
        let said = found(name)["description"].as_str().unwrap().to_string();
        assert!(
            said.contains("base64") && said.contains("1048576 bytes"),
            "{name}: {said}"
        );
        assert_eq!(
            found(name)["inputSchema"]["required"],
            json!(["project_id", "body"]),
            "{name}"
        );
    }
    for name in ["get_logic_package", "remove_logic_package"] {
        assert_eq!(
            found(name)["inputSchema"]["required"],
            json!(["project_id", "package"]),
            "{name}"
        );
    }

    let file = json!({"file": base64_of(&package_file()), "file_name": "base-1.0.0.zip"});
    let checked = tool(
        "check_logic_package",
        json!({"project_id": "plant", "body": file}),
    )
    .await;
    assert_eq!(checked["isError"], false, "{checked}");
    assert_eq!(
        checked["structuredContent"]["body"]["ok"], true,
        "{checked}"
    );
    let added = tool(
        "add_logic_package",
        json!({"project_id": "plant", "body": file}),
    )
    .await;
    assert_eq!(added["isError"], false, "{added}");
    assert_eq!(
        added["structuredContent"]["body"]["added"]["origin"],
        json!({"kind": "file", "file_name": "base-1.0.0.zip"}),
        "{added}"
    );
    let again = tool(
        "add_logic_package",
        json!({"project_id": "plant", "body": file}),
    )
    .await;
    assert_eq!(again["isError"], true, "{again}");
    assert_eq!(again["structuredContent"]["status"], 409, "{again}");
    let all = tool("list_logic_packages", json!({"project_id": "plant"})).await;
    assert_eq!(
        all["structuredContent"]["body"]["packages"][0]["name"], "Base blocks",
        "{all}"
    );
    let got = tool(
        "get_logic_package",
        json!({"project_id": "plant", "package": "ORG.EXAMPLE.BASE"}),
    )
    .await;
    assert_eq!(got["isError"], false, "{got}");
    assert_eq!(
        got["structuredContent"]["body"]["blocks"][0]["name"], "Twice",
        "{got}"
    );
    assert!(project
        .join("logic/packages/org.example.base/1.0.0.zip")
        .is_file());
    let removed = tool(
        "remove_logic_package",
        json!({"project_id": "plant", "package": "org.example.base"}),
    )
    .await;
    assert_eq!(
        removed["structuredContent"]["body"],
        json!({"removed": {"id": "org.example.base", "version": "1.0.0"}}),
        "{removed}"
    );
    assert!(!project.join("logic/packages/org.example.base").exists());
    services.shutdown().await;
}
