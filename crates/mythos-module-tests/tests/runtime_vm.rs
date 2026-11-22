//! A control program running end to end (ADR-013 §2.4): Structured Text compiled here, loaded
//! into the real runtime module process, hosted by the field I/O runtime, reading a tag from
//! one real Modbus device (a simulator on a loopback port) and commanding another.
//!
//! The program is a tank: run the pump while the level is known and below the setpoint, at a
//! speed that follows the distance to it.

use driver_modbus::sim::ModbusSimServer;
use mythos_core::io::{DriverRegistry, IoConfig, IoRuntime, IoStatusHandle, WriteError};
use mythos_core::runtimes::{
    BusTags, ForceAsk, RuntimeEnv, RuntimeStates, RuntimeStatus, RuntimesConfig, TagSource,
};
use mythos_core::tag::TagCatalog;
use mythos_logic_compiler::{
    check_project, compile, compile_sources, DataBlock, Sources, TargetProfile,
};
use mythos_module::protocol::{
    ProgramVariables, RetainedValue, RuntimeMode, WatchReply, WatchedValue,
};
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

/// As TANK, with what watching reaches beyond a variable bound to a tag: locals, arrays, a
/// timer's members, a REAL, strings.
const WATCHED: &str = "
PROGRAM Tank
VAR_INPUT level : INT; level_ok : BOOL; setpoint : INT := 50; END_VAR
VAR_OUTPUT pump : BOOL; speed : INT; END_VAR
VAR
  scans : DINT;
  hist : ARRAY [1..3] OF INT := [10, 20, 30];
  grid : ARRAY [0..1, 1..2] OF INT := [1, 2, 3, 4];
  t : TON;
  half : REAL;
  label : STRING[12];
  names : ARRAY [1..2] OF STRING[4] := ['ab', 'cd'];
END_VAR
  scans := scans + 1;
  label := CONCAT('level ', INT_TO_STRING(level));
  t(IN := TRUE, PT := T#1h);
  hist[2] := level;
  half := INT_TO_REAL(level) / 2.0;
  pump := level_ok AND (level < setpoint);
  IF pump THEN
    speed := (setpoint - level) * 10;
  ELSE
    speed := 0;
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
        retain: None,
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

    /// What the runtime's module says of `variables`: asked until it has answered for exactly
    /// these, as the API asks.
    async fn watched(&self, variables: &[&str]) -> Result<WatchReply, String> {
        let variables: Vec<String> = variables.iter().map(|v| (*v).to_string()).collect();
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            assert!(self.states.watch("tank", &variables), "'tank' is loaded");
            if let Some((age, answer)) = self.states.watched("tank", &variables) {
                if age < Duration::from_millis(200) {
                    return answer;
                }
            }
            assert!(Instant::now() < deadline, "no answer for {variables:?}");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Wait until the watched variables are as `holds` says, or panic with what they were.
    async fn watched_until(
        &self,
        what: &str,
        variables: &[&str],
        holds: impl Fn(&WatchReply) -> bool,
    ) -> WatchReply {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let reply = self.watched(variables).await.expect("the module answers");
            if holds(&reply) {
                return reply;
            }
            assert!(Instant::now() < deadline, "never: {what}\n  {reply:?}");
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
    // The runtime reports what the device confirmed when its exchange ends, so the register can
    // hold 300 a moment before the status says so: wait for the status, rather than assume both
    // change at once (it failed so on Linux in CI, PR #125).
    p.until("the status says 300 was confirmed", |s| {
        s.tank
            .as_ref()
            .is_some_and(|t| t.outputs[1].written == Some(json!(300)))
    })
    .await;
    let tank = p.tank();
    assert_eq!(tank.mode, "run", "{tank:?}");
    assert_eq!(
        (tank.program.as_str(), tank.version.as_str()),
        ("Tank", "test-1")
    );
    assert!(
        tank.scans.is_some_and(|n| n > 0) && tank.pid.is_some(),
        "{tank:?}"
    );
    assert!(
        tank.last_scan_us.is_some() && tank.max_scan_us.is_some() && tank.overruns.is_some(),
        "a running program's scan figures are measured: {tank:?}"
    );
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
    let tank = p.tank();
    assert_eq!(tank.mode, "stop");
    // It has never scanned: that is known (0 scans), and no scan time was measured.
    assert_eq!(
        (
            tank.scans,
            tank.last_scan_us,
            tank.max_scan_us,
            tank.overruns
        ),
        (Some(0), None, None, Some(0)),
        "{tank:?}"
    );
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
    // Nothing is known of a program whose module does not answer: no figure, not zeros.
    p.until("unreachable, with no scan figures", |s| {
        s.tank.as_ref().is_some_and(|t| {
            t.mode == "unreachable"
                && (t.scans, t.last_scan_us, t.max_scan_us, t.overruns) == (None, None, None, None)
        })
    })
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
    outputs_only.instances[0]
        .program
        .as_mut()
        .unwrap()
        .published
        .clear();
    let p = plant_running(outputs_only, 20).await;
    p.drive_is("the pump runs at 300", 1, 300).await;
    // The drive is written by a scan, but the runtime's status is a report of its own that
    // can come later (macOS showed scans: 0 here): wait for it rather than read it once.
    p.until("it reports its scans", |s| {
        s.tank
            .as_ref()
            .is_some_and(|t| t.mode == "run" && t.scans.is_some_and(|n| n > 0))
    })
    .await;

    // Stopped, its outputs go to their safe values like any other program's.
    assert!(p.states.command("tank", false));
    p.drive_is("stopped: pump off, speed at its safe value", 0, SAFE_SPEED)
        .await;
    p.runtime.shutdown().await;
}

#[tokio::test]
async fn a_program_bound_to_nothing_at_all_still_scans() {
    let mut unbound = runtimes(TANK, true);
    let program = unbound.instances[0].program.as_mut().unwrap();
    program.inputs.clear();
    program.outputs.clear();
    program.published.clear();
    let p = plant_running(unbound, 20).await;
    p.until("it scans", |s| {
        s.tank
            .as_ref()
            .is_some_and(|t| t.mode == "run" && t.scans.is_some_and(|n| n > 3))
    })
    .await;
    // It was given no output, so it wrote none: the drive is as the test left it.
    assert_eq!(p.drive.holding(1).await, Some(999));
    p.runtime.shutdown().await;
}

/// One watched variable of a reply, by the name it was asked with.
fn value<'a>(reply: &'a WatchReply, variable: &str) -> &'a WatchedValue {
    reply
        .values
        .iter()
        .find(|v| v.variable == variable)
        .unwrap_or_else(|| panic!("{variable} is not in {reply:?}"))
}

