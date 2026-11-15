//! The read path, end to end: a field device, the driver, the tag bus, the telemetry stream.
//!
//! Every test runs the real pieces against each other: a Modbus TCP device simulator on a real
//! socket, the real driver, a real POSIX shared-memory segment, and the SSE body read through
//! the real router, as a browser reads it. `MemoryShm` is not used: it allocates an isolated
//! buffer per open, so a writer and a reader "on the same name" would never share memory.
//!
//! The properties asserted are the ones an operator depends on:
//!
//! - a value read from the device arrives with its type and a GOOD quality;
//! - a value that can no longer be read is never presented as current;
//! - a tag that was never read has no value at all, rather than a zero.

#![cfg(target_os = "linux")]

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use driver_modbus::sim::ModbusSimServer;
use futures_util::StreamExt;
use mythos_core::{
    api::{create_router, ApiState},
    io::{ConnectionState, DriverRegistry, IoConfig, IoRuntime, IoStatusHandle},
    storage::ControllerStorage,
    tag::TagCatalog,
};
use mythos_pal::PosixShm;
use mythos_plugin_sdk::{
    async_trait, MythosPlugin, PluginContext, PluginError, PluginHealthReport, PluginMetadata,
    PluginType, ProtocolDriver, SampleValue, TagDeclaration, TagSample,
};
use mythos_shm::{QualityCode, ShmReaderHandle, ShmWriterHandle, TagValueRaw};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tower::ServiceExt;

const CAPACITY: usize = 64;
static SEGMENT_SEQ: AtomicU32 = AtomicU32::new(0);

/// A shared-memory segment that is unique to one test and removed when the test ends.
struct Segment(String);

