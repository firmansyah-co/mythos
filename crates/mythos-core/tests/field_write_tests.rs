//! Writing to a field device: `PUT /api/v1/tags/:uri/value` through the running field I/O to a
//! real Modbus TCP device (a simulator on a loopback port), and every way it is refused.
//!
//! What an answer may claim is the point of these tests. `written: true` only when the device
//! confirmed; `false` only when nothing reached it; `"unknown"` when a request went out and no
//! confirmation came back. And a write never changes what a tag reads: only the next poll does.

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use driver_modbus::sim::ModbusSimServer;
use mythos_core::{
    api::{create_router, ApiState},
    io::{DriverRegistry, IoConfig, IoRuntime, IoStatusHandle, WriteError},
    storage::ControllerStorage,
    tag::TagCatalog,
};
use mythos_pal::DefaultShm;
use mythos_plugin_sdk::{
    async_trait, MythosPlugin, PluginContext, PluginError, PluginHealthReport, PluginMetadata,
    PluginType, ProtocolDriver, SampleValue, TagDeclaration, TagSample,
};
use mythos_shm::{ShmReaderHandle, ShmWriterHandle};
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
            "mythos_write_test_{}_{}",
            std::process::id(),
            SEGMENT_SEQ.fetch_add(1, Ordering::SeqCst)
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

struct Rig {
    router: Router,
    runtime: IoRuntime,
    status: IoStatusHandle,
    _segment: Segment,
    _storage_dir: tempfile::TempDir,
}

async fn rig_with(config: Value, registry: DriverRegistry) -> Rig {
    let segment = Segment::new();
    let storage_dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(storage_dir.path()).unwrap());
    let catalog = Arc::new(RwLock::new(TagCatalog::new()));
    let config: IoConfig = serde_json::from_value(config).expect("io config must parse");
    config.validate().expect("io config must validate");
    let writer = ShmWriterHandle::<DefaultShm>::new(&segment.0, CAPACITY).unwrap();
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
    let reader = ShmReaderHandle::<DefaultShm>::new(&segment.0, CAPACITY).unwrap();
    let state =
        ApiState::<DefaultShm>::new(storage, catalog, Some(reader)).with_io_status(status.clone());
    Rig {
        router: create_router(state),
        runtime,
        status,
        _segment: segment,
        _storage_dir: storage_dir,
    }
}

fn modbus_registry() -> DriverRegistry {
    let mut registry = DriverRegistry::empty();
    registry.register("modbus", || Box::new(driver_modbus::ModbusDriver::new()));
    registry
}

const SETPOINT: &str = "external.BoilerRoom.TIC101.setpoint";
const SIGNED: &str = "external.BoilerRoom.TIC101.offset";
const SCALED: &str = "external.BoilerRoom.TIC101.gain";
const MEASURED: &str = "external.BoilerRoom.TT101.temperature";
const WIDE: &str = "external.BoilerRoom.FQ101.total";

/// A device with a writable setpoint (holding 0), a signed one (holding 1), a scaled one
/// (holding 2, 0.1 per count), a measurement (input 0) and a two-register total (holding 10).
/// `poll_interval_ms` is how often the tags are read back.
async fn plant(sim: &ModbusSimServer, poll_interval_ms: u64) -> Rig {
    rig_with(
        json!({"connections": [{
            "id": "plc1",
            "driver": "modbus",
            "poll_interval_ms": poll_interval_ms,
            "config": {
                "transport": "tcp",
                "host": "127.0.0.1",
                "port": sim.port(),
                "timeout_ms": 300,
                "mappings": [
                    {"uri": SETPOINT, "address": 0, "function_code": 3},
                    {"uri": SIGNED, "address": 1, "function_code": 3, "data_type": "i16"},
                    {"uri": SCALED, "address": 2, "function_code": 3, "scale": 0.1},
                    {"uri": MEASURED, "address": 0, "function_code": 4},
                    {"uri": WIDE, "address": 10, "function_code": 3, "data_type": "u32"}
                ]
            }
        }]}),
        modbus_registry(),
    )
    .await
}

async fn put_value(router: &Router, uri: &str, body: Value) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/tags/{uri}/value"))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
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

/// The tag's entry in `GET /api/v1/tags/values`.
async fn reading(router: &Router, uri: &str) -> Value {
    let all = get_json(router, "/api/v1/tags/values").await;
    all["updates"]
        .as_array()
        .expect("tag values are a list of updates")
        .iter()
        .find(|t| t["uri"] == uri)
        .unwrap_or_else(|| panic!("no tag {uri} in {all}"))
        .clone()
}