#[tokio::test]
async fn any_variable_is_read_as_the_last_scan_left_it_bound_to_a_tag_or_not() {
    let p = plant(WATCHED, 20, true).await;
    p.drive_is("the pump runs at 300", 1, 300).await;

    let names = [
        "level",
        "pump",
        "scans",
        "half",
        "hist[2]",
        "HIST[1]",
        "grid[1, 2]",
        "grid[0,2]",
        "t.Q",
        "t.ET",
        "label",
        "names[2]", // what has a value
        "flow",
        "hist",
        "hist[4]",
        "hist[x]",
        "hist[2",
        "level[1]",
        "grid[1]",
        "t", // what has none
    ];
    let reply = p
        // The first scan that sees the level can be the timer's first too, where its elapsed
        // time is still 0 (observed on macOS): wait for both, rather than assume a scan apart.
        .watched_until(
            "the level reached the program and its timer ran",
            &names,
            |r| {
                value(r, "hist[2]").value == Some(json!(20))
                    && value(r, "t.ET")
                        .value
                        .as_ref()
                        .and_then(Value::as_i64)
                        .is_some_and(|ns| ns > 0)
            },
        )
        .await;
    assert_eq!(reply.state.mode, RuntimeMode::Run);
    assert_eq!(reply.state.program, "Tank");
    assert_eq!(reply.state.version, "test-1");
    assert_eq!(
        reply
            .values
            .iter()
            .map(|v| v.variable.as_str())
            .collect::<Vec<_>>(),
        names,
        "one answer per name, in the order asked, under the name asked"
    );

    let has = |variable: &str, ty: &str, expected: Value| {
        let v = value(&reply, variable);
        assert_eq!(
            (v.ty.as_deref(), &v.value, &v.error),
            (Some(ty), &Some(expected), &None),
            "{variable}"
        );
    };
    has("level", "INT", json!(20));
    has("pump", "BOOL", json!(true));
    has("half", "REAL", json!(10.0));
    has("hist[2]", "INT", json!(20));
    // Names do not differ by case, and an untouched element keeps its initial value.
    has("HIST[1]", "INT", json!(10));
    // The rightmost subscript varies fastest: [0,1] [0,2] [1,1] [1,2] hold 1 2 3 4.
    has("grid[1, 2]", "INT", json!(4));
    has("grid[0,2]", "INT", json!(2));
    has("t.Q", "BOOL", json!(false));
    // A STRING is its characters; an element of an array of them as well.
    has("label", "STRING[12]", json!("level 20"));
    has("names[2]", "STRING[4]", json!("cd"));
    let scans = value(&reply, "scans");
    assert!(
        scans.value.as_ref().and_then(Value::as_i64).unwrap() > 0,
        "{scans:?}"
    );
    let elapsed = value(&reply, "t.ET");
    assert_eq!(elapsed.ty.as_deref(), Some("TIME"), "{elapsed:?}");
    assert!(
        elapsed.value.as_ref().and_then(Value::as_i64).unwrap() > 0,
        "a TIME is its nanoseconds: {elapsed:?}"
    );

    let lacks = |variable: &str, why: &str| {
        let v = value(&reply, variable);
        assert_eq!(v.value, None, "{variable} has no value: {v:?}");
        assert!(
            v.error.as_deref().unwrap_or("").contains(why),
            "{variable}: {v:?}"
        );
    };
    lacks("flow", "the program has no variable flow");
    lacks("hist", "name one element");
    lacks("hist[4]", "subscript 4 of hist is outside 1..3");
    lacks("hist[x]", "are not whole numbers");
    lacks("hist[2", "is not a variable or an element of an array");
    lacks("level[1]", "level is a INT, not an array");
    lacks("grid[1]", "grid has 2 subscripts, not 1");
    assert!(
        value(&reply, "t").error.is_some(),
        "an instance has no single value"
    );

    // The values follow the program, and watching it changes nothing it does.
    p.sensors.set_input(0, 30).await;
    p.watched_until("the new level is seen", &["level", "half"], |r| {
        value(r, "level").value == Some(json!(30)) && value(r, "half").value == Some(json!(15.0))
    })
    .await;
    p.drive_is("the pump follows the level as before", 1, 200)
        .await;

    // A stopped program is still read: what it stopped with, and that it is stopped.
    assert!(p.states.command("tank", false));
    let stopped = p
        .watched_until("it says it is stopped", &["level", "scans"], |r| {
            r.state.mode == RuntimeMode::Stop
        })
        .await;
    assert_eq!(value(&stopped, "level").value, Some(json!(30)));
    let frozen = value(&stopped, "scans").value.clone();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let later = p.watched(&["level", "scans"]).await.unwrap();
    assert_eq!(value(&later, "scans").value, frozen, "no scan runs in STOP");

    assert!(
        !p.states.watch("nobody", &["level".to_string()]),
        "an unknown runtime is not watched"
    );
    p.runtime.shutdown().await;
}

