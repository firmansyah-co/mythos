//! A control program running end to end (ADR-013 §2.4): Structured Text compiled here, loaded
//! into the real runtime module process, hosted by the field I/O runtime, reading a tag from
//! one real Modbus device (a simulator on a loopback port) and commanding another.
//!
//! The program is a tank: run the pump while the level is known and below the setpoint, at a
//! speed that follows the distance to it.

use driver_modbus::sim::ModbusSimServer;
use mythos_core::io::{DriverRegistry, IoConfig, IoRuntime, IoStatusHandle, WriteError};
use mythos_core::runtimes::{
    BusTags, RuntimeEnv, RuntimeStates, RuntimeStatus, RuntimesConfig, TagSource,
};
use mythos_core::tag::TagCatalog;
use mythos_logic_compiler::{compile, TargetProfile};
use mythos_pal::DefaultShm;
use mythos_shm::ShmWriterHandle;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

const UNDER_TEST: &str = env!("CARGO_BIN_EXE_runtime-vm-module-under-test");
const STUBBORN: &str = env!("CARGO_BIN_EXE_stubborn-runtime-under-test");
const CAPACITY: usize = 64;
static SEQ: AtomicU32 = AtomicU32::new(0);

const LEVEL: &str = "external.Plant.LT1.level";
const PUMP: &str = "external.Plant.P1.run";
const SPEED: &str = "external.Plant.P1.speed";
const T_PUMP: &str = "internal.Plant.Tank.pump";
const T_SCANS: &str = "internal.Plant.Tank.scans";
const T_SETPOINT: &str = "internal.Plant.Tank.setpoint";
/// What the speed register is written whenever the program is not running. Not 0, so that a
/// test sees the safe value and not a program that happened to compute zero.
const SAFE_SPEED: u16 = 7;

const TANK: &str = "
PROGRAM Tank
VAR_INPUT
  level : INT;
  level_ok : BOOL;
  setpoint : INT := 50;
END_VAR
VAR_OUTPUT
  pump : BOOL;
  speed : INT;
END_VAR
VAR
  scans : DINT;
END_VAR
  scans := scans + 1;
  pump := level_ok AND (level < setpoint);
  IF pump THEN
    speed := (setpoint - level) * 10;
  ELSE
    speed := 0;
  END_IF;
END_PROGRAM
";

/// As TANK, but it divides by the level: a level of 0 is a division by zero. It waits for the
/// level to be known, as a program must: until a tag has been read its variable holds its
/// initial value, here 0.
const DIVIDES: &str = "
PROGRAM Tank
VAR_INPUT level : INT; level_ok : BOOL; setpoint : INT := 50; END_VAR
VAR_OUTPUT pump : BOOL; speed : INT; END_VAR
VAR scans : DINT; END_VAR
  scans := scans + 1;
  IF level_ok THEN
    pump := TRUE;
    speed := 1000 / level;
  END_IF;
END_PROGRAM
";

struct Segment(String);

