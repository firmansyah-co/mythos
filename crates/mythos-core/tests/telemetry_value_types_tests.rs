//! Telemetry serialisation must honour each slot's declared type.
//!
//! The defect this covers (D2): the SSE stream read every slot through `as_f64` regardless of
//! `type_code`. `from_i32`/`from_u16`/`from_bool` store an integer bit pattern, so reading those
//! bytes as `f64` produced a denormal — a DINT holding 42 surfaced as ~1e-322. It was not
//! fabricated data (the bytes were real) but the operator-visible result was the same class of
//! wrong: a number that is not the plant's value that looks like a broken instrument.
//!
//! The test drives the real handler through the real router and reads the SSE body, so it exercises
//! the actual wire path a UI consumer uses rather than a helper in isolation.

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
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tower::ServiceExt;

/// OPC-style `type_code` values, matching the slot layout's documentation.
const TYPE_BOOL: u16 = 1;
const TYPE_INT: u16 = 2;
const TYPE_DINT: u16 = 3;
const TYPE_REAL: u16 = 4;

#[tokio::test]
async fn telemetry_serialises_bool_and_integer_tags_with_their_true_values() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(dir.path()).unwrap());

    let mut catalog = TagCatalog::new();
    for (uri, slot) in [
        ("external.plant.p101.running", 0u32),
        ("external.plant.p101.count", 1),
        ("external.plant.p101.ticks", 2),
        ("external.plant.p101.pressure", 3),
        // A tag whose writer never declared a type. This must be reported as unspecified, not
        // guessed at as a float.
        ("external.plant.p101.undeclared", 4),
    ] {
        catalog
            .register(TagMetadata::new(uri, slot).unwrap())
            .unwrap();
    }
    let tag_catalog = Arc::new(RwLock::new(catalog));

    let shm_name = format!("mythos_telemetry_types_{}", std::process::id());
    #[cfg(target_os = "linux")]
    let _ = mythos_pal::PosixShm::unlink(&shm_name);
    let mut writer = ShmWriterHandle::<mythos_pal::DefaultShm>::new(&shm_name, 64).unwrap();
    let reader = mythos_shm::ShmReaderHandle::<mythos_pal::DefaultShm>::new(&shm_name, 64).unwrap();

    writer
        .write_tag(
            0,
            TYPE_BOOL,
            QualityCode::GOOD,
            TagValueRaw::from_bool(true),
            1_000,
            TagFlags::ACCESS_RO,
        )
        .unwrap();
    writer
        .write_tag(
            1,
            TYPE_INT,
            QualityCode::GOOD,
            TagValueRaw::from_i16(-1234),
            2_000,
            TagFlags::ACCESS_RO,
        )
        .unwrap();
    writer
        .write_tag(
            2,
            TYPE_DINT,
            QualityCode::GOOD,
            TagValueRaw::from_i32(42),
            3_000,
            TagFlags::ACCESS_RO,
        )
        .unwrap();
    writer
        .write_tag(
            3,
            TYPE_REAL,
            QualityCode::GOOD,
            TagValueRaw::from_f32(98.6),
            4_000,
            TagFlags::ACCESS_RO,
        )
        .unwrap();
    writer
        .write_tag(
            4,
            0, // no declared type
            QualityCode::GOOD,
            TagValueRaw::from_i32(7),
            5_000,
            TagFlags::ACCESS_RO,
        )
        .unwrap();
    writer.publish();

    // NOTE: DefaultShm (PosixShm) rather than MemoryShm, deliberately. MemoryShm::open_or_create
    // allocates a fresh isolated buffer on every call, so a writer and reader "on the same name"
    // never share memory - a test built on it silently measures nothing. This test asserts real
    // values, so it needs a real segment.
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

    // Read one tag_update event off the live stream.
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
    let updates = batch["updates"]
        .as_array()
        .expect("updates must be an array");

    let find = |uri: &str| {
        updates
            .iter()
            .find(|u| u["uri"] == uri)
            .unwrap_or_else(|| panic!("no update for {uri} in {payload}"))
            .clone()
    };

    let bool_tag = find("external.plant.p101.running");
    assert_eq!(bool_tag["type"], "BOOL");
    assert_eq!(
        bool_tag["value"],
        json!(true),
        "a BOOL tag must surface as a JSON boolean, not a number"
    );

    let int_tag = find("external.plant.p101.count");
    assert_eq!(int_tag["type"], "INT");
    assert_eq!(
        int_tag["value"],
        json!(-1234),
        "an INT tag must surface its value; the defect rendered it as a denormal"
    );

    let dint_tag = find("external.plant.p101.ticks");
    assert_eq!(dint_tag["type"], "DINT");
    assert_eq!(
        dint_tag["value"],
        json!(42),
        "a DINT holding 42 must not surface as ~1e-322"
    );

    let real_tag = find("external.plant.p101.pressure");
    assert_eq!(real_tag["type"], "REAL");
    let real_value = real_tag["value"].as_f64().unwrap();
    assert!(
        (real_value - 98.6).abs() < 1e-4,
        "a REAL tag must surface ~98.6, got {real_value}"
    );

    let undeclared = find("external.plant.p101.undeclared");
    assert_eq!(
        undeclared["type"], "UNSPECIFIED",
        "a slot with no type code must be labelled, not guessed"
    );
    assert_eq!(
        undeclared["value"],
        Value::Null,
        "an undeclared type must report null rather than a fabricated float"
    );
}