#[tokio::test]
async fn a_runtime_module_that_gives_no_values_says_so_and_runs_on() {
    // The stubborn module is a runtime module with no `watch` of its own.
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

    let answer = p.watched(&["x"]).await;
    assert!(
        answer
            .as_ref()
            .is_err_and(|why| why.contains("does not give the values of its variables")),
        "{answer:?}"
    );
    // Being asked did it no harm.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(p.tank().mode, "run");
    assert_eq!(p.drive.holding(1).await, Some(300));
    p.runtime.shutdown().await;
}

// -----------------------------------------------------------------------------
// Several programs in tasks of their own (ADR-013 §2.19, C8b)
// -----------------------------------------------------------------------------

/// The speed half of the tank, in a program of its own.
const SPEEDS: &str = "
PROGRAM Speeds
VAR_INPUT level : INT; level_ok : BOOL; setpoint : INT := 50; END_VAR
VAR_OUTPUT speed : INT; END_VAR
VAR runs : DINT; END_VAR
  runs := runs + 1;
  IF level_ok AND (level < setpoint) THEN
    speed := (setpoint - level) * 10;
  ELSE
    speed := 0;
  END_IF;
END_PROGRAM
";

/// As SPEEDS, but it divides by the level once the level is known.
const SPEEDS_DIVIDE: &str = "
PROGRAM Speeds
VAR_INPUT level : INT; level_ok : BOOL; setpoint : INT := 50; END_VAR
VAR_OUTPUT speed : INT; END_VAR
VAR runs : DINT; END_VAR
  runs := runs + 1;
  IF level_ok THEN
    speed := 1000 / level;
  END_IF;
END_PROGRAM
";

/// The run half of the tank, in a program of its own.
const RUNS: &str = "
PROGRAM Runs
VAR_INPUT level : INT; level_ok : BOOL; END_VAR
VAR_OUTPUT pump : BOOL; END_VAR
VAR runs : DINT; END_VAR
  runs := runs + 1;
  pump := level_ok AND (level < 50);
END_PROGRAM
";

/// One program of the runtime `tank`, as JSON, with its bindings.
fn program_json(source: &str, version: &str, outputs: Value, published: Value) -> Value {
    let compiled = compile(source, &TargetProfile::pc()).unwrap_or_else(|d| panic!("{d:?}"));
    json!({
        "name": compiled.name,
        "version": version,
        "interval_ms": 20,
        "code": hex::encode(&compiled.bytecode),
        "symbols": compiled.symbols,
        "inputs": [
            {"variable": "level", "tag": LEVEL},
            {"variable": "level_ok", "tag": LEVEL, "of": "good"}
        ],
        "outputs": outputs,
        "published": published
    })
}

