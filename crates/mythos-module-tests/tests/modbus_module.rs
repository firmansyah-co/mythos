//! The Modbus driver as a module: a separate process, found by discovery, driven by the I/O
//! runtime, killed and replaced while the server keeps running.
//!
//! Everything here is real: the module executable, the Modbus TCP simulator on a socket, the
//! platform's shared-memory tag bus. It runs on Linux, macOS and Windows alike.

use driver_modbus::sim::ModbusSimServer;
use mythos_core::io::{
    ConnectionState, DriverRegistry, IoConfig, IoManager, IoRuntime, IoStatusHandle,
};
use mythos_core::tag::TagCatalog;
use mythos_module::host::RESPAWN_BACKOFF;
use mythos_module::{discover, ModuleKind, ProcessDriver};
use mythos_pal::DefaultShm;
use mythos_plugin_sdk::{MythosPlugin, PluginContext, ProtocolDriver};
use mythos_shm::{QualityCode, ShmReaderHandle, ShmWriterHandle, TagDataType, TagSlot};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

const UNDER_TEST: &str = env!("CARGO_BIN_EXE_modbus-module-under-test");
const FIXTURE: &str = env!("CARGO_BIN_EXE_module-fixture");
const CAPACITY: usize = 64;
const TAG: &str = "external.Plant.PT101.pressure";
static SEQ: AtomicU32 = AtomicU32::new(0);

/// The file name of the executable called `name` on this platform (`name.exe` on Windows).
fn exe_name(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

/// The installed Modbus module in `dir`.
fn modbus_exe(dir: &Path) -> PathBuf {
    dir.join(exe_name("mythos-module-modbus"))
}

/// Copy a file the way macOS and Windows do, keeping its modification time. Linux's copy gives
/// the copy a new one; doing it this way everywhere means every platform proves that a
/// reinstalled build is told apart by which file it is, not by its time.
fn copy_keeping_time(from: &Path, to: &Path) {
    std::fs::copy(from, to).unwrap();
    let modified = std::fs::metadata(from).unwrap().modified().unwrap();
    std::fs::File::options()
        .write(true)
        .open(to)
        .unwrap()
        .set_modified(modified)
        .unwrap();
}

/// A directory holding the module under its shipped name, as an installation would.
fn module_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    copy_keeping_time(Path::new(UNDER_TEST), &modbus_exe(dir.path()));
    dir
}

/// Put a module that behaves as told at `path`: it prints `manifest` (if any) and exits with
/// `manifest_exit` when asked for its manifest, and exits with `serve_exit` when started.
fn fixture(path: &Path, manifest: Option<&str>, manifest_exit: u8, serve_exit: u8) {
    std::fs::copy(FIXTURE, path).unwrap();
    let mut sidecar = path.as_os_str().to_owned();
    sidecar.push(".fixture.json");
    std::fs::write(
        sidecar,
        serde_json::to_vec(&json!({
            "manifest": manifest,
            "manifest_exit": manifest_exit,
            "serve_exit": serve_exit,
        }))
        .unwrap(),
    )
    .unwrap();
}

struct Segment(String);

impl Segment {
    fn new() -> Self {
        let name = format!(
            "mythos_module_test_{}_{}",
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
    let mut reader = ShmReaderHandle::<DefaultShm>::new(&segment.0, CAPACITY).unwrap();
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

/// This process's children running `<exe> --serve`. Tests run in parallel in one process, so
/// each test asks about the executable in its own directory.
///
/// **The scan runs off the test's thread.** It reads every process of the machine and takes
/// from a tenth of a second to more than one (measured at a load average of 1.2: 97 ms to
/// 1.5 s). `#[tokio::test]` gives a test one thread, and the simulated device, the I/O manager
/// and the host side of the module process all run on it: a scan made on that thread stops
/// them together. A poll already sent then gets no answer within the connection's 300 ms, the
/// tag is `BAD_COMM_FAILURE`, rightly, until the scan ends, and a swap that needs the thread
/// misses its 10 s. That is how `installing_a_new_build_of_a_module_swaps_it_in_without_a_gap`
/// failed twice on a loaded machine on 2026-11-22; the server itself runs on several threads
/// and its devices are not tasks of its own runtime.
async fn module_children(exe: &Path) -> Vec<u32> {
    module_children_after(exe, Duration::ZERO).await
}

/// [`module_children`], its scan made longer by `extra`: for the test that the scan leaves the
/// thread free.
async fn module_children_after(exe: &Path, extra: Duration) -> Vec<u32> {
    let exe = exe.to_path_buf();
    tokio::task::spawn_blocking(move || {
        std::thread::sleep(extra);
        scan_children(&exe)
    })
    .await
    .unwrap()
}

fn scan_children(exe: &Path) -> Vec<u32> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let me = sysinfo::Pid::from_u32(std::process::id());
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always),
    );
    let mut pids: Vec<u32> = system
        .processes()
        .values()
        .filter(|p| p.parent() == Some(me))
        .filter(|p| {
            // The command line as the host started it: the module's path, then `--serve`.
            let cmd = p.cmd();
            cmd.first().map(Path::new) == Some(exe)
                && cmd.get(1).and_then(|a| a.to_str()) == Some("--serve")
        })
        .map(|p| p.pid().as_u32())
        .collect();
    pids.sort();
    pids
}