/// Wait until the tag reads `value` with GOOD quality, or panic with what it read last.
async fn reads(router: &Router, uri: &str, value: Value) {
    let mut last = Value::Null;
    for _ in 0..200 {
        last = reading(router, uri).await;
        if last["quality_name"] == "GOOD" && last["value"] == value {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("{uri} never read {value}; last: {last}");
}

#[tokio::test]
async fn an_operator_writes_a_setpoint_the_device_takes_it_and_the_tag_reads_it_back() {
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(0, 10).await;
    let rig = plant(&sim, 50).await;
    reads(&rig.router, SETPOINT, json!(10)).await;

    let (status, body) = put_value(&rig.router, SETPOINT, json!({"value": 42})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!({"uri": SETPOINT, "written": true}));
    assert_eq!(
        sim.holding(0).await,
        Some(42),
        "the device holds what was written"
    );
    reads(&rig.router, SETPOINT, json!(42)).await;

    // A signed register takes a negative value; a scaled one is written in engineering units.
    let (status, body) = put_value(&rig.router, SIGNED, json!({"value": -7})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(sim.holding(1).await, Some((-7i16) as u16));
    let (status, body) = put_value(&rig.router, SCALED, json!({"value": 2.5})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(sim.holding(2).await, Some(25), "2.5 at 0.1 per count");

    rig.runtime.shutdown().await;
}

#[tokio::test]
async fn a_write_does_not_change_what_the_tag_reads_only_the_next_poll_does() {
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(0, 10).await;
    // Read every ten seconds: within this test, once.
    let rig = plant(&sim, 10_000).await;
    reads(&rig.router, SETPOINT, json!(10)).await;

    let (status, body) = put_value(&rig.router, SETPOINT, json!({"value": 42})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(sim.holding(0).await, Some(42));

    // The device holds 42 and nothing has read it since: the tag still says what was read.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let now = reading(&rig.router, SETPOINT).await;
    assert_eq!(
        now["value"],
        json!(10),
        "shown is what was read, not what was asked: {now}"
    );

    rig.runtime.shutdown().await;
}

#[tokio::test]
async fn a_tag_says_whether_it_can_be_written_and_a_read_only_one_is_refused_without_asking_the_device(
) {
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(0, 0).await;
    sim.set_holding(10, 1).await;
    sim.set_holding(11, 2).await;
    let rig = plant(&sim, 50).await;
    reads(&rig.router, SETPOINT, json!(0)).await;

    for (uri, mode) in [
        (SETPOINT, "READ_WRITE"),
        (SIGNED, "READ_WRITE"),
        (MEASURED, "READ_ONLY"),
        (WIDE, "READ_ONLY"),
    ] {
        let meta = get_json(&rig.router, &format!("/api/v1/tags/{uri}")).await;
        assert_eq!(meta["access_mode"], mode, "{uri}: {meta}");
    }

    // An input register cannot be written at all.
    let (status, body) = put_value(&rig.router, MEASURED, json!({"value": 1})).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["written"], json!(false), "{body}");
    assert!(
        body["error"].as_str().unwrap().contains("read-only"),
        "{body}"
    );

    // A value spanning two registers would need a multi-register write, which does not exist.
    let (status, body) = put_value(&rig.router, WIDE, json!({"value": 5})).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(sim.holding(10).await, Some(1), "untouched");
    assert_eq!(sim.holding(11).await, Some(2), "untouched");

    rig.runtime.shutdown().await;
}

#[tokio::test]
async fn a_value_that_does_not_fit_or_is_not_a_value_is_refused_and_nothing_is_written() {
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(0, 10).await;
    let rig = plant(&sim, 50).await;
    reads(&rig.router, SETPOINT, json!(10)).await;

    for value in [
        json!(70000),
        json!(-1),
        json!("42"),
        json!(null),
        json!([1]),
    ] {
        let (status, body) = put_value(&rig.router, SETPOINT, json!({ "value": value })).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{value}: {body}");
        assert_eq!(body["written"], json!(false), "{value}: {body}");
        assert!(
            body["error"]
                .as_str()
                .unwrap()
                .starts_with("nothing was written"),
            "{value}: {body}"
        );
    }
    // What is not a number or a boolean is refused here, whatever a driver would make of it.
    let (_, body) = put_value(&rig.router, SETPOINT, json!({"value": "42"})).await;
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("must be a number or a boolean"),
        "{body}"
    );
    assert_eq!(
        sim.holding(0).await,
        Some(10),
        "never wrapped, clamped or guessed"
    );

    rig.runtime.shutdown().await;
}

#[tokio::test]
async fn a_tag_nothing_provides_and_a_tag_of_another_device_are_refused() {
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(0, 0).await;
    let rig = plant(&sim, 50).await;
    reads(&rig.router, SETPOINT, json!(0)).await;

    let (status, body) = put_value(&rig.router, "external.Nowhere.X1.y", json!({"value": 1})).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["written"], json!(false), "{body}");

    // A tag read from another device of the plant (ADR-012 §2.6) is named with the device.
    let (status, body) = put_value(
        &rig.router,
        &format!("edge1:{SETPOINT}"),
        json!({"value": 1}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body["error"].as_str().unwrap().contains("another device"),
        "{body}"
    );
    assert_eq!(sim.holding(0).await, Some(0));

    rig.runtime.shutdown().await;
}

#[tokio::test]
async fn when_the_device_does_not_answer_the_write_is_reported_unknown_never_as_done_or_not_done() {
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(0, 10).await;
    let rig = plant(&sim, 10_000).await;
    reads(&rig.router, SETPOINT, json!(10)).await;

    sim.stop();
    drop(sim);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let (status, body) = put_value(&rig.router, SETPOINT, json!({"value": 42})).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    assert_eq!(body["written"], json!("unknown"), "{body}");
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("not known whether it took the value"),
        "{body}"
    );

    rig.runtime.shutdown().await;
}

#[tokio::test]
async fn once_its_connection_is_gone_a_tag_can_no_longer_be_written() {
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(0, 10).await;
    let mut rig = plant(&sim, 50).await;
    reads(&rig.router, SETPOINT, json!(10)).await;

    let registry = modbus_registry();
    rig.runtime
        .apply(
            IoConfig {
                connections: vec![],
            },
            None,
            &registry,
        )
        .await;

    let (status, body) = put_value(&rig.router, SETPOINT, json!({"value": 42})).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(sim.holding(0).await, Some(10));

    rig.runtime.shutdown().await;
}

// -----------------------------------------------------------------------------
// A driver that never answers a write
// -----------------------------------------------------------------------------

const STUCK_URI: &str = "external.Test.Stuck.value";
static WRITES_SEEN: AtomicU32 = AtomicU32::new(0);
static RESTARTED_AFTER_WRITE: AtomicBool = AtomicBool::new(false);

/// Reads fine; a write to it never returns.
struct NeverAnswersWrites;

#[async_trait]
impl MythosPlugin for NeverAnswersWrites {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "test.never-answers-writes".into(),
            name: "never answers a write".into(),
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
        if WRITES_SEEN.load(Ordering::SeqCst) > 0 {
            RESTARTED_AFTER_WRITE.store(true, Ordering::SeqCst);
        }
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
impl ProtocolDriver for NeverAnswersWrites {
    async fn describe_config(&self, _config: &Value) -> Result<Vec<TagDeclaration>, PluginError> {
        Ok(self.declared_tags())
    }
    fn declared_tags(&self) -> Vec<TagDeclaration> {
        vec![TagDeclaration {
            uri: STUCK_URI.into(),
            description: "test".into(),
            writable: true,
        }]
    }
    async fn poll_cycle(&mut self, _ctx: &PluginContext) -> Result<Vec<TagSample>, PluginError> {
        Ok(vec![TagSample::value(STUCK_URI, SampleValue::I32(7))])
    }
    async fn write_tag(
        &mut self,
        _ctx: &PluginContext,
        _tag_uri: &str,
        _value: &Value,
    ) -> Result<(), PluginError> {
        WRITES_SEEN.fetch_add(1, Ordering::SeqCst);
        std::future::pending::<()>().await;
        unreachable!()
    }
}

#[tokio::test]
async fn a_write_the_driver_never_answers_ends_at_the_deadline_as_unknown_and_reading_goes_on() {
    let mut registry = DriverRegistry::empty();
    registry.register("stuck", || Box::new(NeverAnswersWrites));
    let rig = rig_with(
        json!({"connections": [{
            "id": "stuck",
            "driver": "stuck",
            "poll_interval_ms": 50,
            "cycle_timeout_ms": 300,
            "config": {}
        }]}),
        registry,
    )
    .await;
    reads(&rig.router, STUCK_URI, json!(7)).await;

    let started = std::time::Instant::now();
    let answer = rig.status.write_tag(STUCK_URI, json!(1)).await;
    assert!(
        matches!(&answer, Err(WriteError::NotConfirmed(why)) if why.contains("300 ms")),
        "{answer:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "bounded by the deadline"
    );
    assert_eq!(
        WRITES_SEEN.load(Ordering::SeqCst),
        1,
        "the driver was asked once"
    );

    // The driver was restarted, as after a poll that overran, and the tag is read again.
    let before = rig.status.snapshot().connections[0].cycles_completed;
    for _ in 0..100 {
        if rig.status.snapshot().connections[0].cycles_completed > before + 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(
        rig.status.snapshot().connections[0].cycles_completed > before + 1,
        "polling continues after the write"
    );
    assert!(RESTARTED_AFTER_WRITE.load(Ordering::SeqCst));
    reads(&rig.router, STUCK_URI, json!(7)).await;

    rig.runtime.shutdown().await;
}
