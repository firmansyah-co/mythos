//! Reading, checking, saving and applying a project's field I/O configuration over the API.
//!
//! Run against the real pieces: a simulated Modbus TCP device on a socket, a real
//! shared-memory segment, the I/O manager, and the router. The properties that matter:
//!
//! - saving the active project's configuration changes what the controller reads, with no
//!   restart, and a tag that is no longer configured stops existing;
//! - a configuration with a problem is refused whole, nothing is written, and what was running
//!   keeps running;
//! - a project that is not active can be configured without touching the running I/O.

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    Router,
};
use driver_modbus::sim::ModbusSimServer;
use futures_util::StreamExt;
use mythos_core::{
    api::{create_router, ApiState},
    io::{DriverRegistry, IoManager},
    storage::{ControllerStorage, ProjectConfig},
    tag::TagCatalog,
};
use mythos_pal::DefaultShm;
use mythos_shm::ShmReaderHandle;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tower::ServiceExt;

const CAPACITY: usize = 64;
static SEGMENT_SEQ: AtomicU32 = AtomicU32::new(0);

const PRESSURE: &str = "external.BoilerRoom.PT101.pressure";
const TEMPERATURE: &str = "external.BoilerRoom.TT101.temperature";
const LEVEL: &str = "external.Intake.LT201.level";

struct Rig {
    router: Router,
    storage: Arc<ControllerStorage>,
    catalog: Arc<RwLock<TagCatalog>>,
    io: Arc<IoManager<DefaultShm>>,
    segment: String,
    _dir: tempfile::TempDir,
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = DefaultShm::unlink(&self.segment);
    }
}

/// A server with two projects, `plant_a` active, and no I/O configuration anywhere.
async fn rig() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(dir.path()).unwrap());
    for (id, name) in [("plant_a", "Plant A"), ("plant_b", "Plant B")] {
        storage
            .create_project(&ProjectConfig::new(id, name))
            .unwrap();
    }
    storage.activate_project("plant_a").unwrap();

    let segment = format!(
        "mythos_io_config_test_{}_{}",
        std::process::id(),
        SEGMENT_SEQ.fetch_add(1, Ordering::SeqCst)
    );
    let _ = DefaultShm::unlink(&segment);

    let catalog = Arc::new(RwLock::new(TagCatalog::new()));
    let io = IoManager::<DefaultShm>::new(
        Arc::clone(&storage),
        Arc::clone(&catalog),
        segment.clone(),
        CAPACITY,
        modbus_registry(),
    );
    io.reload().await;

    let reader = ShmReaderHandle::<DefaultShm>::new(&segment, CAPACITY).unwrap();
    let state =
        ApiState::<DefaultShm>::new(Arc::clone(&storage), Arc::clone(&catalog), Some(reader))
            .with_io_manager(Arc::clone(&io));

    Rig {
        router: create_router(state),
        storage,
        catalog,
        io,
        segment,
        _dir: dir,
    }
}