/// Kill a process outright: SIGKILL on Unix, TerminateProcess on Windows. It gets no chance to
/// say anything.
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
async fn looking_for_a_module_s_processes_leaves_the_test_s_thread_free() {
    // What every test here relies on: while the processes are looked for, the one thread of the
    // test goes on running its other tasks (in a test, the device and the I/O manager). A task
    // that counts every 5 ms stands for them, and the scan is made 300 ms longer so that the
    // count does not depend on how fast this machine lists its processes. A scan made on the
    // thread itself leaves the count where it was.
    let ticks = Arc::new(AtomicU32::new(0));
    let counting = Arc::clone(&ticks);
    let ticker = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(5)).await;
            counting.fetch_add(1, Ordering::Relaxed);
        }
    });
    tokio::task::yield_now().await;
    let before = ticks.load(Ordering::Relaxed);
    let found =
        module_children_after(Path::new("no-such-module"), Duration::from_millis(300)).await;
    let during = ticks.load(Ordering::Relaxed) - before;
    ticker.abort();
    assert!(found.is_empty(), "{found:?}");
    assert!(
        during >= 10,
        "the thread ran {during} of about 60 ticks while the processes were looked for"
    );
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
    assert_eq!(module.path, modbus_exe(dir.path()));
    assert_eq!(
        module.manifest.config_schema["properties"]["mappings"]["type"],
        "array"
    );

    let registry = registry(dir.path()).await;
    assert_eq!(registry.names(), vec!["modbus".to_string()]);
    assert_eq!(registry.drivers()[0].source, "module");
    // What GET /api/v1/modules writes. A fingerprint used to fail here, so the route panicked.
    let listed = serde_json::to_value(registry.drivers()).expect("the module list serialises");
    assert_eq!(listed[0]["module"]["manifest"]["provides"], "modbus");
}