/// The runtime `tank` in the VM module: `speeds` drives the speed every 20 ms at priority 1
/// (task `fast`), RUNS the pump every 50 ms at priority 5 (task `slow`).
fn two_programs(speeds: &str) -> RuntimesConfig {
    serde_json::from_value(json!({"instances": [{
        "id": "tank", "module": "vm", "run": true,
        "programs": [
            program_json(speeds, "s-1",
                json!([{"variable": "speed", "tag": SPEED, "safe": SAFE_SPEED}]),
                json!([{"variable": "runs", "tag": T_SCANS}])),
            program_json(RUNS, "r-1",
                json!([{"variable": "pump", "tag": PUMP, "safe": false}]),
                json!([{"variable": "pump", "tag": T_PUMP}]))
        ],
        "tasks": [
            {"name": "fast", "kind": {"type": "cyclic", "interval_ms": 20}, "priority": 1, "programs": ["Speeds"]},
            {"name": "slow", "kind": {"type": "cyclic", "interval_ms": 50}, "priority": 5, "programs": ["Runs"]}
        ]
    }]}))
    .unwrap()
}

/// The stubborn module (which says nothing of tasks: an older module) given `instance`'s
/// programs and tasks.
fn stubborn_with(programs: Value, tasks: Value) -> RuntimesConfig {
    let mut instance = json!({"id": "tank", "module": "stubborn", "run": true, "tasks": tasks});
    if programs.as_array().is_some_and(|p| p.len() == 1) {
        instance["program"] = programs[0].clone();
    } else {
        instance["programs"] = programs;
    }
    serde_json::from_value(json!({ "instances": [instance] })).unwrap()
}

fn stubborn_program(name: &str, tag: &str) -> Value {
    json!({
        "name": name, "version": "1", "interval_ms": 20, "code": "",
        "outputs": [{"variable": "x", "tag": tag, "safe": SAFE_SPEED}]
    })
}

/// A file of today, as the server writes it (`to_vec_pretty`): the fields of the types in their
/// order, and the keys of `symbols`, which the core holds as an opaque JSON value, sorted.
const FILE_OF_TODAY: &str = r#"{
  "instances": [
    {
      "id": "tank",
      "module": "vm",
      "run": true,
      "program": {
        "name": "Tank",
        "version": "v1",
        "interval_ms": 20,
        "code": "00ff",
        "symbols": [
          {
            "offset": 0,
            "path": "level",
            "type": "INT"
          }
        ],
        "inputs": [
          {
            "variable": "level",
            "tag": "external.Plant.LT1.level",
            "of": "value"
          }
        ],
        "outputs": [
          {
            "variable": "speed",
            "tag": "external.Plant.P1.speed",
            "safe": 7
          }
        ],
        "published": [
          {
            "variable": "setpoint",
            "tag": "internal.Plant.Tank.setpoint",
            "writable": true
          }
        ]
      }
    }
  ]
}"#;

#[test]
fn a_runtimes_file_of_today_round_trips_byte_for_byte() {
    let config: RuntimesConfig = serde_json::from_str(FILE_OF_TODAY).unwrap();
    config.validate().unwrap();
    let instance = &config.instances[0];
    assert!(instance.programs.is_empty() && instance.tasks.is_empty());
    assert!(instance.runs_as_today());
    assert_eq!(instance.program_key(), ["Tank"]);
    assert_eq!(
        String::from_utf8(serde_json::to_vec_pretty(&config).unwrap()).unwrap(),
        FILE_OF_TODAY,
        "written back byte for byte"
    );
    // The configuration its module is started with is the instance of today, as JSON text.
    let today: Value = serde_json::from_str(FILE_OF_TODAY).unwrap();
    let connection = &config.connections()[0];
    assert_eq!(
        serde_json::to_string(&connection.config).unwrap(),
        serde_json::to_string(&today["instances"][0]).unwrap()
    );
    assert_eq!(connection.poll_interval_ms, 20);
}

#[tokio::test]
async fn the_status_of_a_runtime_of_today_has_exactly_todays_keys() {
    let p = plant(TANK, 20, true).await;
    p.drive_is("the pump runs at 300", 1, 300).await;
    p.until("it reports its scans", |s| {
        s.tank
            .as_ref()
            .is_some_and(|t| t.scans.is_some_and(|n| n > 0))
    })
    .await;
    let status = serde_json::to_value(p.tank()).unwrap();
    let mut keys: Vec<&str> = status
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    let mut today = vec![
        "id",
        "module",
        "program",
        "version",
        "interval_ms",
        "mode",
        "reason",
        "scans",
        "last_scan_us",
        "max_scan_us",
        "overruns",
        "outputs_safe",
        "outputs",
        "pid",
        "restarts",
        "forced",
        "forced_tags",
    ];
    today.sort_unstable();
    assert_eq!(keys, today, "{status}");
    assert_eq!(status["interval_ms"], json!(20), "a number, as today");
    assert_eq!(
        status["outputs"][1]["variable"], "speed",
        "named by its path"
    );
    p.runtime.shutdown().await;
}

