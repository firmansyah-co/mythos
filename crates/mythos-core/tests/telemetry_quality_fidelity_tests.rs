//! The telemetry stream must carry the *specific* quality, not just its class.
//!
//! `build_telemetry_event` reduced the quality word to the string `GOOD`/`BAD`/`UNCERTAIN` before
//! the value left the process. `BAD_COMM_FAILURE` (0x0018) and `BAD_SENSOR_FAILURE` (0x0010) both
//! became `"BAD"`, so the distinction was destroyed at the source — where no amount of UI work can
//! recover it. TSK-801's criterion ("map QualityCode to the faceplate quality state") was
//! therefore unmeetable by frontend effort, and a slice sized as UI work would have been sized
//! wrong.
//!
//! # End to end, not at either end
//!
//! The test drives the real handler through the real router and reads the SSE body, as the
//! UI consumer does, and the quality words come from independent writes into a real POSIX segment
//! rather than from a helper in front of the serialiser. Asserting the mapping function's output
//! would only prove the function; asserting the parsed stream after a genuine slot write proves
//! the distinction survived from the slot to the wire.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use futures_util::StreamExt;
use mythos_core::{
    api::{create_router, ApiState},
    storage::ControllerStorage,
    tag::{TagCatalog, TagMetadata},
};
use mythos_shm::{QualityCode, ShmWriterHandle, TagFlags, TagValueRaw};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tower::ServiceExt;

const TYPE_REAL: u16 = 4;

/// Two tags whose qualities share the class `BAD` but mean different things to an operator.
const COMM_FAIL_URI: &str = "external.plant.p101.flow";
const SENSOR_FAIL_URI: &str = "external.plant.p101.pressure";

#[tokio::test]
async fn the_wire_distinguishes_a_comm_failure_from_a_sensor_failure() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(dir.path()).unwrap());

    let mut catalog = TagCatalog::new();
    for (uri, slot) in [(COMM_FAIL_URI, 0u32), (SENSOR_FAIL_URI, 1)] {
        catalog
            .register(TagMetadata::new(uri, slot).unwrap())
            .unwrap();
    }
    let tag_catalog = Arc::new(RwLock::new(catalog));

    let shm_name = format!("mythos_telemetry_quality_{}", std::process::id());
    #[cfg(target_os = "linux")]
    let _ = mythos_pal::PosixShm::unlink(&shm_name);
    // DefaultShm (PosixShm), deliberately: MemoryShm::open_or_create allocates a fresh isolated
    // buffer per call, so a writer and reader "on the same name" never share memory and a test
    // built on it silently measures nothing.
    let mut writer = ShmWriterHandle::<mythos_pal::DefaultShm>::new(&shm_name, 64).unwrap();

    writer
        .write_tag(
            0,
            TYPE_REAL,
            QualityCode::BAD_COMM_FAILURE,
            TagValueRaw::from_f32(1.0),
            1_000,
            TagFlags::ACCESS_RO,
        )
        .unwrap();
    writer
        .write_tag(
            1,
            TYPE_REAL,
            QualityCode::BAD_SENSOR_FAILURE,
            TagValueRaw::from_f32(2.0),
            2_000,
            TagFlags::ACCESS_RO,
        )
        .unwrap();
    writer.publish();

    let reader = mythos_shm::ShmReaderHandle::<mythos_pal::DefaultShm>::new(&shm_name, 64).unwrap();
    let state =
        ApiState::<mythos_pal::DefaultShm>::new(storage.clone(), tag_catalog.clone(), Some(reader));
    let app = create_router(state);

    let response = app
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
    let payload = tokio::time::timeout(Duration::from_secs(5), async {
        let mut buffer = String::new();
        loop {
            let chunk = match body.next().await {
                Some(Ok(chunk)) => chunk,
                Some(Err(e)) => panic!("telemetry stream errored: {e}"),
                None => return None,
            };
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            if let Some(start) = buffer.find("data: ") {
                if let Some(end) = buffer[start..].find("\n\n") {
                    return Some(buffer[start + 6..start + end].to_owned());
                }
            }
        }
    })
    .await
    .expect("timed out waiting for a telemetry event")
    .expect("stream produced no parseable data line");

    let batch: Value = serde_json::from_str(&payload).expect("telemetry payload must be JSON");
    let updates = batch["updates"].as_array().expect("updates array");
    let find = |uri: &str| {
        updates
            .iter()
            .find(|u| u["uri"] == uri)
            .unwrap_or_else(|| panic!("no update for {uri} in {payload}"))
            .clone()
    };

    let comm = find(COMM_FAIL_URI);
    let sensor = find(SENSOR_FAIL_URI);

    // The class is preserved for existing consumers...
    assert_eq!(comm["quality"], "BAD", "body: {comm}");
    assert_eq!(sensor["quality"], "BAD", "body: {sensor}");

    // ...and the two are still told apart, which is the property that was missing.
    assert_eq!(
        comm["quality_code"],
        Value::from(QualityCode::BAD_COMM_FAILURE.code())
    );
    assert_eq!(comm["quality_name"], "BAD_COMM_FAILURE");
    assert_eq!(
        sensor["quality_code"],
        Value::from(QualityCode::BAD_SENSOR_FAILURE.code())
    );
    assert_eq!(sensor["quality_name"], "BAD_SENSOR_FAILURE");
    assert_ne!(
        comm["quality_code"], sensor["quality_code"],
        "a faceplate cannot distinguish these on `quality` alone, so the code must differ"
    );
    assert_ne!(comm["quality_name"], sensor["quality_name"]);

    // The raw word and its name cannot disagree: both derive from the same QualityCode.
    for update in [&comm, &sensor] {
        let code = update["quality_code"].as_u64().unwrap() as u16;
        let named = update["quality_name"].as_str().unwrap();
        assert_eq!(
            QualityCode(code).as_str(),
            named,
            "quality_name must be the name of quality_code, not an independently produced string"
        );
    }

    let _ = mythos_pal::PosixShm::unlink(&shm_name);
}