#[tokio::test]
async fn files_that_are_not_usable_modules_are_reported_or_ignored() {
    let dir = module_dir();
    let path = |name: &str| dir.path().join(name);
    fixture(
        &path(&exe_name("mythos-module-broken")),
        Some("not-a-manifest\n"),
        0,
        1,
    );
    fixture(&path(&exe_name("mythos-module-failing")), None, 3, 1);
    fixture(&path(&exe_name("unrelated-tool")), None, 0, 0);
    std::fs::write(path("mythos-module-modbus.d"), "dep-info\n").unwrap();
    // A file with a module's name that cannot be started: on Unix it lacks the execute bit; on
    // Windows it is not an .exe.
    let not_executable = path("mythos-module-not-executable");
    std::fs::write(&not_executable, "not a program\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&not_executable, std::fs::Permissions::from_mode(0o644)).unwrap();
    }

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
        vec![
            exe_name("mythos-module-broken"),
            exe_name("mythos-module-failing")
        ]
    );
    let broken = found
        .problems
        .iter()
        .find(|p| p.path.ends_with(exe_name("mythos-module-broken")))
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
    assert_eq!(found.modules[0].path, modbus_exe(first.path()));
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
        ShmWriterHandle::<DefaultShm>::new(&segment.0, CAPACITY).unwrap(),
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
    assert_eq!(module_children(&modbus_exe(dir.path())).await.len(), 1);

    let slot = wait_for_slot(&segment, |s| s.quality == QualityCode::GOOD, "first read").await;
    assert_eq!(slot.type_code, TagDataType::Uint16.code());
    assert_eq!(slot.value.as_u16(), 1234);

    sim.set_holding(0, 4321).await;
    wait_for_slot(&segment, |s| s.value.as_u16() == 4321, "a changed value").await;

    // And writes it through the same process: the module says the tag is writable (so the
    // catalog does), the write crosses the protocol, the device holds the value, and the
    // next poll reads it back.
    assert_eq!(
        catalog.read().await.get_by_uri(TAG).unwrap().access_mode,
        mythos_core::tag::AccessMode::ReadWrite,
        "what the module declared arrived"
    );
    status
        .write_tag(TAG, json!(555))
        .await
        .expect("the device confirms");
    assert_eq!(sim.holding(0).await, Some(555));
    wait_for_slot(
        &segment,
        |s| s.value.as_u16() == 555,
        "the written value read back",
    )
    .await;
    let refused = status.write_tag(TAG, json!(70000)).await;
    assert!(
        matches!(&refused, Err(mythos_core::io::WriteError::Refused(why)) if why.contains("does not fit")),
        "a refusal crosses the protocol as a refusal: {refused:?}"
    );
    assert_eq!(sim.holding(0).await, Some(555));

    runtime.shutdown().await;
    assert_eq!(
        status.write_tag(TAG, json!(1)).await,
        Err(mythos_core::io::WriteError::UnknownTag),
        "nothing can be written once the runtime is gone"
    );
    let slot = wait_for_slot(&segment, |_| true, "after shutdown").await;
    assert_eq!(slot.quality, QualityCode::BAD_NOT_CONNECTED);
    // The module process is gone with the runtime.
    let deadline = Instant::now() + Duration::from_secs(5);
    while !module_children(&modbus_exe(dir.path())).await.is_empty() {
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
        ShmWriterHandle::<DefaultShm>::new(&segment.0, CAPACITY).unwrap(),
        IoStatusHandle::default(),
    )
    .await;

    wait_for_slot(&segment, |s| s.quality == QualityCode::GOOD, "first read").await;
    let before = module_children(&modbus_exe(dir.path())).await;
    assert_eq!(before.len(), 1);

    // Past the respawn backoff, so the replacement may be started at once: the case of
    // `a_poll_that_finds_its_module_process_dead_fails_and_the_next_one_replaces_it`, seen here
    // on Windows in CI (PR #128).
    tokio::time::sleep(RESPAWN_BACKOFF + Duration::from_millis(200)).await;
    kill(before[0]);

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

    let after = module_children(&modbus_exe(dir.path())).await;
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
    let problems = IoManager::<DefaultShm>::check_with(&registry, &config).await;
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].message.contains("1..=247"),
        "{}",
        problems[0].message
    );

    let good: IoConfig = serde_json::from_value(json!({"connections": [connection(502)]})).unwrap();
    assert!(IoManager::<DefaultShm>::check_with(&registry, &good)
        .await
        .is_empty());

    // Checking starts nothing that stays running.
    let deadline = Instant::now() + Duration::from_secs(5);
    while !module_children(&modbus_exe(dir.path())).await.is_empty() {
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
    io: Arc<IoManager<DefaultShm>>,
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
    let io = IoManager::<DefaultShm>::with_module_dirs(
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

/// Put `source` in place of the module, the way an installer does: write beside it, move the
/// old build out of the way, then move the new one in. The running process keeps the file it was
/// started from.
///
/// Moving the old build away first is what Windows requires (a running executable can be
/// renamed but not replaced), and it leaves the module missing for a moment, which the watcher
/// must ride out. It is done the same way everywhere, so every platform proves that.
async fn install(dir: &Path, source: &Path) {
    static OLD: AtomicU32 = AtomicU32::new(0);
    tokio::time::sleep(Duration::from_millis(20)).await;
    let target = modbus_exe(dir);
    let staged = dir.join(".staged-module");
    copy_keeping_time(source, &staged);
    let mut sidecar = source.as_os_str().to_owned();
    sidecar.push(".fixture.json");
    if Path::new(&sidecar).exists() {
        let mut beside = target.as_os_str().to_owned();
        beside.push(".fixture.json");
        std::fs::copy(&sidecar, beside).unwrap();
    }
    let old = dir.join(format!(
        "replaced-{}.old",
        OLD.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::rename(&target, old).unwrap();
    // A slow installer: the module stays missing for longer than the watcher needs to notice a
    // change and let it settle, so the watcher certainly acts on its absence. The connection
    // reading through the module must ride that out untouched.
    tokio::time::sleep(WATCH_EVERY * 5).await;
    std::fs::rename(&staged, &target).unwrap();
}

/// How often the watcher looks in the tests that run one.
const WATCH_EVERY: Duration = Duration::from_millis(100);

/// Records every read of slot 0 that is not GOOD, until finished or dropped.
struct SlotWatch {
    stop: Arc<std::sync::atomic::AtomicBool>,
    reads: Arc<std::sync::atomic::AtomicU64>,
    handle: Option<tokio::task::JoinHandle<(u64, Vec<String>)>>,
}

impl Drop for SlotWatch {
    /// A test that panics must still stop the thread: the runtime waits for it on shutdown.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl SlotWatch {
    /// Start watching, and return once the first read is done: the watch covers everything
    /// after this call.
    async fn start(segment: &Segment) -> Self {
        let name = segment.0.clone();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reads = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let (flag, count) = (Arc::clone(&stop), Arc::clone(&reads));
        let handle = tokio::task::spawn_blocking(move || {
            let mut reader = ShmReaderHandle::<DefaultShm>::new(&name, CAPACITY).unwrap();
            let mut gaps = Vec::new();
            while !flag.load(Ordering::SeqCst) {
                reader.update();
                let quality = reader.read_tag(0).unwrap().quality;
                if quality != QualityCode::GOOD {
                    gaps.push(format!("{quality:?}"));
                }
                count.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(1));
            }
            (count.load(Ordering::SeqCst), gaps)
        });
        let watch = Self {
            stop,
            reads,
            handle: Some(handle),
        };
        watch.wait_for_reads(1).await;
        watch
    }

    async fn wait_for_reads(&self, at_least: u64) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.reads.load(Ordering::SeqCst) < at_least {
            assert!(Instant::now() < deadline, "the watch stopped reading");
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }

    /// Stop once the watch has read again after this call, so it covers everything before it.
    async fn finish(mut self) -> (u64, Vec<String>) {
        self.wait_for_reads(self.reads.load(Ordering::SeqCst) + 1)
            .await;
        self.stop.store(true, Ordering::SeqCst);
        self.handle.take().unwrap().await.unwrap()
    }
}

#[tokio::test]
async fn installing_a_new_build_of_a_module_swaps_it_in_without_a_gap() {
    a_new_build_is_swapped_in_without_a_gap(Duration::ZERO, Some(Duration::from_millis(20))).await;
}

/// The same, on a machine that takes 400 ms to list its processes and with no pause between
/// two looks: what a loaded machine did to this test on 2026-11-22. With the scan on the test's
/// own thread the device could not answer a poll already sent, and the tag went bad for as long
/// as the scan took (thousands of reads). The swap has no gap however long the test takes to
/// look.
#[tokio::test]
async fn a_new_build_is_swapped_in_without_a_gap_while_the_test_is_slow_to_look_for_its_processes()
{
    a_new_build_is_swapped_in_without_a_gap(Duration::from_millis(400), None).await;
}

/// Installs a new build over a running module and requires the swap without a gap. Each look for
/// the module's processes takes `scan_takes` longer, and `pause` passes between two looks (none:
/// the next look starts as soon as the other tasks have had one turn).
async fn a_new_build_is_swapped_in_without_a_gap(scan_takes: Duration, pause: Option<Duration>) {
    let dir = module_dir();
    let exe = modbus_exe(dir.path());
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(0, 5).await;
    let rig = manager_rig(dir.path(), sim.port()).await;
    wait_for_slot(
        &rig.segment,
        |s| s.quality == QualityCode::GOOD,
        "first read",
    )
    .await;
    let before = module_children(&exe).await;
    assert_eq!(before.len(), 1);

    let _watcher = rig.io.spawn_module_watcher(WATCH_EVERY).unwrap();
    let watch = SlotWatch::start(&rig.segment).await;

    install(dir.path(), Path::new(UNDER_TEST)).await;

    // The watcher notices the new file and swaps the connection to a process started from it.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let now = module_children_after(&exe, scan_takes).await;
        if now.len() == 1 && now != before {
            break;
        }
        if Instant::now() >= deadline {
            let status = rig.io.status().snapshot();
            panic!(
                "the module was never swapped; children {now:?}; connection {:?}: {:?}; \
                 drivers {:?}; module problems {:?}; installed {:?}",
                status.connections.first().map(|c| &c.state),
                status.connections.first().and_then(|c| c.detail.clone()),
                rig.io.drivers(),
                rig.io.module_problems(),
                mythos_module::snapshot(&[dir.path().to_path_buf()]),
            );
        }
        match pause {
            Some(pause) => tokio::time::sleep(pause).await,
            None => tokio::task::yield_now().await,
        }
    }
    sim.set_holding(0, 6).await;
    wait_for_slot(
        &rig.segment,
        |s| s.value.as_u16() == 6,
        "read by the new build",
    )
    .await;

    let (reads, gaps) = watch.finish().await;
    // The watch read from before the install until after the swap (see SlotWatch).
    assert!(reads >= 2, "the watch did not run: {reads}");
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
    let dir = module_dir();
    let exe = modbus_exe(dir.path());
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(0, 9).await;
    let rig = manager_rig(dir.path(), sim.port()).await;
    wait_for_slot(
        &rig.segment,
        |s| s.quality == QualityCode::GOOD,
        "first read",
    )
    .await;
    let before = module_children(&exe).await;

    // A "new version" whose manifest is fine but which dies as soon as it is started.
    let output = std::process::Command::new(UNDER_TEST)
        .arg("--manifest")
        .output()
        .unwrap();
    let mut manifest: Value = serde_json::from_slice(&output.stdout).unwrap();
    manifest["version"] = json!("9.9.9");
    let broken = tempfile::tempdir().unwrap();
    let script = broken.path().join(exe_name("module"));
    fixture(&script, Some(&manifest.to_string()), 0, 1);

    let watch = SlotWatch::start(&rig.segment).await;
    install(dir.path(), &script).await;
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
        module_children(&exe).await,
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

/// The host found its module process dead at the start of a poll, past the respawn backoff. It
/// used to start the replacement inside that poll and answer it, so the tags went from the dead
/// process's last reading to the new one's, GOOD throughout, however long the restart took. A
/// death in the middle of a poll failed it either way, which is why this is driven here, with
/// the process gone before the poll begins.
#[tokio::test]
async fn a_poll_that_finds_its_module_process_dead_fails_and_the_next_one_replaces_it() {
    let dir = module_dir();
    let module = discover(&[dir.path().to_path_buf()])
        .await
        .modules
        .into_iter()
        .next()
        .expect("the Modbus module is found");
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(0, 77).await;

    // As the field I/O runtime grants it (`DRIVER_CAPABILITY` in `mythos_core::io`).
    let ctx = PluginContext::new(
        "io.plc1",
        std::iter::once("network:client".to_string()).collect(),
        String::new(),
    );
    let mut driver = ProcessDriver::new(Arc::new(module));
    driver.init(&ctx).await.unwrap();
    driver
        .start(&ctx, &connection(sim.port())["config"])
        .await
        .unwrap();
    driver.poll_cycle(&ctx).await.expect("the first poll reads");

    tokio::time::sleep(RESPAWN_BACKOFF + Duration::from_millis(200)).await;
    let pid = driver.pid().expect("a module process runs");
    kill(pid);
    // Gone before the next poll starts.
    tokio::time::sleep(Duration::from_millis(500)).await;

    let found_dead = driver.poll_cycle(&ctx).await;
    let why = found_dead
        .expect_err("the poll that finds the process dead fails, rather than answer for a new one")
        .to_string();
    assert!(why.contains("it will be started again"), "{why}");
    assert_eq!(driver.restarts(), 0, "nothing was started yet");

    let replaced = driver
        .poll_cycle(&ctx)
        .await
        .expect("the next poll starts a new process and reads");
    assert_eq!(replaced.len(), 1);
    assert_eq!(driver.restarts(), 1);
    assert_ne!(driver.pid(), Some(pid), "a new process, not the killed one");
    driver.stop(&ctx).await.unwrap();
}
