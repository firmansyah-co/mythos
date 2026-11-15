//! The Modbus driver as a module: a separate process, found by discovery, driven by the I/O
//! runtime, killed and replaced while the server keeps running.
//!
//! Everything here is real: the module executable, the Modbus TCP simulator on a socket, a
//! POSIX shared-memory tag bus.

#![cfg(target_os = "linux")]

use driver_modbus::sim::ModbusSimServer;
use mythos_core::io::{
    ConnectionState, DriverRegistry, IoConfig, IoManager, IoRuntime, IoStatusHandle,
};
use mythos_core::tag::TagCatalog;
use mythos_module::{discover, ModuleKind};
use mythos_pal::PosixShm;
use mythos_shm::{QualityCode, ShmReaderHandle, ShmWriterHandle, TagDataType, TagSlot};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

const UNDER_TEST: &str = env!("CARGO_BIN_EXE_modbus-module-under-test");
const CAPACITY: usize = 64;
const TAG: &str = "external.Plant.PT101.pressure";
static SEQ: AtomicU32 = AtomicU32::new(0);

/// A directory holding the module under its shipped name, as an installation would.
fn module_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(UNDER_TEST, dir.path().join("mythos-module-modbus")).unwrap();
    dir
}

struct Segment(String);

impl Segment {
    fn new() -> Self {
        let name = format!(
            "mythos_module_test_{}_{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        );
        let _ = PosixShm::unlink(&name);
        Self(name)
    }
}

impl Drop for Segment {
    fn drop(&mut self) {
        let _ = PosixShm::unlink(&self.0);
    }
}

fn connection(port: u16) -> Value {
    json!({
        "id": "plc1",
        "driver": "modbus",
        "poll_interval_ms": 50,
        "config": {
            "transport": "tcp",
            "host": "127.0.0.1",
            "port": port,
            "timeout_ms": 300,
            "mappings": [{"uri": TAG, "address": 0, "function_code": 3}]
        }
    })
}

async fn registry(dir: &Path) -> DriverRegistry {
    DriverRegistry::discover(&[dir.to_path_buf()]).await
}

/// Read slot 0 until `accept` holds, or panic with the last slot seen.
async fn wait_for_slot(
    segment: &Segment,
    accept: impl Fn(&TagSlot) -> bool,
    what: &str,
) -> TagSlot {
    let mut reader = ShmReaderHandle::<PosixShm>::new(&segment.0, CAPACITY).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        reader.update();
        let slot = reader.read_tag(0).unwrap();
        if accept(&slot) {
            return slot;
        }
        if Instant::now() > deadline {
            panic!(
                "{what}: slot 0 never got there; last quality {:?}, type {}",
                slot.quality, slot.type_code
            );
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Process ids of this process's children running `<exe> --serve`. Tests run in parallel in
/// one process, so each test asks about the executable in its own directory.
fn module_children(exe: &Path) -> Vec<u32> {
    let exe = exe.to_string_lossy().into_owned();
    let me = std::process::id();
    let mut pids = Vec::new();
    for entry in std::fs::read_dir("/proc").unwrap().flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
            continue;
        };
        // Field 4, after the parenthesised command name, is the parent pid.
        let Some(after) = stat.rsplit_once(')').map(|(_, rest)| rest) else {
            continue;
        };
        let ppid: u32 = after
            .split_whitespace()
            .nth(1)
            .and_then(|p| p.parse().ok())
            .unwrap_or(0);
        if ppid != me {
            continue;
        }
        let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
        let args: Vec<String> = cmdline
            .split(|b| *b == 0)
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .collect();
        if args.first() == Some(&exe) && args.get(1).map(String::as_str) == Some("--serve") {
            pids.push(pid);
        }
    }
    pids
}

#[tokio::test]
async fn the_module_is_discovered_by_its_manifest() {
    let dir = module_dir();
    let found = discover(&[dir.path().to_path_buf()]).await;
    assert!(found.problems.is_empty(), "problems: {:?}", found.problems);
    assert_eq!(found.modules.len(), 1);
    let module = &found.modules[0];
    assert_eq!(module.manifest.id, "org.mythos.driver.modbus");
    assert_eq!(module.manifest.provides, "modbus");
    assert_eq!(module.manifest.kind, ModuleKind::Driver);
    assert_eq!(module.path, dir.path().join("mythos-module-modbus"));
    assert_eq!(
        module.manifest.config_schema["properties"]["mappings"]["type"],
        "array"
    );

    let registry = registry(dir.path()).await;
    assert_eq!(registry.names(), vec!["modbus".to_string()]);
    assert_eq!(registry.drivers()[0].source, "module");
}

#[tokio::test]
async fn files_that_are_not_usable_modules_are_reported_or_ignored() {
    use std::os::unix::fs::PermissionsExt;
    let dir = module_dir();
    let write = |name: &str, body: &str, mode: u32| {
        let path: PathBuf = dir.path().join(name);
        std::fs::write(&path, body).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
    };
    write(
        "mythos-module-broken",
        "#!/bin/sh\necho not-a-manifest\n",
        0o755,
    );
    write("mythos-module-failing", "#!/bin/sh\nexit 3\n", 0o755);
    write("mythos-module-not-executable", "#!/bin/sh\n", 0o644);
    write("mythos-module-modbus.d", "dep-info\n", 0o755);
    write("unrelated-tool", "#!/bin/sh\n", 0o755);

    let found = discover(&[dir.path().to_path_buf()]).await;
    assert_eq!(found.modules.len(), 1, "only the real module is usable");

    let mut reported: Vec<String> = found
        .problems
        .iter()
        .map(|p| p.path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    reported.sort();
    assert_eq!(
        reported,
        vec!["mythos-module-broken", "mythos-module-failing"]
    );
    let broken = found
        .problems
        .iter()
        .find(|p| p.path.ends_with("mythos-module-broken"))
        .unwrap();
    assert!(
        broken.message.contains("manifest is not valid"),
        "{}",
        broken.message
    );
}

#[tokio::test]
async fn a_second_module_providing_the_same_driver_is_refused() {
    let first = module_dir();
    let second = module_dir();
    let found = discover(&[first.path().to_path_buf(), second.path().to_path_buf()]).await;
    assert_eq!(found.modules.len(), 1);
    assert_eq!(
        found.modules[0].path,
        first.path().join("mythos-module-modbus")
    );
    assert_eq!(found.problems.len(), 1);
    assert!(found.problems[0].message.contains("already provided"));
}

#[tokio::test]
async fn the_runtime_reads_a_device_through_the_module_process() {
    let dir = module_dir();
    let registry = registry(dir.path()).await;
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(0, 1234).await;

    let segment = Segment::new();
    let config: IoConfig =
        serde_json::from_value(json!({"connections": [connection(sim.port())]})).unwrap();
    let catalog = Arc::new(RwLock::new(TagCatalog::new()));
    let status = IoStatusHandle::default();
    let runtime = IoRuntime::start(
        config,
        None,
        &registry,
        Arc::clone(&catalog),
        ShmWriterHandle::<PosixShm>::new(&segment.0, CAPACITY).unwrap(),
        status.clone(),
    )
    .await;

    let snapshot = status.snapshot();
    assert_eq!(
        snapshot.connections[0].state,
        ConnectionState::Running,
        "{:?}",
        snapshot.connections[0].detail
    );
    assert_eq!(
        module_children(&dir.path().join("mythos-module-modbus")).len(),
        1
    );

    let slot = wait_for_slot(&segment, |s| s.quality == QualityCode::GOOD, "first read").await;
    assert_eq!(slot.type_code, TagDataType::Uint16.code());
    assert_eq!(slot.value.as_u16(), 1234);

    sim.set_holding(0, 4321).await;
    wait_for_slot(&segment, |s| s.value.as_u16() == 4321, "a changed value").await;

    runtime.shutdown().await;
    let slot = wait_for_slot(&segment, |_| true, "after shutdown").await;
    assert_eq!(slot.quality, QualityCode::BAD_NOT_CONNECTED);
    // The module process is gone with the runtime.
    let deadline = Instant::now() + Duration::from_secs(5);
    while !module_children(&dir.path().join("mythos-module-modbus")).is_empty() {
        assert!(
            Instant::now() < deadline,
            "the module process outlived its runtime"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn a_module_process_that_dies_is_replaced_and_its_tag_is_never_left_good() {
    let dir = module_dir();
    let registry = registry(dir.path()).await;
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(0, 77).await;

    let segment = Segment::new();
    let config: IoConfig =
        serde_json::from_value(json!({"connections": [connection(sim.port())]})).unwrap();
    let catalog = Arc::new(RwLock::new(TagCatalog::new()));
    let runtime = IoRuntime::start(
        config,
        None,
        &registry,
        Arc::clone(&catalog),
        ShmWriterHandle::<PosixShm>::new(&segment.0, CAPACITY).unwrap(),
        IoStatusHandle::default(),
    )
    .await;

    wait_for_slot(&segment, |s| s.quality == QualityCode::GOOD, "first read").await;
    let before = module_children(&dir.path().join("mythos-module-modbus"));
    assert_eq!(before.len(), 1);

    // SIGKILL: the module gets no chance to say anything.
    let status = std::process::Command::new("kill")
        .args(["-9", &before[0].to_string()])
        .status()
        .unwrap();
    assert!(status.success());

    let bad = wait_for_slot(
        &segment,
        |s| s.quality == QualityCode::BAD_COMM_FAILURE,
        "after the module was killed",
    )
    .await;
    // The last value read stays, marked bad, with no number made up for the gap.
    assert_eq!(bad.value.as_u16(), 77);

    sim.set_holding(0, 78).await;
    let good = wait_for_slot(
        &segment,
        |s| s.quality == QualityCode::GOOD && s.value.as_u16() == 78,
        "after the module was replaced",
    )
    .await;
    assert_eq!(good.type_code, TagDataType::Uint16.code());

    let after = module_children(&dir.path().join("mythos-module-modbus"));
    assert_eq!(after.len(), 1, "exactly one module process runs again");
    assert_ne!(after, before, "a new process, not the killed one");

    runtime.shutdown().await;
}

#[tokio::test]
async fn checking_a_configuration_asks_the_module() {
    let dir = module_dir();
    let registry = registry(dir.path()).await;

    let mut bad = connection(502);
    bad["config"]["slave_id"] = json!(0);
    let config: IoConfig = serde_json::from_value(json!({"connections": [bad]})).unwrap();
    let problems = IoManager::<PosixShm>::check_with(&registry, &config).await;
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].message.contains("1..=247"),
        "{}",
        problems[0].message
    );

    let good: IoConfig = serde_json::from_value(json!({"connections": [connection(502)]})).unwrap();
    assert!(IoManager::<PosixShm>::check_with(&registry, &good)
        .await
        .is_empty());

    // Checking starts nothing that stays running.
    let deadline = Instant::now() + Duration::from_secs(5);
    while !module_children(&dir.path().join("mythos-module-modbus")).is_empty() {
        assert!(
            Instant::now() < deadline,
            "a check left a module process behind"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

// -----------------------------------------------------------------------------
// Replacing a module while it runs
// -----------------------------------------------------------------------------

struct ManagerRig {
    io: Arc<IoManager<PosixShm>>,
    segment: Segment,
    _storage: tempfile::TempDir,
}

/// A manager over one active project whose connection reads the simulator through the module
/// in `dir`, with field I/O running.
async fn manager_rig(dir: &Path, sim_port: u16) -> ManagerRig {
    use mythos_core::storage::{ControllerStorage, ProjectConfig};
    let storage_dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(storage_dir.path()).unwrap());
    storage
        .create_project(&ProjectConfig::new("plant", "Plant"))
        .unwrap();
    storage.activate_project("plant").unwrap();
    let io_file = storage.project_dir("plant").unwrap().join("io.json");
    std::fs::write(
        &io_file,
        serde_json::to_vec(&json!({"connections": [connection(sim_port)]})).unwrap(),
    )
    .unwrap();

    let segment = Segment::new();
    let dirs = vec![dir.to_path_buf()];
    let io = IoManager::<PosixShm>::with_module_dirs(
        storage,
        Arc::new(RwLock::new(TagCatalog::new())),
        segment.0.clone(),
        CAPACITY,
        DriverRegistry::discover(&dirs).await,
        dirs,
    );
    io.reload().await;
    ManagerRig {
        io,
        segment,
        _storage: storage_dir,
    }
}

/// Put `source` in place of the module, the way an installer does: write beside it, then
/// rename over it. The running process keeps the file it was started from.
fn install(dir: &Path, source: &Path) {
    std::thread::sleep(Duration::from_millis(20));
    let staged = dir.join(".staged-module");
    std::fs::copy(source, &staged).unwrap();
    std::fs::rename(&staged, dir.join("mythos-module-modbus")).unwrap();
}

/// Records every read of slot 0 that is not GOOD, until finished or dropped.
struct SlotWatch {
    stop: Arc<std::sync::atomic::AtomicBool>,
    handle: Option<tokio::task::JoinHandle<(u64, Vec<String>)>>,
}

impl Drop for SlotWatch {
    /// A test that panics must still stop the thread: the runtime waits for it on shutdown.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl SlotWatch {
    fn start(segment: &Segment) -> Self {
        let name = segment.0.clone();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let handle = tokio::task::spawn_blocking(move || {
            let mut reader = ShmReaderHandle::<PosixShm>::new(&name, CAPACITY).unwrap();
            let (mut reads, mut gaps) = (0u64, Vec::new());
            while !flag.load(Ordering::SeqCst) {
                reader.update();
                let quality = reader.read_tag(0).unwrap().quality;
                reads += 1;
                if quality != QualityCode::GOOD {
                    gaps.push(format!("{quality:?}"));
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            (reads, gaps)
        });
        Self {
            stop,
            handle: Some(handle),
        }
    }

    async fn finish(mut self) -> (u64, Vec<String>) {
        self.stop.store(true, Ordering::SeqCst);
        self.handle.take().unwrap().await.unwrap()
    }
}

#[tokio::test]
async fn installing_a_new_build_of_a_module_swaps_it_in_without_a_gap() {
    let dir = module_dir();
    let exe = dir.path().join("mythos-module-modbus");
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(0, 5).await;
    let rig = manager_rig(dir.path(), sim.port()).await;
    wait_for_slot(
        &rig.segment,
        |s| s.quality == QualityCode::GOOD,
        "first read",
    )
    .await;
    let before = module_children(&exe);
    assert_eq!(before.len(), 1);

    let _watcher = rig
        .io
        .spawn_module_watcher(Duration::from_millis(100))
        .unwrap();
    let watch = SlotWatch::start(&rig.segment);

    install(dir.path(), Path::new(UNDER_TEST));

    // The watcher notices the new file and swaps the connection to a process started from it.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let now = module_children(&exe);
        if now.len() == 1 && now != before {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the module was never swapped; children {now:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    sim.set_holding(0, 6).await;
    wait_for_slot(
        &rig.segment,
        |s| s.value.as_u16() == 6,
        "read by the new build",
    )
    .await;

    let (reads, gaps) = watch.finish().await;
    assert!(reads > 100, "the watch hardly ran: {reads}");
    assert!(
        gaps.is_empty(),
        "the tag stopped being GOOD during the swap: {gaps:?}"
    );

    let status = rig.io.status().snapshot();
    assert_eq!(status.connections[0].state, ConnectionState::Running);
    rig.io.shutdown().await;
}

#[tokio::test]
async fn a_new_build_that_cannot_start_leaves_the_running_one_in_place() {
    use std::os::unix::fs::PermissionsExt;
    let dir = module_dir();
    let exe = dir.path().join("mythos-module-modbus");
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(0, 9).await;
    let rig = manager_rig(dir.path(), sim.port()).await;
    wait_for_slot(
        &rig.segment,
        |s| s.quality == QualityCode::GOOD,
        "first read",
    )
    .await;
    let before = module_children(&exe);

    // A "new version" whose manifest is fine but which dies as soon as it is started.
    let output = std::process::Command::new(UNDER_TEST)
        .arg("--manifest")
        .output()
        .unwrap();
    let mut manifest: Value = serde_json::from_slice(&output.stdout).unwrap();
    manifest["version"] = json!("9.9.9");
    let broken = tempfile::tempdir().unwrap();
    let script = broken.path().join("module");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nif [ \"$1\" = --manifest ]; then\ncat <<'EOF'\n{manifest}\nEOF\nexit 0\nfi\nexit 1\n"
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

    let watch = SlotWatch::start(&rig.segment);
    install(dir.path(), &script);
    let report = rig.io.rescan().await;
    let (_, gaps) = watch.finish().await;

    let applied = report.applied.expect("field I/O is running");
    assert!(applied.swapped.is_empty(), "{applied:?}");
    assert_eq!(applied.failed.len(), 1, "{applied:?}");
    assert_eq!(applied.failed[0].0, "plc1");
    assert!(
        gaps.is_empty(),
        "the running connection was disturbed: {gaps:?}"
    );
    assert_eq!(
        module_children(&exe),
        before,
        "the running process was replaced"
    );

    let status = rig.io.status().snapshot();
    assert_eq!(status.connections[0].state, ConnectionState::Running);
    let detail = status.connections[0].detail.clone().unwrap_or_default();
    assert!(detail.contains("previous one is still running"), "{detail}");

    sim.set_holding(0, 10).await;
    wait_for_slot(&rig.segment, |s| s.value.as_u16() == 10, "still read").await;
    rig.io.shutdown().await;
}