#[tokio::test]
async fn an_output_of_a_program_whose_task_has_not_run_is_written_its_safe_value() {
    // Runs's task begins a minute after RUN. Until then the pump output, which the runtime
    // claims so nothing else writes it, is written its safe value (FALSE: 0), not left at the 9
    // the drive held before anything commanded it.
    let mut v = serde_json::to_value(two_programs(SPEEDS)).unwrap();
    v["instances"][0]["tasks"][1]["kind"]["phase_ms"] = json!(60_000);
    let cfg: RuntimesConfig = serde_json::from_value(v).unwrap();
    let p = plant_running(cfg, 20).await;
    p.drive_is(
        "Speeds drives the speed, the pump is at its safe value",
        0,
        300,
    )
    .await;
    // The status is of the last exchange the core has taken in, which can be one behind the
    // write the drive already shows: in the first exchange the pump is written its safe value
    // and the speed is still the 0 of a program that has not ended a run. So both are waited
    // for, not the pump alone (on CI the status was once read at that first exchange: scans 1,
    // speed written 0, while the drive held 300).
    p.until(
        "the status says the pump was written its safe value and the speed its computed one",
        |s| {
            s.tank.as_ref().is_some_and(|t| {
                let written = |tag: &str, value: serde_json::Value| {
                    t.outputs
                        .iter()
                        .any(|o| o.tag == tag && o.written == Some(value.clone()))
                };
                written(PUMP, json!(false)) && written(SPEED, json!(300))
            })
        },
    )
    .await;
    let tank = p.tank();
    assert_eq!(tank.mode, "run", "{tank:?}");
    let runs: Vec<(&str, u64)> = tank
        .tasks
        .iter()
        .map(|t| (t.name.as_str(), t.runs))
        .collect();
    assert_eq!(runs[1], ("slow", 0), "Runs has not run: {runs:?}");
    let output = |tag: &str| tank.outputs.iter().find(|o| o.tag == tag).unwrap().clone();
    assert_eq!(output(PUMP).written, Some(json!(false)), "{tank:?}");
    assert_eq!(output(SPEED).written, Some(json!(300)), "{tank:?}");
    assert!(!tank.outputs_safe, "the speed is a computed value");
    p.runtime.shutdown().await;
}

#[tokio::test]
async fn two_programs_in_two_tasks_drive_their_outputs_through_one_runtime() {
    let p = plant_running(two_programs(SPEEDS), 20).await;
    // Speeds commands 300, Runs the pump: one module process, two tasks.
    p.drive_is("the pump runs at 300", 1, 300).await;
    p.until("the pump tag is TRUE and GOOD", |s| {
        s.pump == (Some(json!(true)), true)
    })
    .await;
    p.until("both tasks have run", |s| {
        s.tank
            .as_ref()
            .is_some_and(|t| t.tasks.len() == 2 && t.tasks.iter().all(|k| k.runs > 0))
    })
    .await;
    let tank = p.tank();
    assert_eq!(tank.mode, "run", "{tank:?}");
    assert_eq!(
        (tank.program.as_str(), tank.version.as_str()),
        ("Speeds, Runs", "s-1, r-1")
    );
    assert_eq!(tank.interval_ms, Some(20), "its fastest cyclic task's");
    assert_eq!(
        tank.programs
            .iter()
            .map(|p| (p.name.as_str(), p.version.as_str()))
            .collect::<Vec<_>>(),
        vec![("Speeds", "s-1"), ("Runs", "r-1")]
    );
    let named: Vec<&str> = tank.tasks.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(named, vec!["fast", "slow"], "{tank:?}");
    assert_eq!(tank.tasks[0].programs, vec!["Speeds".to_string()]);
    assert_eq!(tank.tasks[1].priority, 5);
    assert_eq!(
        tank.outputs
            .iter()
            .map(|o| (o.tag.as_str(), o.variable.as_str()))
            .collect::<Vec<_>>(),
        vec![(SPEED, "Speeds.speed"), (PUMP, "Runs.pump")],
        "each output named by its program"
    );
    assert!(tank.scans.is_some_and(|n| n > 0), "{tank:?}");

    // Each program's variables, named by their program; a bare path names none.
    let reply = p
        .watched_until(
            "Speeds has seen the level",
            &["speeds.level", "Runs.pump", "level"],
            |r| value(r, "speeds.level").value == Some(json!(20)),
        )
        .await;
    assert_eq!(value(&reply, "Runs.pump").value, Some(json!(true)));
    let bare = value(&reply, "level");
    assert!(
        bare.error
            .as_deref()
            .unwrap_or("")
            .contains("name the program first"),
        "{bare:?}"
    );

    // A force names its program, and marks the tags of that program's variable alone.
    let forced = p
        .states
        .force(
            "tank",
            ForceAsk::Force {
                variable: "runs.pump".to_string(),
                value: json!(false),
            },
        )
        .unwrap()
        .await
        .unwrap()
        .expect("a variable of a running program is forced");
    assert_eq!(forced.forced.len(), 1, "{forced:?}");
    p.drive_is("the pump forced off, the speed still 300", 0, 300)
        .await;
    p.until("the forced tags are those bound to Runs.pump", |s| {
        s.tank
            .as_ref()
            .is_some_and(|t| t.forced_tags == vec![PUMP.to_string(), T_PUMP.to_string()])
    })
    .await;

    // The level changes: each program follows it in its own task.
    p.sensors.set_input(0, 70).await;
    p.drive_is("the pump stops and so does the speed", 0, 0)
        .await;
    p.runtime.shutdown().await;
}