impl Segment {
    fn new() -> Self {
        let name = format!(
            "mythos_runtime_test_{}_{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        );
        let _ = DefaultShm::unlink(&name);
        Self(name)
    }
}

impl Drop for Segment {
    fn drop(&mut self) {
        let _ = DefaultShm::unlink(&self.0);
    }
}

/// One instance, `tank`, of `source` in the VM runtime module, bound to the plant's tags.
fn runtimes(source: &str, run: bool) -> RuntimesConfig {
    let compiled = compile(source, &TargetProfile::pc()).unwrap_or_else(|d| panic!("{d:?}"));
    serde_json::from_value(json!({"instances": [{
        "id": "tank",
        "module": "vm",
        "run": run,
        "program": {
            "name": compiled.name,
            "version": "test-1",
            "interval_ms": 20,
            "code": hex::encode(&compiled.bytecode),
            "symbols": compiled.symbols,
            "inputs": [
                {"variable": "level", "tag": LEVEL},
                {"variable": "level_ok", "tag": LEVEL, "of": "good"}
            ],
            "outputs": [
                {"variable": "pump", "tag": PUMP, "safe": false},
                {"variable": "speed", "tag": SPEED, "safe": SAFE_SPEED}
            ],
            "published": [
                {"variable": "pump", "tag": T_PUMP},
                {"variable": "scans", "tag": T_SCANS},
                {"variable": "setpoint", "tag": T_SETPOINT, "writable": true}
            ]
        }
    }]}))
    .unwrap()
}

struct Plant {
    /// The device the level is read from.
    sensors: ModbusSimServer,
    /// The device the pump is commanded on: holding 0 is run, holding 1 is speed.
    drive: ModbusSimServer,
    runtime: IoRuntime,
    status: IoStatusHandle,
    states: RuntimeStates,
    tags: Arc<BusTags<DefaultShm>>,
    registry: DriverRegistry,
    _modules: tempfile::TempDir,
    _segment: Segment,
}

fn field(sensors: u16, drive: u16) -> Vec<Value> {
    let modbus = |id: &str, port: u16, mappings: Value| {
        json!({
            "id": id, "driver": "modbus", "poll_interval_ms": 20,
            "config": {"transport": "tcp", "host": "127.0.0.1", "port": port,
                       "timeout_ms": 300, "mappings": mappings}
        })
    };
    vec![
        modbus(
            "sensors",
            sensors,
            json!([{"uri": LEVEL, "address": 0, "function_code": 4, "data_type": "i16"}]),
        ),
        modbus(
            "drive",
            drive,
            json!([
                {"uri": PUMP, "address": 0, "function_code": 3},
                {"uri": SPEED, "address": 1, "function_code": 3}
            ]),
        ),
    ]
}

/// The whole configuration: the field connections and the project's runtimes.
fn config(plant_ports: (u16, u16), runtimes: &RuntimesConfig) -> IoConfig {
    let mut connections: Vec<Value> = field(plant_ports.0, plant_ports.1);
    connections.extend(
        runtimes
            .connections()
            .into_iter()
            .map(|c| serde_json::to_value(c).unwrap()),
    );
    serde_json::from_value(json!({ "connections": connections })).unwrap()
}

async fn plant(source: &str, level: i16, run: bool) -> Plant {
    plant_running(runtimes(source, run), level).await
}

async fn plant_running(runtimes: RuntimesConfig, level: i16) -> Plant {
    let sensors = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    let drive = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sensors.set_input(0, level as u16).await;
    // What the drive holds before anything commands it: neither the safe nor a computed value.
    drive.set_holding(0, 9).await;
    drive.set_holding(1, 999).await;

    let modules = tempfile::tempdir().unwrap();
    for (source, name) in [
        (UNDER_TEST, "mythos-module-runtime-vm"),
        (STUBBORN, "mythos-module-stubborn"),
    ] {
        std::fs::copy(
            source,
            modules
                .path()
                .join(format!("{name}{}", std::env::consts::EXE_SUFFIX)),
        )
        .unwrap();
    }

    let segment = Segment::new();
    let catalog = Arc::new(RwLock::new(TagCatalog::new()));
    let status = IoStatusHandle::default();
    let states = RuntimeStates::default();
    let tags = Arc::new(BusTags::<DefaultShm>::new(
        Arc::clone(&catalog),
        &segment.0,
        CAPACITY,
    ));
    let mut registry = DriverRegistry::discover(&[modules.path().to_path_buf()]).await;
    assert!(registry.problems().is_empty(), "{:?}", registry.problems());
    registry.register("modbus", || Box::new(driver_modbus::ModbusDriver::new()));
    registry.set_runtime_env(RuntimeEnv {
        tags: tags.clone(),
        io: status.clone(),
        states: states.clone(),
    });

    runtimes.validate().unwrap();
    let runtime = IoRuntime::start(
        config((sensors.port(), drive.port()), &runtimes),
        None,
        &registry,
        catalog,
        ShmWriterHandle::<DefaultShm>::new(&segment.0, CAPACITY).unwrap(),
        status.clone(),
    )
    .await;
    for c in &status.snapshot().connections {
        assert!(
            c.detail.is_none(),
            "connection {} did not start: {:?}",
            c.id,
            c.detail
        );
    }
    Plant {
        sensors,
        drive,
        runtime,
        status,
        states,
        tags,
        registry,
        _modules: modules,
        _segment: segment,
    }
}

impl Plant {
    fn tank(&self) -> RuntimeStatus {
        self.states
            .snapshot()
            .into_iter()
            .find(|s| s.id == "tank")
            .expect("the runtime 'tank' reports")
    }

    /// A tag as the bus has it: its value, and whether it is GOOD.
    async fn tag(&self, uri: &str) -> (Option<Value>, bool) {
        let read = self.tags.read(&[uri.to_string()]).await.remove(0);
        (read.value, read.good)
    }