impl Segment {
    fn new() -> Self {
        let name = format!(
            "mythos_io_test_{}_{}",
            std::process::id(),
            SEGMENT_SEQ.fetch_add(1, Ordering::SeqCst)
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

struct Rig {
    router: Router,
    runtime: IoRuntime,
    status: IoStatusHandle,
    catalog: Arc<RwLock<TagCatalog>>,
    segment: Segment,
    _storage_dir: tempfile::TempDir,
}

async fn rig_with(config: Value, registry: DriverRegistry, segment: Segment) -> Rig {
    let storage_dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(storage_dir.path()).unwrap());
    let catalog = Arc::new(RwLock::new(TagCatalog::new()));

    let config: IoConfig = serde_json::from_value(config).expect("io config must parse");
    config.validate().expect("io config must validate");

    let writer = ShmWriterHandle::<PosixShm>::new(&segment.0, CAPACITY).unwrap();
    let status = IoStatusHandle::default();
    let runtime = IoRuntime::start(
        config,
        None,
        &registry,
        Arc::clone(&catalog),
        writer,
        status.clone(),
    )
    .await;

    let reader = ShmReaderHandle::<PosixShm>::new(&segment.0, CAPACITY).unwrap();
    let state = ApiState::<PosixShm>::new(storage, Arc::clone(&catalog), Some(reader))
        .with_io_status(status.clone());

    Rig {
        router: create_router(state),
        runtime,
        status,
        catalog,
        segment,
        _storage_dir: storage_dir,
    }
}

async fn rig(config: Value) -> Rig {
    rig_with(config, modbus_registry(), Segment::new()).await
}

/// Read the telemetry stream until `accept` returns true for the update of `uri`, and return
/// that update. Panics with the last update seen if it never does.
async fn wait_for_update(router: &Router, uri: &str, accept: impl Fn(&Value) -> bool) -> Value {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/telemetry/stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let mut body = response.into_body().into_data_stream();
    let mut last_seen: Option<Value> = None;

    let found = tokio::time::timeout(Duration::from_secs(8), async {
        let mut buffer = String::new();
        loop {
            let chunk = match body.next().await {
                Some(Ok(chunk)) => chunk,
                Some(Err(e)) => panic!("telemetry stream errored: {e}"),
                None => panic!("telemetry stream ended"),
            };
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(end) = buffer.find("\n\n") {
                let event: String = buffer.drain(..end + 2).collect();
                let Some(data) = event.lines().find_map(|line| line.strip_prefix("data: ")) else {
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

    match found {
        Ok(update) => update,
        Err(_) => {
            panic!("no accepted update for {uri} within the deadline; last seen: {last_seen:?}")
        }
    }
}

async fn get_json(router: &Router, path: &str) -> Value {
    let response = router
        .clone()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK, "GET {path}");
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn modbus_connection(port: u16, mappings: Value) -> Value {
    json!({
        "id": "plc1",
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

/// A port on which nothing is listening.
async fn dead_port() -> u16 {
    let probe = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    let port = probe.port();
    probe.stop();
    drop(probe);
    tokio::time::sleep(Duration::from_millis(50)).await;
    port
}

const PRESSURE: &str = "external.BoilerRoom.PT101.pressure";
const TEMPERATURE: &str = "external.BoilerRoom.TT101.temperature";

#[tokio::test]
async fn a_device_value_reaches_the_stream_follows_the_device_and_goes_bad_when_the_link_drops() {
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(0, 1234).await;
    let bits = 21.5f32.to_bits();
    sim.set_holding(2, (bits >> 16) as u16).await;
    sim.set_holding(3, (bits & 0xFFFF) as u16).await;

    let rig = rig(json!({"connections": [modbus_connection(sim.port(), json!([
        {"uri": PRESSURE, "address": 0, "function_code": 3},
        {"uri": TEMPERATURE, "address": 2, "function_code": 3, "data_type": "f32"}
    ]))]}))
    .await;

    // 1. The value the device holds arrives, typed, with a GOOD quality.
    let update = wait_for_update(&rig.router, PRESSURE, |u| u["quality_name"] == "GOOD").await;
    assert_eq!(update["value"], json!(1234), "body: {update}");
    assert_eq!(update["type"], "UINT", "body: {update}");
    assert_eq!(update["quality"], "GOOD", "body: {update}");
    assert!(
        update["timestamp_ns"].as_u64().unwrap() > 0,
        "a read value carries the time it was read: {update}"
    );

    let update = wait_for_update(&rig.router, TEMPERATURE, |u| u["quality_name"] == "GOOD").await;
    assert_eq!(update["value"], json!(21.5), "body: {update}");
    assert_eq!(update["type"], "REAL", "body: {update}");

    // 2. The stream follows the device, so it is the device being read and not a constant.
    sim.set_holding(0, 4321).await;
    let update = wait_for_update(&rig.router, PRESSURE, |u| u["value"] == json!(4321)).await;
    assert_eq!(update["quality_name"], "GOOD", "body: {update}");
    let read_at = update["timestamp_ns"].as_u64().unwrap();

    // 3. The link drops. The last value read may still be shown, but never as current.
    sim.stop();
    let update = wait_for_update(&rig.router, PRESSURE, |u| u["quality_name"] != "GOOD").await;
    assert_eq!(update["quality_name"], "BAD_COMM_FAILURE", "body: {update}");
    assert_eq!(update["quality"], "BAD", "body: {update}");
    assert_eq!(
        update["value"],
        json!(4321),
        "the last value read is kept, not replaced: {update}"
    );
    assert!(
        update["timestamp_ns"].as_u64().unwrap() <= read_at + 1_000_000_000
            && update["timestamp_ns"].as_u64().unwrap() >= read_at,
        "the timestamp stays at the last successful read instead of advancing: {update}"
    );

    // 4. The status endpoint names the fault.
    let status = get_json(&rig.router, "/api/v1/io/status").await;
    let connection = &status["connections"][0];
    assert_eq!(connection["state"], "RUNNING", "body: {status}");
    assert_eq!(connection["tags_good"], json!(0), "body: {status}");
    let faults = connection["faults"].as_array().unwrap();
    assert_eq!(faults.len(), 2, "body: {status}");
    assert!(
        faults[0]["reason"]
            .as_str()
            .unwrap()
            .contains("communication failure"),
        "body: {status}"
    );

    rig.runtime.shutdown().await;
}

#[tokio::test]
async fn a_tag_that_was_never_read_has_no_type_and_no_value() {
    let port = dead_port().await;
    let rig = rig(json!({"connections": [modbus_connection(port, json!([
        {"uri": PRESSURE, "address": 0, "function_code": 3}
    ]))]}))
    .await;

    // The tag exists from the moment the connection starts...
    let tags = get_json(&rig.router, "/api/v1/tags").await;
    assert!(
        tags.as_array()
            .unwrap()
            .iter()
            .any(|t| t["uri"] == PRESSURE),
        "body: {tags}"
    );

    // ...and once a poll has failed it says why, with nothing where a value would be.
    let update = wait_for_update(&rig.router, PRESSURE, |u| {
        u["quality_name"] == "BAD_COMM_FAILURE"
    })
    .await;
    assert_eq!(update["value"], Value::Null, "body: {update}");
    assert_eq!(update["type"], "UNSPECIFIED", "body: {update}");
    assert_eq!(update["timestamp_ns"], json!(0), "body: {update}");

    rig.runtime.shutdown().await;
}

#[tokio::test]
async fn data_left_in_the_segment_by_an_earlier_process_is_never_served() {
    let segment = Segment::new();

    // An earlier process wrote a GOOD value into the slot this runtime is about to use, and
    // left it there. Every bank is filled, because which one a reader lands on is not ours to
    // choose.
    {
        let mut stale = ShmWriterHandle::<PosixShm>::new(&segment.0, CAPACITY).unwrap();
        for _ in 0..3 {
            stale
                .write_tag(
                    0,
                    mythos_shm::TagDataType::Uint16.code(),
                    QualityCode::GOOD,
                    TagValueRaw::from_u16(999),
                    1,
                    0,
                )
                .unwrap();
            stale.publish();
        }
    }

    let port = dead_port().await;
    let rig = rig_with(
        json!({"connections": [modbus_connection(port, json!([
            {"uri": PRESSURE, "address": 0, "function_code": 3}
        ]))]}),
        modbus_registry(),
        segment,
    )
    .await;

    assert_eq!(
        rig.catalog
            .read()
            .await
            .get_by_uri(PRESSURE)
            .unwrap()
            .slot_id,
        0,
        "the test is only meaningful if the tag landed on the slot holding stale data"
    );

    // Whatever the first update is, it is not the leftover.
    let update = wait_for_update(&rig.router, PRESSURE, |_| true).await;
    assert_ne!(update["quality_name"], "GOOD", "body: {update}");
    assert_eq!(update["value"], Value::Null, "body: {update}");

    rig.runtime.shutdown().await;
}

#[tokio::test]
async fn a_rejected_connection_contributes_no_tags_and_the_others_still_run() {
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(0, 7).await;

    let rig = rig(json!({"connections": [
        {
            "id": "serial1",
            "driver": "modbus",
            "poll_interval_ms": 50,
            "config": {
                "transport": "rtu",
                "serial_port": "/dev/ttyUSB0",
                "mappings": [{"uri": "external.Yard.RG1.rainfall", "address": 0, "function_code": 3}]
            }
        },
        {
            "id": "nodriver",
            "driver": "profinet",
            "poll_interval_ms": 50,
            "config": {}
        },
        modbus_connection(sim.port(), json!([
            {"uri": PRESSURE, "address": 0, "function_code": 3}
        ]))
    ]}))
    .await;

    let snapshot = rig.status.snapshot();
    assert_eq!(snapshot.connections.len(), 3);

    assert_eq!(snapshot.connections[0].state, ConnectionState::ConfigError);
    assert!(
        snapshot.connections[0]
            .detail
            .as_deref()
            .unwrap()
            .contains("RTU"),
        "detail: {:?}",
        snapshot.connections[0].detail
    );
    assert!(snapshot.connections[0].tags.is_empty());

    assert_eq!(snapshot.connections[1].state, ConnectionState::ConfigError);
    assert!(
        snapshot.connections[1]
            .detail
            .as_deref()
            .unwrap()
            .contains("unknown driver 'profinet'"),
        "detail: {:?}",
        snapshot.connections[1].detail
    );

    assert_eq!(snapshot.connections[2].state, ConnectionState::Running);

    // The rejected connection's tag does not exist anywhere a reader could find it.
    let catalog = rig.catalog.read().await;
    assert!(catalog.get_by_uri("external.Yard.RG1.rainfall").is_none());
    assert_eq!(catalog.len(), 1);
    drop(catalog);

    let update = wait_for_update(&rig.router, PRESSURE, |u| u["quality_name"] == "GOOD").await;
    assert_eq!(update["value"], json!(7), "body: {update}");

    rig.runtime.shutdown().await;
}

#[tokio::test]
async fn two_connections_cannot_both_provide_the_same_tag() {
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(0, 1).await;

    let mut second = modbus_connection(
        sim.port(),
        json!([
            {"uri": PRESSURE, "address": 0, "function_code": 3}
        ]),
    );
    second["id"] = json!("plc2");

    let rig = rig(json!({"connections": [
        modbus_connection(sim.port(), json!([
            {"uri": PRESSURE, "address": 0, "function_code": 3}
        ])),
        second
    ]}))
    .await;

    let snapshot = rig.status.snapshot();
    assert_eq!(snapshot.connections[0].state, ConnectionState::Running);
    assert_eq!(snapshot.connections[1].state, ConnectionState::ConfigError);
    assert!(
        snapshot.connections[1]
            .detail
            .as_deref()
            .unwrap()
            .contains("already provided"),
        "detail: {:?}",
        snapshot.connections[1].detail
    );
    assert_eq!(rig.catalog.read().await.len(), 1);

    rig.runtime.shutdown().await;
}

// -----------------------------------------------------------------------------
// A driver that reads once and then misbehaves
// -----------------------------------------------------------------------------

const HANG_URI: &str = "external.Plant.Stuck.value";

/// What the scripted driver does once its test tells it to misbehave.
#[derive(Clone, Copy)]
enum Misbehaviour {
    /// Never returns from a poll.
    Hang,
    /// Returns successfully with no sample for its declared tag.
    ReportNothing,
}

/// Reads a constant until `misbehave` is set, then does `then`.
///
/// The switch is flipped by the test rather than by a poll count, so the test decides when the
/// healthy period ends instead of racing a timer to observe it.
struct ScriptedDriver {
    misbehave: &'static AtomicBool,
    then: Misbehaviour,
}

static HANG_NOW: AtomicBool = AtomicBool::new(false);
static GO_SILENT_NOW: AtomicBool = AtomicBool::new(false);

#[async_trait]
impl MythosPlugin for ScriptedDriver {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "test.scripted".into(),
            name: "reads once, then misbehaves".into(),
            version: "0".into(),
            author: "test".into(),
            description: String::new(),
            plugin_type: PluginType::ProtocolDriver,
            capabilities: vec![],
        }
    }
    async fn init(&mut self, _ctx: &PluginContext) -> Result<(), PluginError> {
        Ok(())
    }
    async fn start(&mut self, _ctx: &PluginContext, _config: &Value) -> Result<(), PluginError> {
        Ok(())
    }
    async fn stop(&mut self, _ctx: &PluginContext) -> Result<(), PluginError> {
        Ok(())
    }
    async fn health_check(&self) -> Result<PluginHealthReport, PluginError> {
        Ok(PluginHealthReport::healthy("test"))
    }
}

#[async_trait]
impl ProtocolDriver for ScriptedDriver {
    async fn describe_config(&self, _config: &Value) -> Result<Vec<TagDeclaration>, PluginError> {
        Ok(self.declared_tags())
    }
    fn declared_tags(&self) -> Vec<TagDeclaration> {
        vec![TagDeclaration {
            uri: HANG_URI.into(),
            description: "test".into(),
        }]
    }
    async fn poll_cycle(&mut self, _ctx: &PluginContext) -> Result<Vec<TagSample>, PluginError> {
        if !self.misbehave.load(Ordering::SeqCst) {
            return Ok(vec![TagSample::value(HANG_URI, SampleValue::I32(-55))]);
        }
        match self.then {
            Misbehaviour::Hang => {
                std::future::pending::<()>().await;
                unreachable!()
            }
            Misbehaviour::ReportNothing => Ok(Vec::new()),
        }
    }
    async fn write_tag(
        &mut self,
        _ctx: &PluginContext,
        _tag_uri: &str,
        _value: &Value,
    ) -> Result<(), PluginError> {
        Err(PluginError::RuntimeError("not writable".into()))
    }
}

#[tokio::test]
async fn a_driver_that_stops_answering_is_marked_failed_at_its_deadline() {
    let mut registry = DriverRegistry::empty();
    registry.register("hangs", || {
        Box::new(ScriptedDriver {
            misbehave: &HANG_NOW,
            then: Misbehaviour::Hang,
        })
    });

    let rig = rig_with(
        json!({"connections": [{
            "id": "stuck",
            "driver": "hangs",
            "poll_interval_ms": 50,
            "cycle_timeout_ms": 300,
            "config": {}
        }]}),
        registry,
        Segment::new(),
    )
    .await;

    let update = wait_for_update(&rig.router, HANG_URI, |u| u["quality_name"] == "GOOD").await;
    assert_eq!(update["value"], json!(-55), "body: {update}");
    assert_eq!(update["type"], "DINT", "body: {update}");

    // From here the driver never returns. Without a deadline the value would stay GOOD forever.
    HANG_NOW.store(true, Ordering::SeqCst);
    let update = wait_for_update(&rig.router, HANG_URI, |u| u["quality_name"] != "GOOD").await;
    assert_eq!(update["quality_name"], "BAD_COMM_FAILURE", "body: {update}");

    let snapshot = rig.status.snapshot();
    assert!(
        snapshot.connections[0]
            .last_cycle_error
            .as_deref()
            .unwrap_or("")
            .contains("deadline"),
        "status: {:?}",
        snapshot.connections[0]
    );

    rig.runtime.shutdown().await;
}

#[tokio::test]
async fn a_tag_the_driver_stops_reporting_on_is_not_left_good() {
    let mut registry = DriverRegistry::empty();
    registry.register("silent", || {
        Box::new(ScriptedDriver {
            misbehave: &GO_SILENT_NOW,
            then: Misbehaviour::ReportNothing,
        })
    });

    let rig = rig_with(
        json!({"connections": [{
            "id": "silent",
            "driver": "silent",
            "poll_interval_ms": 50,
            "config": {}
        }]}),
        registry,
        Segment::new(),
    )
    .await;

    let update = wait_for_update(&rig.router, HANG_URI, |u| u["quality_name"] == "GOOD").await;
    assert_eq!(update["value"], json!(-55), "body: {update}");

    // From here every cycle succeeds and says nothing about the tag. Silence is not "unchanged".
    GO_SILENT_NOW.store(true, Ordering::SeqCst);
    let update = wait_for_update(&rig.router, HANG_URI, |u| u["quality_name"] != "GOOD").await;
    assert_eq!(update["quality_name"], "BAD_COMM_FAILURE", "body: {update}");

    let snapshot = rig.status.snapshot();
    assert_eq!(snapshot.connections[0].last_cycle_error, None);
    assert!(
        snapshot.connections[0].faults[0]
            .reason
            .contains("no sample"),
        "status: {:?}",
        snapshot.connections[0]
    );

    rig.runtime.shutdown().await;
}

#[tokio::test]
async fn shutting_down_leaves_no_tag_reading_as_good() {
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(0, 42).await;

    let rig = rig(json!({"connections": [modbus_connection(sim.port(), json!([
        {"uri": PRESSURE, "address": 0, "function_code": 3}
    ]))]}))
    .await;

    wait_for_update(&rig.router, PRESSURE, |u| u["quality_name"] == "GOOD").await;

    let segment_name = rig.segment.0.clone();
    rig.runtime.shutdown().await;

    // Read the segment the way the next process to attach would.
    let mut reader = ShmReaderHandle::<PosixShm>::new(&segment_name, CAPACITY).unwrap();
    reader.update();
    let slot = reader.read_tag(0).unwrap();
    assert_eq!(slot.quality, QualityCode::BAD_NOT_CONNECTED);

    let snapshot = rig.status.snapshot();
    assert_eq!(snapshot.connections[0].state, ConnectionState::Stopped);
}

#[tokio::test]
async fn a_server_with_no_io_runtime_says_so() {
    let storage_dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(storage_dir.path()).unwrap());
    let catalog = Arc::new(RwLock::new(TagCatalog::new()));
    let state = ApiState::<PosixShm>::new(storage, catalog, None);
    let router = create_router(state);

    let status = get_json(&router, "/api/v1/io/status").await;
    assert_eq!(status["connections"], json!([]), "body: {status}");
    assert!(
        status["note"].as_str().is_some_and(|n| !n.is_empty()),
        "an idle runtime must state why it is idle: {status}"
    );
}

/// The Modbus driver, in-process. A running server gets it as a module process instead; that
/// path is tested in `mythos-module-tests`.
fn modbus_registry() -> DriverRegistry {
    let mut registry = DriverRegistry::empty();
    registry.register("modbus", || Box::new(driver_modbus::ModbusDriver::new()));
    registry
}