#[tokio::test]
async fn a_fault_in_one_program_puts_every_output_of_the_runtime_safe() {
    let p = plant_running(two_programs(SPEEDS_DIVIDE), 4).await;
    p.drive_is("1000 / 4, the pump on", 1, 250).await;

    // Speeds divides by zero. Runs did nothing wrong; its output goes safe all the same.
    p.sensors.set_input(0, 0).await;
    p.drive_is("faulted: both outputs at their safe values", 0, SAFE_SPEED)
        .await;
    p.until("it reports the fault with safe outputs", |s| {
        s.mode() == "fault" && s.tank.as_ref().is_some_and(|t| t.outputs_safe)
    })
    .await;
    let reason = p.tank().reason.unwrap_or_default();
    assert!(
        reason.contains("task 'fast'")
            && reason.contains("program 'Speeds'")
            && reason.contains("division by zero"),
        "{reason}"
    );
    // It stays so: no program of the runtime writes until an engineer runs it again.
    p.sensors.set_input(0, 5).await;
    for _ in 0..20 {
        assert_eq!(
            (p.drive.holding(0).await, p.drive.holding(1).await),
            (Some(0), Some(SAFE_SPEED))
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(p.tank().mode, "fault");
    assert!(p.states.command("tank", true));
    p.drive_is("run again: 1000 / 5, the pump on", 1, 200).await;
    p.runtime.shutdown().await;
}

#[tokio::test]
async fn an_older_module_is_refused_a_configuration_it_cannot_run_and_says_why() {
    let mut p = plant(TANK, 20, true).await;
    p.drive_is("the pump runs at 300", 1, 300).await;

    let older = "the runtime module 'stubborn' (version 0.0.0) runs one program in one cyclic task and does not say it runs tasks: install a newer build of it";
    let two = stubborn_with(
        json!([stubborn_program("a", SPEED), stubborn_program("b", PUMP)]),
        json!([
            {"name": "f", "kind": {"type": "cyclic", "interval_ms": 20}, "priority": 1, "programs": ["a"]},
            {"name": "s", "kind": {"type": "cyclic", "interval_ms": 50}, "priority": 5, "programs": ["b"]}
        ]),
    );
    let slower = stubborn_with(
        json!([stubborn_program("a", SPEED)]),
        json!([{"name": "f", "kind": {"type": "cyclic", "interval_ms": 50}, "priority": 1, "programs": ["a"]}]),
    );
    for (config, has) in [
        (&two, "runtime 'tank' has 2 programs"),
        (
            &slower,
            "runtime 'tank' has task 'f' of kind cyclic with interval_ms 50, not its program's 20",
        ),
    ] {
        config.validate().unwrap();
        // The check a save makes: the module is never asked.
        let connection = &config.connections()[0];
        let driver = p
            .registry
            .create(&connection.driver)
            .expect("stubborn is a runtime");
        let said = driver
            .describe_config(&connection.config)
            .await
            .unwrap_err()
            .to_string();
        assert!(said.contains(&format!("{has}; {older}")), "{said}");
    }

    // Loaded all the same, it is not started: the start says the same.
    let report = p
        .runtime
        .apply(
            config((p.sensors.port(), p.drive.port()), &two),
            None,
            &p.registry,
        )
        .await;
    let failed = report
        .failed
        .iter()
        .find(|(id, _)| id == "runtime:tank")
        .map(|(_, why)| why.clone())
        .unwrap_or_default();
    assert!(failed.contains(older), "{report:?}");
    assert!(
        p.states.snapshot().iter().all(|s| s.module != "stubborn"),
        "no stubborn runtime runs"
    );
    p.runtime.shutdown().await;
}

#[tokio::test]
async fn an_older_module_runs_one_written_out_cyclic_task() {
    // Written out exactly as its implied task, at a priority of its own: it runs as today, so
    // the module that knows no tasks is given it.
    let config = stubborn_with(
        json!([stubborn_program("stubborn", SPEED)]),
        json!([{"name": "main", "kind": {"type": "cyclic", "interval_ms": 20}, "priority": 3, "programs": ["stubborn"]}]),
    );
    assert!(config.instances[0].runs_as_today());
    let p = plant_running(config, 20).await;
    p.until("it commands 300", |s| s.speed == Some(300)).await;
    let tank = p.tank();
    assert_eq!(tank.mode, "run");
    assert_eq!(tank.interval_ms, Some(20));
    assert!(
        tank.tasks.is_empty() && tank.programs.is_empty(),
        "a module that says nothing of tasks reports none: {tank:?}"
    );
    p.runtime.shutdown().await;
}

// -----------------------------------------------------------------------------
// Data blocks shared by the programs of one runtime (ADR-014, D2)
// -----------------------------------------------------------------------------

#[test]
fn a_retain_file_and_variables_of_today_read_and_write_back_without_a_data_key() {
    // A retain file of today: no `layout` key in, none out.
    let kept = r#"[{"path":"total","type":"DINT","bytes":"2a000000"}]"#;
    let values: Vec<RetainedValue> = serde_json::from_str(kept).unwrap();
    assert_eq!(values[0].layout, None);
    assert_eq!(serde_json::to_string(&values).unwrap(), kept);
    // The variables of a running build, as an online change carries them: unchanged.
    let carried = r#"{"clock_ns":5,"values":[{"path":"n","type":"DINT","bytes":"01000000"}]}"#;
    let variables: ProgramVariables = serde_json::from_str(carried).unwrap();
    assert_eq!(serde_json::to_string(&variables).unwrap(), carried);
    // The configuration a module of today is started with has no `data` key, at the instance
    // or at its program.
    let config: RuntimesConfig = serde_json::from_str(FILE_OF_TODAY).unwrap();
    let sent = &config.connections()[0].config;
    assert!(sent.get("data").is_none(), "{sent}");
    assert!(sent["program"].get("data").is_none(), "{sent}");
}

/// The project's types: the block `S` of type `Shared`.
const SHARED_TYPES: &str =
    "TYPE Shared : STRUCT level : INT; seen : INT; spare : INT := 5; END_STRUCT; END_TYPE";
const SHARED_BLOCK: DataBlock = DataBlock {
    name: "S",
    ty: "Shared",
};

/// Writes `S.level` from the level tag; reads nothing of S.
const LEVEL_WRITER: &str = "PROGRAM Writer
VAR_INPUT level : INT; level_ok : BOOL; END_VAR
VAR_EXTERNAL S : Shared; END_VAR
VAR runs : DINT; END_VAR
  runs := runs + 1;
  IF level_ok THEN S.level := level; END_IF;
END_PROGRAM";

/// Drives the speed from `S.level` and `S.spare`, which no program writes, and writes
/// `S.seen` back.
const LEVEL_READER: &str = "PROGRAM Reader
VAR_OUTPUT speed : INT; END_VAR
VAR_EXTERNAL S : Shared; END_VAR
  speed := S.level * 10 + S.spare;
  S.seen := S.level;
END_PROGRAM";

/// A program of the project compiled with its types and the block S, as JSON with its `data`.
fn shared_program(source: &str, extra: Value) -> Value {
    let c = compile_sources(
        &Sources {
            st: Some(("main.st", source)),
            library: &[("types.st", SHARED_TYPES)],
            data: &[SHARED_BLOCK],
            ..Default::default()
        },
        &TargetProfile::pc(),
    )
    .unwrap_or_else(|d| panic!("{d:?}"));
    let mut program = json!({
        "name": c.name, "version": "d-1", "interval_ms": 20,
        "code": hex::encode(&c.bytecode), "symbols": c.symbols, "data": c.data,
    });
    if let (Value::Object(p), Value::Object(e)) = (&mut program, extra) {
        p.extend(e);
    }
    program
}

/// The runtime's `data`: the block S as the project lays it out.
fn shared_data() -> Value {
    let layouts = check_project(&Sources {
        library: &[("types.st", SHARED_TYPES)],
        data: &[SHARED_BLOCK],
        ..Default::default()
    })
    .unwrap_or_else(|d| panic!("{d:?}"));
    Value::Array(
        layouts
            .into_iter()
            .map(|l| {
                json!({
                    "name": l.name, "type": l.ty, "layout": l.layout, "size": l.size,
                    "members": l.members.iter().map(|m| json!({
                        "name": m.name, "type": m.ty, "offset": m.offset, "size": m.size,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

/// The runtime `tank`: Writer every 50 ms at priority 5, Reader every 20 ms at priority 1,
/// sharing S.
fn sharing_tank() -> RuntimesConfig {
    serde_json::from_value(json!({"instances": [{
        "id": "tank", "module": "vm", "run": true,
        "programs": [
            shared_program(LEVEL_WRITER, json!({"inputs": [
                {"variable": "level", "tag": LEVEL},
                {"variable": "level_ok", "tag": LEVEL, "of": "good"}
            ]})),
            shared_program(LEVEL_READER, json!({
                "outputs": [{"variable": "speed", "tag": SPEED, "safe": SAFE_SPEED}]
            }))
        ],
        "tasks": [
            {"name": "slow", "kind": {"type": "cyclic", "interval_ms": 50}, "priority": 5, "programs": ["Writer"]},
            {"name": "fast", "kind": {"type": "cyclic", "interval_ms": 20}, "priority": 1, "programs": ["Reader"]}
        ],
        "data": shared_data()
    }]}))
    .unwrap()
}

#[tokio::test]
async fn a_block_written_by_one_program_drives_an_output_of_another() {
    let p = plant_running(sharing_tank(), 20).await;
    // The level reaches Reader only through S: 20 * 10 + the unwritten spare's 5.
    p.until("Reader drives 205 from what Writer handed over", |s| {
        s.speed == Some(205)
    })
    .await;
    p.sensors.set_input(0, 31).await;
    p.until("the speed follows the level, through the block", |s| {
        s.speed == Some(315)
    })
    .await;
    let reply = p
        .watched_until(
            "each copy holds the other's member",
            &["Writer.S.seen", "Reader.S.level", "Reader.S.spare"],
            |r| value(r, "Writer.S.seen").value == Some(json!(31)),
        )
        .await;
    assert_eq!(value(&reply, "Reader.S.level").value, Some(json!(31)));
    assert_eq!(value(&reply, "Reader.S.spare").value, Some(json!(5)));
    assert_eq!(p.tank().mode, "run");
    p.runtime.shutdown().await;
}

#[tokio::test]
async fn the_status_names_each_members_writer() {
    let p = plant_running(sharing_tank(), 20).await;
    p.until("it runs", |s| s.mode() == "run").await;
    let status = serde_json::to_value(p.tank()).unwrap();
    let layout = shared_data()[0]["layout"].clone();
    assert_eq!(
        status["data"],
        json!([{"name": "S", "type": "Shared", "layout": layout, "retain": false, "members": [
            {"name": "level", "type": "INT", "writer": "Writer"},
            {"name": "seen", "type": "INT", "writer": "Reader"},
            {"name": "spare", "type": "INT", "writer": null}
        ]}]),
        "{status}"
    );
    p.runtime.shutdown().await;
}

#[tokio::test]
async fn an_older_module_is_refused_data_blocks_and_says_why() {
    let mut p = plant(TANK, 20, true).await;
    p.drive_is("the pump runs at 300", 1, 300).await;
    // One program in its implied task runs as today for tasks; with data it is refused.
    let mut v = serde_json::to_value(stubborn_with(
        json!([stubborn_program("stubborn", SPEED)]),
        json!([]),
    ))
    .unwrap();
    v["instances"][0]["data"] = shared_data();
    let with_data: RuntimesConfig = serde_json::from_value(v).unwrap();
    with_data.validate().unwrap();
    assert!(with_data.instances[0].runs_as_today());
    let refusal = "runtime 'tank' has data blocks (S); the runtime module 'stubborn' (version 0.0.0) does not say it hands data blocks over: install a newer build of it";
    let connection = &with_data.connections()[0];
    let driver = p
        .registry
        .create(&connection.driver)
        .expect("stubborn is a runtime");
    let said = driver
        .describe_config(&connection.config)
        .await
        .unwrap_err()
        .to_string();
    assert!(said.contains(refusal), "{said}");
    // Loaded all the same, it is not started: the start says the same.
    let report = p
        .runtime
        .apply(
            config((p.sensors.port(), p.drive.port()), &with_data),
            None,
            &p.registry,
        )
        .await;
    let failed = report
        .failed
        .iter()
        .find(|(id, _)| id == "runtime:tank")
        .map(|(_, why)| why.clone())
        .unwrap_or_default();
    assert!(failed.contains(refusal), "{report:?}");
    assert!(
        p.states.snapshot().iter().all(|s| s.module != "stubborn"),
        "no stubborn runtime runs"
    );
    p.runtime.shutdown().await;
}