    /// The plant as it is now.
    async fn seen(&self) -> Seen {
        Seen {
            run: self.drive.holding(0).await,
            speed: self.drive.holding(1).await,
            tank: self.states.snapshot().into_iter().find(|s| s.id == "tank"),
            pump: self.tag(T_PUMP).await,
            scans: self.tag(T_SCANS).await,
            setpoint: self.tag(T_SETPOINT).await,
        }
    }

    /// Wait until `what` holds, or panic saying what the plant looked like.
    async fn until(&self, what: &str, holds: impl Fn(&Seen) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let seen = self.seen().await;
            if holds(&seen) {
                return;
            }
            if Instant::now() > deadline {
                panic!("never: {what}\n  {seen:?}");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn drive_is(&self, what: &str, run: u16, speed: u16) {
        self.until(what, |s| s.run == Some(run) && s.speed == Some(speed))
            .await;
    }
}

/// What a test looks at: the drive's registers, the runtime's status, and its published tags
/// as the bus has them (value, and whether GOOD).
#[derive(Debug)]
struct Seen {
    run: Option<u16>,
    speed: Option<u16>,
    tank: Option<RuntimeStatus>,
    pump: (Option<Value>, bool),
    scans: (Option<Value>, bool),
    setpoint: (Option<Value>, bool),
}

impl Seen {
    fn mode(&self) -> &str {
        self.tank.as_ref().map_or("gone", |t| t.mode.as_str())
    }
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

#[tokio::test]
async fn a_program_reads_a_tag_decides_and_the_device_takes_its_outputs() {
    let p = plant(TANK, 20, true).await;

    // Level 20, setpoint 50: the pump runs at (50 - 20) * 10.
    p.drive_is("the pump runs at 300", 1, 300).await;
    let tank = p.tank();
    assert_eq!(tank.mode, "run", "{tank:?}");
    assert_eq!(
        (tank.program.as_str(), tank.version.as_str()),
        ("Tank", "test-1")
    );
    assert!(tank.scans > 0 && tank.pid.is_some(), "{tank:?}");
    assert!(!tank.outputs_safe);
    assert_eq!(tank.outputs[1].written, Some(json!(300)), "{tank:?}");

    // What the program publishes is on the tag bus, current, typed.
    p.until("the pump tag is TRUE and GOOD", |s| {
        s.pump == (Some(json!(true)), true)
    })
    .await;
    let (first, good) = p.tag(T_SCANS).await;
    assert!(good);
    let first = first.and_then(|v| v.as_i64());
    p.until("it keeps scanning", |s| {
        s.scans.0.as_ref().and_then(|v| v.as_i64()) > first
    })
    .await;
    assert_eq!(p.tag(T_SETPOINT).await, (Some(json!(50)), true));

    // The program follows the plant: a higher level slows the pump, a full tank stops it.
    p.sensors.set_input(0, 45).await;
    p.drive_is("the pump runs at 50", 1, 50).await;
    p.sensors.set_input(0, 70).await;
    p.drive_is("the pump stops", 0, 0).await;
    p.until("the pump tag is FALSE", |s| {
        s.pump == (Some(json!(false)), true)
    })
    .await;

    p.runtime.shutdown().await;
}

#[tokio::test]
async fn an_output_is_written_by_its_program_alone_and_an_operator_changes_a_setpoint() {
    let p = plant(TANK, 20, true).await;
    p.drive_is("the pump runs at 300", 1, 300).await;

    // The speed is the program's: an operator's write is refused and nothing reaches the drive.
    let refused = p.status.write_tag(SPEED, json!(5)).await;
    assert!(
        matches!(&refused, Err(WriteError::Owned(by)) if by == "runtime 'tank'"),
        "{refused:?}"
    );
    assert_eq!(p.drive.holding(1).await, Some(300));

    // A published variable that is not a setpoint is not written either.
    let refused = p.status.write_tag(T_SCANS, json!(0)).await;
    assert_eq!(refused, Err(WriteError::ReadOnly));

    // The setpoint is: the operator lowers it below the level and the program stops the pump.
    p.status
        .write_tag(T_SETPOINT, json!(10))
        .await
        .expect("a setpoint is taken");
    p.drive_is("the pump stops at setpoint 10", 0, 0).await;
    p.until("the setpoint tag reads 10", |s| {
        s.setpoint == (Some(json!(10)), true)
    })
    .await;
    // A setpoint that does not fit its variable is refused, never wrapped.
    let refused = p.status.write_tag(T_SETPOINT, json!(100_000)).await;
    assert!(
        matches!(&refused, Err(WriteError::Refused(why)) if why.contains("does not fit")),
        "{refused:?}"
    );

    p.runtime.shutdown().await;
}

#[tokio::test]
async fn a_stopped_program_leaves_its_outputs_at_their_safe_values_and_run_is_a_cold_start() {
    let p = plant(TANK, 20, true).await;
    p.drive_is("the pump runs at 300", 1, 300).await;
    p.status.write_tag(T_SETPOINT, json!(30)).await.unwrap();
    p.drive_is("the pump runs at 100 with setpoint 30", 1, 100)
        .await;

    assert!(p.states.command("tank", false));
    p.drive_is("stopped: pump off, speed at its safe value", 0, SAFE_SPEED)
        .await;
    p.until("it reports stop with safe outputs", |s| {
        s.mode() == "stop" && s.tank.as_ref().is_some_and(|t| t.outputs_safe)
    })
    .await;
    // A stopped program's tags are not current, and say why.
    p.until("the pump tag is not GOOD", |s| !s.pump.1).await;
    // Its outputs stay its own: stopping does not hand them to the operator.
    assert!(matches!(
        p.status.write_tag(SPEED, json!(5)).await,
        Err(WriteError::Owned(_))
    ));

    // RUN again is a cold start: the setpoint is back at its initial 50, not the 30 it had.
    assert!(p.states.command("tank", true));
    p.drive_is("running again from initial values", 1, 300)
        .await;
    p.until("the setpoint tag reads 50 again", |s| {
        s.setpoint == (Some(json!(50)), true)
    })
    .await;
    assert!(
        !p.states.command("nobody", true),
        "an unknown runtime is not commanded"
    );

    p.runtime.shutdown().await;
}

#[tokio::test]
async fn a_program_loaded_without_run_waits_in_stop_with_its_outputs_safe() {
    let p = plant(TANK, 20, false).await;
    p.drive_is("loaded in stop: outputs safe", 0, SAFE_SPEED)
        .await;
    assert_eq!(p.tank().mode, "stop");
    assert!(p.states.command("tank", true));
    p.drive_is("then run", 1, 300).await;
    p.runtime.shutdown().await;
}

#[tokio::test]
async fn when_the_runtime_process_dies_its_outputs_go_safe_and_it_is_started_again() {
    let p = plant(TANK, 20, true).await;
    p.drive_is("the pump runs at 300", 1, 300).await;
    let before = p.tank();

    kill(before.pid.unwrap());
    p.drive_is("process dead: outputs at their safe values", 0, SAFE_SPEED)
        .await;
    p.until("it reports unreachable", |s| s.mode() == "unreachable")
        .await;
    assert!(
        !p.tag(T_PUMP).await.1,
        "its tags are not GOOD while it is gone"
    );

    // The module is started again with the same program, from a cold start, and runs.
    p.drive_is("back: the pump runs at 300", 1, 300).await;
    let after = p.tank();
    assert_eq!(after.mode, "run");
    assert_ne!(after.pid, before.pid);
    assert_eq!(after.restarts, before.restarts + 1);

    p.runtime.shutdown().await;
}

#[tokio::test]
async fn a_program_an_engineer_stopped_stays_stopped_when_its_process_is_started_again() {
    let p = plant(TANK, 20, true).await;
    p.drive_is("the pump runs at 300", 1, 300).await;
    assert!(p.states.command("tank", false));
    p.drive_is("stopped: safe values", 0, SAFE_SPEED).await;
    p.until("it reports stop", |s| s.mode() == "stop").await;
    let before = p.tank();

    // The module is restarted with a configuration that says run. It must not get to command
    // the drive, even for one exchange: the engineer's stop outlives the process.
    kill(before.pid.unwrap());
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut restarted = false;
    while !restarted || p.tank().mode != "stop" {
        assert_eq!(
            (p.drive.holding(0).await, p.drive.holding(1).await),
            (Some(0), Some(SAFE_SPEED)),
            "the stopped program wrote an output after its restart"
        );
        restarted |= p.tank().pid.is_some_and(|pid| Some(pid) != before.pid);
        assert!(
            Instant::now() < deadline,
            "never restarted into stop: {:?}",
            p.tank()
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    // And a while longer, in stop.
    for _ in 0..40 {
        assert_eq!(p.drive.holding(1).await, Some(SAFE_SPEED));
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    p.runtime.shutdown().await;
}

#[tokio::test]
async fn when_everything_is_unloaded_at_once_the_runtime_goes_first_so_its_safe_values_arrive() {
    // Removed together with the connection its outputs go through.
    let mut p = plant(TANK, 20, true).await;
    p.drive_is("the pump runs at 300", 1, 300).await;
    p.runtime
        .apply(
            IoConfig {
                connections: vec![],
            },
            None,
            &p.registry,
        )
        .await;
    assert_eq!(
        (p.drive.holding(0).await, p.drive.holding(1).await),
        (Some(0), Some(SAFE_SPEED)),
        "the drive was told the safe values before its connection was closed"
    );
    p.runtime.shutdown().await;

    // And at shutdown.
    let p = plant(TANK, 20, true).await;
    p.drive_is("the pump runs at 300", 1, 300).await;
    let drive = p.drive;
    p.runtime.shutdown().await;
    assert_eq!(
        (drive.holding(0).await, drive.holding(1).await),
        (Some(0), Some(SAFE_SPEED))
    );
}

#[tokio::test]
async fn a_module_that_ignores_stop_still_cannot_write_once_an_engineer_has_stopped_it() {
    // The stubborn module reports RUN whatever it is told and always asks for 300.
    let stubborn: RuntimesConfig = serde_json::from_value(json!({"instances": [{
        "id": "tank", "module": "stubborn", "run": true,
        "program": {
            "name": "stubborn", "version": "1", "interval_ms": 20, "code": "",
            "outputs": [{"variable": "x", "tag": SPEED, "safe": SAFE_SPEED}],
            "published": [{"variable": "x", "tag": T_SCANS}]
        }
    }]}))
    .unwrap();
    let p = plant_running(stubborn, 20).await;
    p.until("it commands 300", |s| s.speed == Some(300)).await;

    assert!(p.states.command("tank", false));
    p.until("the safe value is written", |s| s.speed == Some(SAFE_SPEED))
        .await;
    // It still says it runs and still asks for 300. Nothing it asks for is written.
    for _ in 0..60 {
        assert_eq!(p.drive.holding(1).await, Some(SAFE_SPEED));
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let tank = p.tank();
    assert_eq!(
        tank.mode, "run",
        "what the module claims is reported as it claims it"
    );
    assert!(
        tank.outputs_safe,
        "and the outputs are safe all the same: {tank:?}"
    );

    assert!(p.states.command("tank", true));
    p.until("allowed again, it commands 300", |s| s.speed == Some(300))
        .await;
    p.runtime.shutdown().await;
}

#[tokio::test]
async fn a_scan_that_faults_stops_the_program_and_its_outputs_go_safe() {
    let p = plant(DIVIDES, 4, true).await;
    p.drive_is("1000 / 4", 1, 250).await;

    p.sensors.set_input(0, 0).await;
    p.drive_is("faulted: outputs at their safe values", 0, SAFE_SPEED)
        .await;
    p.until("it reports the fault", |s| s.mode() == "fault")
        .await;
    let tank = p.tank();
    assert!(
        tank.reason
            .as_deref()
            .unwrap_or("")
            .contains("division by zero"),
        "{tank:?}"
    );

    // It stays stopped until an engineer runs it again.
    p.sensors.set_input(0, 5).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(p.tank().mode, "fault");
    assert!(p.states.command("tank", true));
    p.drive_is("run again: 1000 / 5", 1, 200).await;

    p.runtime.shutdown().await;
}

#[tokio::test]
async fn a_tag_that_is_not_good_feeds_the_program_no_value_only_the_fact() {
    let p = plant(TANK, 20, true).await;
    p.drive_is("the pump runs at 300", 1, 300).await;

    // The sensor device goes away: the level is no longer GOOD. The program's `level_ok`
    // turns FALSE and it stops the pump; it did not keep pumping on a level it cannot see.
    p.sensors.stop();
    p.drive_is("level unknown: the pump stops", 0, 0).await;
    assert_eq!(p.tank().mode, "run", "the program itself keeps running");

    p.runtime.shutdown().await;
}

#[tokio::test]
async fn removing_a_runtime_makes_its_outputs_safe_and_gives_them_back() {
    let mut p = plant(TANK, 20, true).await;
    p.drive_is("the pump runs at 300", 1, 300).await;

    // The same field connections, no runtime.
    let none = RuntimesConfig::default();
    p.runtime
        .apply(
            config((p.sensors.port(), p.drive.port()), &none),
            None,
            &p.registry,
        )
        .await;
    assert_eq!(
        p.drive.holding(0).await,
        Some(0),
        "made safe before it was unloaded"
    );
    assert_eq!(p.drive.holding(1).await, Some(SAFE_SPEED));
    assert!(p.states.snapshot().is_empty());
    assert!(p.status.owner_of(SPEED).is_none());
    assert!(!p.tag(T_PUMP).await.1);

    // With no program owning it, the operator may write the speed again.
    p.status
        .write_tag(SPEED, json!(5))
        .await
        .expect("nobody owns it now");
    assert_eq!(p.drive.holding(1).await, Some(5));

    p.runtime.shutdown().await;
}

#[tokio::test]
async fn a_program_that_does_not_load_or_names_a_variable_it_lacks_is_refused_by_the_module() {
    let modules = tempfile::tempdir().unwrap();
    let exe = modules.path().join(format!(
        "mythos-module-runtime-vm{}",
        std::env::consts::EXE_SUFFIX
    ));
    std::fs::copy(UNDER_TEST, &exe).unwrap();
    let discovered = mythos_module::discover(&[modules.path().to_path_buf()]).await;
    assert!(discovered.problems.is_empty(), "{:?}", discovered.problems);
    let module = Arc::new(discovered.modules.into_iter().next().unwrap());
    assert_eq!(module.manifest.kind, mythos_module::ModuleKind::Runtime);
    assert_eq!(
        module.manifest.profile["name"], "pc",
        "it says what it can run"
    );

    /// What the module says of the instance once `change` has been made to it.
    async fn ask(module: &Arc<mythos_module::ModuleBinary>, change: &dyn Fn(&mut Value)) -> String {
        use mythos_plugin_sdk::ProtocolDriver;
        let mut instance = serde_json::to_value(&runtimes(TANK, true).instances[0]).unwrap();
        change(&mut instance);
        let driver = mythos_module::ProcessDriver::new(Arc::clone(module));
        match driver.describe_config(&instance).await {
            Ok(tags) => format!("accepted: {} tags", tags.len()),
            Err(e) => e.to_string(),
        }
    }
    assert_eq!(ask(&module, &|_| {}).await, "accepted: 3 tags");
    let said = ask(&module, &|i| {
        i["program"]["inputs"][0]["variable"] = json!("flow")
    })
    .await;
    assert!(said.contains("the program has no variable flow"), "{said}");
    let said = ask(&module, &|i| i["program"]["code"] = json!("00ff")).await;
    assert!(said.contains("the program does not load"), "{said}");
    let said = ask(&module, &|i| {
        i["program"]["inputs"][1]["variable"] = json!("level")
    })
    .await;
    assert!(said.contains("so it must be a BOOL"), "{said}");
    let said = ask(&module, &|i| i["program"]["interval_ms"] = json!(0)).await;
    assert!(said.contains("interval_ms must be at least 1"), "{said}");
}

#[tokio::test]
async fn a_program_that_publishes_no_tag_runs_and_writes_its_outputs() {
    // Outputs only: nothing of the program is a tag of its own.
    let mut outputs_only = runtimes(TANK, true);
    outputs_only.instances[0].program.published.clear();
    let p = plant_running(outputs_only, 20).await;
    p.drive_is("the pump runs at 300", 1, 300).await;
    let tank = p.tank();
    assert_eq!(tank.mode, "run", "{tank:?}");
    assert!(tank.scans > 0, "{tank:?}");

    // Stopped, its outputs go to their safe values like any other program's.
    assert!(p.states.command("tank", false));
    p.drive_is("stopped: pump off, speed at its safe value", 0, SAFE_SPEED)
        .await;
    p.runtime.shutdown().await;
}

#[tokio::test]
async fn a_program_bound_to_nothing_at_all_still_scans() {
    let mut unbound = runtimes(TANK, true);
    let program = &mut unbound.instances[0].program;
    program.inputs.clear();
    program.outputs.clear();
    program.published.clear();
    let p = plant_running(unbound, 20).await;
    p.until("it scans", |s| {
        s.tank
            .as_ref()
            .is_some_and(|t| t.mode == "run" && t.scans > 3)
    })
    .await;
    // It was given no output, so it wrote none: the drive is as the test left it.
    assert_eq!(p.drive.holding(1).await, Some(999));
    p.runtime.shutdown().await;
}