impl Rig {
    async fn request(&self, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        let mut request = Request::builder().method(method).uri(uri);
        let body = match body {
            Some(json) => {
                request = request.header(header::CONTENT_TYPE, "application/json");
                Body::from(serde_json::to_vec(&json).unwrap())
            }
            None => Body::empty(),
        };
        let response = self
            .router
            .clone()
            .oneshot(request.body(body).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn put_io(&self, project: &str, config: Value) -> (StatusCode, Value) {
        self.request(
            "PUT",
            &format!("/api/v1/projects/{project}/io"),
            Some(config),
        )
        .await
    }

    fn io_file(&self, project: &str) -> std::path::PathBuf {
        self.storage.project_dir(project).unwrap().join("io.json")
    }

    async fn catalog_uris(&self) -> Vec<String> {
        let mut uris: Vec<String> = self
            .catalog
            .read()
            .await
            .iter()
            .map(|tag| tag.uri.clone())
            .collect();
        uris.sort();
        uris
    }

    /// Read the telemetry stream until `accept` is true for the update of `uri`.
    async fn wait_for(&self, uri: &str, accept: impl Fn(&Value) -> bool) -> Value {
        let response = self
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/telemetry/stream")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let mut body = response.into_body().into_data_stream();
        let mut last_seen: Option<Value> = None;

        let found = tokio::time::timeout(Duration::from_secs(8), async {
            let mut buffer = String::new();
            loop {
                let chunk = body.next().await.expect("telemetry stream ended").unwrap();
                buffer.push_str(&String::from_utf8_lossy(&chunk));
                while let Some(end) = buffer.find("\n\n") {
                    let event: String = buffer.drain(..end + 2).collect();
                    let Some(data) = event.lines().find_map(|l| l.strip_prefix("data: ")) else {
                        continue;
                    };
                    let Ok(batch) = serde_json::from_str::<Value>(data) else {
                        continue;
                    };
                    let Some(updates) = batch["updates"].as_array() else {
                        continue;
                    };
                    if let Some(update) = updates.iter().find(|u| u["uri"] == uri) {
                        if accept(update) {
                            return update.clone();
                        }
                        last_seen = Some(update.clone());
                    }
                }
            }
        })
        .await;

        found.unwrap_or_else(|_| panic!("no accepted update for {uri}; last seen: {last_seen:?}"))
    }

    /// The URIs present in one telemetry batch.
    async fn stream_uris(&self) -> Vec<String> {
        let response = self
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/telemetry/stream")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let mut body = response.into_body().into_data_stream();
        let mut buffer = String::new();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let chunk = body.next().await.expect("stream ended").unwrap();
                buffer.push_str(&String::from_utf8_lossy(&chunk));
                if let Some(end) = buffer.find("\n\n") {
                    let event = &buffer[..end];
                    if let Some(data) = event.lines().find_map(|l| l.strip_prefix("data: ")) {
                        let batch: Value = serde_json::from_str(data).unwrap();
                        let mut uris: Vec<String> = batch["updates"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|u| u["uri"].as_str().unwrap().to_string())
                            .collect();
                        uris.sort();
                        return uris;
                    }
                }
            }
        })
        .await
        .expect("no telemetry batch within the deadline")
    }
}

fn connection(id: &str, port: u16, mappings: Value) -> Value {
    json!({
        "id": id,
        "driver": "modbus",
        "poll_interval_ms": 50,
        "config": {
            "transport": "tcp",
            "host": "127.0.0.1",
            "port": port,
            "timeout_ms": 300,
            "mappings": mappings
        }
    })
}

fn holding(uri: &str, address: u16) -> Value {
    json!({"uri": uri, "address": address, "function_code": 3})
}

/// A port on which nothing is listening.
async fn dead_port() -> u16 {
    let probe = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    let port = probe.port();
    probe.stop();
    drop(probe);
    tokio::time::sleep(Duration::from_millis(50)).await;
    port
}

#[tokio::test]
async fn saving_the_active_projects_configuration_starts_reading_without_a_restart() {
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(0, 1234).await;
    let rig = rig().await;

    // Nothing is configured, and the status says so.
    let (_, status) = rig.request("GET", "/api/v1/io/status", None).await;
    assert_eq!(status["connections"], json!([]), "body: {status}");
    let (_, body) = rig
        .request("GET", "/api/v1/projects/plant_a/io", None)
        .await;
    assert_eq!(body, json!({ "config": null }));

    let config =
        json!({"connections": [connection("plc1", sim.port(), json!([holding(PRESSURE, 0)]))]});
    let (code, body) = rig.put_io("plant_a", config.clone()).await;
    assert_eq!(code, StatusCode::OK, "body: {body}");
    assert_eq!(body["applied"], json!(true), "body: {body}");

    // The device is now being read, by the same server process.
    let update = rig
        .wait_for(PRESSURE, |u| u["quality_name"] == "GOOD")
        .await;
    assert_eq!(update["value"], json!(1234), "body: {update}");

    // What was saved is what is read back, and it is on disk in the project.
    let (_, body) = rig
        .request("GET", "/api/v1/projects/plant_a/io", None)
        .await;
    assert_eq!(body["config"], config);
    assert!(rig.io_file("plant_a").exists());

    rig.io.shutdown().await;
}