#[tokio::test]
async fn a_good_quality_slot_still_reports_good() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(dir.path()).unwrap());

    let mut catalog = TagCatalog::new();
    catalog
        .register(TagMetadata::new(COMM_FAIL_URI, 0).unwrap())
        .unwrap();
    let tag_catalog = Arc::new(RwLock::new(catalog));

    let shm_name = format!("mythos_telemetry_good_{}", std::process::id());
    #[cfg(target_os = "linux")]
    let _ = mythos_pal::PosixShm::unlink(&shm_name);
    let mut writer = ShmWriterHandle::<mythos_pal::DefaultShm>::new(&shm_name, 64).unwrap();
    writer
        .write_tag(
            0,
            TYPE_REAL,
            QualityCode::GOOD,
            TagValueRaw::from_f32(3.0),
            1_000,
            TagFlags::ACCESS_RO,
        )
        .unwrap();
    writer.publish();

    let reader = mythos_shm::ShmReaderHandle::<mythos_pal::DefaultShm>::new(&shm_name, 64).unwrap();
    let state =
        ApiState::<mythos_pal::DefaultShm>::new(storage.clone(), tag_catalog.clone(), Some(reader));
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/telemetry/stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let mut body = response.into_body().into_data_stream();
    let payload = tokio::time::timeout(Duration::from_secs(5), async {
        let mut buffer = String::new();
        loop {
            let chunk = match body.next().await {
                Some(Ok(chunk)) => chunk,
                Some(Err(e)) => panic!("telemetry stream errored: {e}"),
                None => return None,
            };
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            if let Some(start) = buffer.find("data: ") {
                if let Some(end) = buffer[start..].find("\n\n") {
                    return Some(buffer[start + 6..start + end].to_owned());
                }
            }
        }
    })
    .await
    .expect("timed out")
    .expect("no data line");

    let batch: Value = serde_json::from_str(&payload).unwrap();
    let update = batch["updates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["uri"] == COMM_FAIL_URI)
        .expect("no update")
        .clone();

    assert_eq!(update["quality"], "GOOD");
    assert_eq!(update["quality_name"], "GOOD");
    assert_eq!(
        update["quality_code"],
        Value::from(QualityCode::GOOD.code())
    );

    let _ = mythos_pal::PosixShm::unlink(&shm_name);
}