#[tokio::test]
async fn replacing_the_configuration_removes_the_tags_it_no_longer_has() {
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(0, 11).await;
    sim.set_holding(1, 22).await;
    let rig = rig().await;

    let (code, _) = rig
        .put_io(
            "plant_a",
            json!({"connections": [connection("plc1", sim.port(), json!([
                holding(PRESSURE, 0), holding(TEMPERATURE, 1)
            ]))]}),
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    rig.wait_for(TEMPERATURE, |u| u["quality_name"] == "GOOD")
        .await;
    assert_eq!(rig.catalog_uris().await, vec![PRESSURE, TEMPERATURE]);

    // The new configuration drops TEMPERATURE and adds LEVEL.
    let (code, _) = rig
        .put_io(
            "plant_a",
            json!({"connections": [connection("plc1", sim.port(), json!([
                holding(PRESSURE, 0), holding(LEVEL, 1)
            ]))]}),
        )
        .await;
    assert_eq!(code, StatusCode::OK);

    let update = rig.wait_for(LEVEL, |u| u["quality_name"] == "GOOD").await;
    assert_eq!(update["value"], json!(22), "body: {update}");

    assert_eq!(rig.catalog_uris().await, vec![PRESSURE, LEVEL]);
    assert!(
        !rig.stream_uris().await.contains(&TEMPERATURE.to_string()),
        "a tag that is no longer configured must not be reported"
    );

    rig.io.shutdown().await;
}

#[tokio::test]
async fn a_configuration_with_a_problem_is_not_saved_and_what_was_running_keeps_running() {
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(0, 77).await;
    let rig = rig().await;

    let good =
        json!({"connections": [connection("plc1", sim.port(), json!([holding(PRESSURE, 0)]))]});
    let (code, _) = rig.put_io("plant_a", good).await;
    assert_eq!(code, StatusCode::OK);
    rig.wait_for(PRESSURE, |u| u["quality_name"] == "GOOD")
        .await;
    let saved = std::fs::read(rig.io_file("plant_a")).unwrap();

    let mut two_with_one_tag = connection("plc2", sim.port(), json!([holding(PRESSURE, 0)]));
    two_with_one_tag["poll_interval_ms"] = json!(100);

    // Each body is wrong in one way. `expect` is text the answer must contain.
    let cases: Vec<(&str, Value, &str)> = vec![
        (
            "a serial transport, which has no implementation",
            json!({"connections": [{
                "id": "serial1", "driver": "modbus", "poll_interval_ms": 100,
                "config": {"transport": "rtu", "serial_port": "/dev/ttyUSB0",
                           "mappings": [holding("external.Yard.RG1.rainfall", 0)]}
            }]}),
            "serial1",
        ),
        (
            "a driver that does not exist",
            json!({"connections": [{"id": "x", "driver": "profinet", "poll_interval_ms": 100, "config": {}}]}),
            "unknown driver 'profinet'",
        ),
        (
            "the same tag provided by two connections",
            json!({"connections": [
                connection("plc1", sim.port(), json!([holding(PRESSURE, 0)])),
                two_with_one_tag
            ]}),
            "already provided by connection 'plc1'",
        ),
        (
            "two connections with one id",
            json!({"connections": [
                connection("plc1", sim.port(), json!([holding(PRESSURE, 0)])),
                connection("plc1", sim.port(), json!([holding(TEMPERATURE, 1)]))
            ]}),
            "used more than once",
        ),
        (
            "a poll interval below the minimum",
            json!({"connections": [{
                "id": "fast", "driver": "modbus", "poll_interval_ms": 1,
                "config": {"transport": "tcp", "host": "127.0.0.1", "mappings": [holding(PRESSURE, 0)]}
            }]}),
            "below the minimum",
        ),
        (
            "a connection with no tags",
            json!({"connections": [connection("empty", sim.port(), json!([]))]}),
            "declares no tags",
        ),
        (
            "a tag outside the field namespace",
            json!({"connections": [connection("plc1", sim.port(), json!([holding("internal.Timer.t1.value", 0)]))]}),
            "plc1",
        ),
        (
            "a field the schema does not have",
            json!({"connections": [], "simulate": true}),
            "not an I/O configuration",
        ),
        (
            "a body that is not a configuration at all",
            json!([1, 2, 3]),
            "not an I/O configuration",
        ),
    ];

    for (what, body, expect) in cases {
        let (code, answer) = rig.put_io("plant_a", body).await;
        assert_eq!(code, StatusCode::BAD_REQUEST, "{what}: {answer}");
        assert!(
            answer.to_string().contains(expect),
            "{what}: the answer must name the problem ({expect:?}), got {answer}"
        );
        assert_eq!(
            std::fs::read(rig.io_file("plant_a")).unwrap(),
            saved,
            "{what}: the saved configuration must be untouched"
        );
    }

    // The runtime that was started before any of that is still reading the device.
    sim.set_holding(0, 78).await;
    let update = rig.wait_for(PRESSURE, |u| u["value"] == json!(78)).await;
    assert_eq!(update["quality_name"], "GOOD", "body: {update}");
    assert_eq!(rig.catalog_uris().await, vec![PRESSURE]);

    rig.io.shutdown().await;
}

#[tokio::test]
async fn a_project_that_is_not_active_is_saved_and_nothing_running_is_touched() {
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(0, 5).await;
    let rig = rig().await;

    let (code, _) = rig
        .put_io(
            "plant_a",
            json!({"connections": [connection("plc1", sim.port(), json!([holding(PRESSURE, 0)]))]}),
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    rig.wait_for(PRESSURE, |u| u["quality_name"] == "GOOD")
        .await;

    let other =
        json!({"connections": [connection("intake", sim.port(), json!([holding(LEVEL, 0)]))]});
    let (code, body) = rig.put_io("plant_b", other.clone()).await;
    assert_eq!(code, StatusCode::OK, "body: {body}");
    assert_eq!(body["applied"], json!(false), "body: {body}");

    let (_, body) = rig
        .request("GET", "/api/v1/projects/plant_b/io", None)
        .await;
    assert_eq!(body["config"], other);

    // Only the active project's tags exist.
    assert_eq!(rig.catalog_uris().await, vec![PRESSURE]);
    let (_, status) = rig.request("GET", "/api/v1/io/status", None).await;
    assert_eq!(status["connections"][0]["id"], "plc1", "body: {status}");

    rig.io.shutdown().await;
}

#[tokio::test]
async fn activating_another_project_switches_the_field_io_to_it() {
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(0, 5).await;
    sim.set_holding(1, 9).await;
    let rig = rig().await;

    rig.put_io(
        "plant_a",
        json!({"connections": [connection("plc1", sim.port(), json!([holding(PRESSURE, 0)]))]}),
    )
    .await;
    rig.put_io(
        "plant_b",
        json!({"connections": [connection("intake", sim.port(), json!([holding(LEVEL, 1)]))]}),
    )
    .await;
    rig.wait_for(PRESSURE, |u| u["quality_name"] == "GOOD")
        .await;

    let (code, _) = rig
        .request("POST", "/api/v1/projects/plant_b/activate", None)
        .await;
    assert_eq!(code, StatusCode::OK);

    let update = rig.wait_for(LEVEL, |u| u["quality_name"] == "GOOD").await;
    assert_eq!(update["value"], json!(9), "body: {update}");
    assert_eq!(rig.catalog_uris().await, vec![LEVEL]);
    assert_eq!(rig.stream_uris().await, vec![LEVEL]);

    let (_, status) = rig.request("GET", "/api/v1/io/status", None).await;
    assert_eq!(status["connections"][0]["id"], "intake", "body: {status}");
    assert!(
        status["source"].as_str().unwrap().contains("plant_b"),
        "body: {status}"
    );

    rig.io.shutdown().await;
}

#[tokio::test]
async fn a_slot_taken_over_by_a_new_tag_does_not_show_the_old_tags_value() {
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(0, 4242).await;
    let rig = rig().await;

    rig.put_io(
        "plant_a",
        json!({"connections": [connection("plc1", sim.port(), json!([holding(PRESSURE, 0)]))]}),
    )
    .await;
    rig.wait_for(PRESSURE, |u| u["value"] == json!(4242)).await;
    let slot = rig
        .catalog
        .read()
        .await
        .get_by_uri(PRESSURE)
        .unwrap()
        .slot_id;

    // The replacement has one tag, on a device that does not answer. It gets the freed slot.
    let port = dead_port().await;
    let (code, _) = rig
        .put_io(
            "plant_a",
            json!({"connections": [connection("gone", port, json!([holding(LEVEL, 0)]))]}),
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(
        rig.catalog.read().await.get_by_uri(LEVEL).unwrap().slot_id,
        slot,
        "the test is only meaningful if the new tag landed on the old tag's slot"
    );

    let update = rig.wait_for(LEVEL, |_| true).await;
    assert_eq!(update["value"], Value::Null, "body: {update}");
    assert_ne!(update["quality_name"], "GOOD", "body: {update}");

    rig.io.shutdown().await;
}

#[tokio::test]
async fn an_empty_configuration_stops_field_io_and_says_why() {
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(0, 1).await;
    let rig = rig().await;

    rig.put_io(
        "plant_a",
        json!({"connections": [connection("plc1", sim.port(), json!([holding(PRESSURE, 0)]))]}),
    )
    .await;
    rig.wait_for(PRESSURE, |u| u["quality_name"] == "GOOD")
        .await;

    let (code, body) = rig.put_io("plant_a", json!({"connections": []})).await;
    assert_eq!(code, StatusCode::OK, "body: {body}");

    assert!(rig.catalog_uris().await.is_empty());
    assert!(rig.stream_uris().await.is_empty());
    let (_, status) = rig.request("GET", "/api/v1/io/status", None).await;
    assert_eq!(status["connections"], json!([]), "body: {status}");
    assert!(
        status["note"].as_str().is_some_and(|n| !n.is_empty()),
        "body: {status}"
    );

    rig.io.shutdown().await;
}

#[tokio::test]
async fn the_routes_refuse_projects_that_do_not_exist_or_cannot_be_named() {
    let rig = rig().await;

    let (code, _) = rig
        .request("GET", "/api/v1/projects/no_such/io", None)
        .await;
    assert_eq!(code, StatusCode::NOT_FOUND);
    let (code, _) = rig.put_io("no_such", json!({"connections": []})).await;
    assert_eq!(code, StatusCode::NOT_FOUND);

    let (code, _) = rig.request("GET", "/api/v1/projects/..%2Fx/io", None).await;
    assert_eq!(code, StatusCode::BAD_REQUEST);
    let (code, _) = rig.put_io("..%2Fx", json!({"connections": []})).await;
    assert_eq!(code, StatusCode::BAD_REQUEST);

    let (code, drivers) = rig.request("GET", "/api/v1/io/drivers", None).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(drivers, json!(["modbus"]));
}

#[tokio::test]
async fn the_modules_route_lists_each_driver_and_where_it_comes_from() {
    let rig = rig().await;
    let (status, body) = rig.request("GET", "/api/v1/modules", None).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["problems"], json!([]));
    assert_eq!(
        body["drivers"],
        json!([{ "name": "modbus", "source": "in_process", "module": null }])
    );

    let (status, names) = rig.request("GET", "/api/v1/io/drivers", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(names, json!(["modbus"]));
}

/// The Modbus driver, in-process. A running server gets it as a module process instead; that
/// path is tested in `mythos-module-tests`.
fn modbus_registry() -> DriverRegistry {
    let mut registry = DriverRegistry::empty();
    registry.register("modbus", || Box::new(driver_modbus::ModbusDriver::new()));
    registry
}

/// Watches tag slots on the tag bus from another thread and records every moment one of them
/// is not GOOD, until stopped.
/// Reads made, and every (tag, quality) read that was not GOOD.
type GapLog = (u64, Vec<(String, String)>);

struct GapWatch {
    stop: Arc<std::sync::atomic::AtomicBool>,
    reads: Arc<std::sync::atomic::AtomicU64>,
    handle: Option<tokio::task::JoinHandle<GapLog>>,
}

impl Drop for GapWatch {
    /// A test that panics must still stop the thread: the runtime waits for it on shutdown.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl GapWatch {
    /// Start watching, and return once the first read is done: the watch covers everything
    /// after this call.
    async fn start(rig: &Rig, uris: &[&str]) -> Self {
        let catalog = rig.catalog.read().await;
        let slots: Vec<(String, usize)> = uris
            .iter()
            .map(|uri| {
                let meta = catalog
                    .get_by_uri(uri)
                    .unwrap_or_else(|| panic!("{uri} not registered"));
                (uri.to_string(), meta.slot_id as usize)
            })
            .collect();
        drop(catalog);
        let segment = rig.segment.clone();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reads = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let (flag, count) = (Arc::clone(&stop), Arc::clone(&reads));
        let handle = tokio::task::spawn_blocking(move || {
            let mut reader = ShmReaderHandle::<DefaultShm>::new(&segment, CAPACITY).unwrap();
            let mut gaps = Vec::new();
            while !flag.load(Ordering::SeqCst) {
                reader.update();
                for (uri, slot) in &slots {
                    let quality = reader.read_tag(*slot).unwrap().quality;
                    if quality != mythos_shm::QualityCode::GOOD {
                        gaps.push((uri.clone(), format!("{quality:?}")));
                    }
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
    async fn finish(mut self) -> GapLog {
        self.wait_for_reads(self.reads.load(Ordering::SeqCst) + 1)
            .await;
        self.stop.store(true, Ordering::SeqCst);
        self.handle.take().unwrap().await.unwrap()
    }
}

fn cycles_of(status: &Value, id: &str) -> u64 {
    status["connections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap_or_else(|| panic!("no connection {id} in {status}"))["cycles_completed"]
        .as_u64()
        .unwrap()
}

#[tokio::test]
async fn changing_one_connection_swaps_it_in_place_and_leaves_the_others_untouched() {
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(0, 10).await;
    sim.set_holding(1, 20).await;
    let rig = rig().await;

    let a = connection("plc_a", sim.port(), json!([holding(PRESSURE, 0)]));
    let b = connection("plc_b", sim.port(), json!([holding(LEVEL, 1)]));
    let (code, body) = rig
        .put_io("plant_a", json!({"connections": [a.clone(), b.clone()]}))
        .await;
    assert_eq!(code, StatusCode::OK, "body: {body}");
    rig.wait_for(PRESSURE, |u| u["quality_name"] == "GOOD")
        .await;
    rig.wait_for(LEVEL, |u| u["quality_name"] == "GOOD").await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let (_, before) = rig.request("GET", "/api/v1/io/status", None).await;

    // Same tags for plc_b, a different request timeout and poll interval.
    let mut b2 = b.clone();
    b2["poll_interval_ms"] = json!(60);
    b2["config"]["timeout_ms"] = json!(500);

    let watch = GapWatch::start(&rig, &[PRESSURE, LEVEL]).await;
    let (code, body) = rig.put_io("plant_a", json!({"connections": [a, b2]})).await;
    assert_eq!(code, StatusCode::OK, "body: {body}");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let (reads, gaps) = watch.finish().await;

    // What a reader of the tag bus saw comes first: the report is the runtime's own account.
    // The watch read from before the change was sent until after it was applied (see GapWatch).
    assert!(reads >= 2, "the watch did not run: {reads} reads");
    assert!(
        gaps.is_empty(),
        "a tag stopped being GOOD during the change: {gaps:?}"
    );
    assert_eq!(body["changes"]["kept"], json!(["plc_a"]), "body: {body}");
    assert_eq!(body["changes"]["swapped"], json!(["plc_b"]), "body: {body}");
    assert_eq!(body["changes"]["restarted"], json!([]), "body: {body}");

    let (_, after) = rig.request("GET", "/api/v1/io/status", None).await;
    // plc_a was not restarted: its cycle count carried on rather than starting again.
    assert!(
        cycles_of(&after, "plc_a") > cycles_of(&before, "plc_a"),
        "before {before}\nafter {after}"
    );
    let b_status = after["connections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "plc_b")
        .unwrap()
        .clone();
    assert_eq!(b_status["poll_interval_ms"], json!(60), "{after}");
    assert_eq!(b_status["state"], json!("RUNNING"), "{after}");

    rig.io.shutdown().await;
}

#[tokio::test]
async fn a_connection_whose_tags_change_is_restarted_alone() {
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    for (address, value) in [(0, 10), (1, 20), (2, 30)] {
        sim.set_holding(address, value).await;
    }
    let rig = rig().await;

    let a = connection("plc_a", sim.port(), json!([holding(PRESSURE, 0)]));
    let b = connection("plc_b", sim.port(), json!([holding(LEVEL, 1)]));
    rig.put_io("plant_a", json!({"connections": [a.clone(), b]}))
        .await;
    rig.wait_for(PRESSURE, |u| u["quality_name"] == "GOOD")
        .await;

    let b2 = connection(
        "plc_b",
        sim.port(),
        json!([holding(LEVEL, 1), holding(TEMPERATURE, 2)]),
    );
    let watch = GapWatch::start(&rig, &[PRESSURE]).await;
    let (code, body) = rig.put_io("plant_a", json!({"connections": [a, b2]})).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let (_, gaps) = watch.finish().await;

    assert_eq!(code, StatusCode::OK, "body: {body}");
    assert_eq!(body["changes"]["kept"], json!(["plc_a"]), "body: {body}");
    assert_eq!(
        body["changes"]["restarted"],
        json!(["plc_b"]),
        "body: {body}"
    );
    assert!(gaps.is_empty(), "plc_a was disturbed: {gaps:?}");
    rig.wait_for(TEMPERATURE, |u| u["quality_name"] == "GOOD")
        .await;
    assert_eq!(rig.catalog_uris().await, {
        let mut v = vec![
            PRESSURE.to_string(),
            LEVEL.to_string(),
            TEMPERATURE.to_string(),
        ];
        v.sort();
        v
    });

    rig.io.shutdown().await;
}
