//! What the tag routes may say about a tag, and who may create one.

#![cfg(target_os = "linux")]

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    Router,
};
use futures_util::StreamExt;
use mythos_core::{
    api::{create_router, ApiState},
    storage::ControllerStorage,
    tag::{AccessMode, TagCatalog, TagMetadata},
};
use mythos_pal::PosixShm;
use mythos_shm::{QualityCode, ShmReaderHandle, ShmWriterHandle, TagDataType, TagValueRaw};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tower::ServiceExt;

const CAPACITY: usize = 64;

struct Segment(String);

impl Segment {
    fn new(label: &str) -> Self {
        let name = format!("mythos_tag_api_{}_{}", label, std::process::id());
        let _ = PosixShm::unlink(&name);
        Self(name)
    }
}

impl Drop for Segment {
    fn drop(&mut self) {
        let _ = PosixShm::unlink(&self.0);
    }
}

async fn send(
    router: &Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(json) => {
            request = request.header(header::CONTENT_TYPE, "application/json");
            Body::from(serde_json::to_vec(&json).unwrap())
        }
        None => Body::empty(),
    };
    let response = router
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

/// The updates in one telemetry batch.
async fn one_batch(router: &Router) -> Vec<Value> {
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
    let mut body = response.into_body().into_data_stream();
    let mut buffer = String::new();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let chunk = body.next().await.expect("stream ended").unwrap();
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            if let Some(end) = buffer.find("\n\n") {
                if let Some(data) = buffer[..end].lines().find_map(|l| l.strip_prefix("data: ")) {
                    let batch: Value = serde_json::from_str(data).unwrap();
                    return batch["updates"].as_array().unwrap().clone();
                }
            }
        }
    })
    .await
    .expect("no telemetry batch within the deadline")
}

#[tokio::test]
async fn a_tag_is_described_only_by_what_a_source_stated() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(dir.path()).unwrap());
    let mut catalog = TagCatalog::new();
    catalog
        .register(
            TagMetadata::new("external.BoilerRoom.PT101.pressure", 3)
                .unwrap()
                .with_description("Modbus TCP 192.0.2.10:502 unit 1 holding register 100")
                .with_access(AccessMode::ReadOnly),
        )
        .unwrap();
    let state =
        ApiState::<mythos_pal::MemoryShm>::new(storage, Arc::new(RwLock::new(catalog)), None);
    let router = create_router(state);

    let (status, tags) = send(&router, "GET", "/api/v1/tags", None).await;
    assert_eq!(status, StatusCode::OK);
    let tag = &tags[0];

    // The whole set of fields. A scale range, a raw range or a deadband appearing here would be
    // a value nobody configured, published for every tag.
    let keys: BTreeSet<&str> = tag
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        BTreeSet::from([
            "access_mode",
            "description",
            "engineering_units",
            "retention_mode",
            "slot_id",
            "uri",
        ]),
        "body: {tag}"
    );
    assert_eq!(tag["engineering_units"], "", "no units were stated: {tag}");
    assert_eq!(tag["access_mode"], "READ_ONLY", "body: {tag}");
}

#[tokio::test]
async fn a_client_cannot_register_a_tag_over_a_slot_and_have_what_was_in_it_served() {
    let segment = Segment::new("register");

    // Slot 7 holds a GOOD value from some earlier writer.
    {
        let mut stale = ShmWriterHandle::<PosixShm>::new(&segment.0, CAPACITY).unwrap();
        for _ in 0..3 {
            stale
                .write_tag(
                    7,
                    TagDataType::Uint16.code(),
                    QualityCode::GOOD,
                    TagValueRaw::from_u16(999),
                    1,
                    0,
                )
                .unwrap();
            stale.publish();
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(dir.path()).unwrap());
    let catalog = Arc::new(RwLock::new(TagCatalog::new()));
    let reader = ShmReaderHandle::<PosixShm>::new(&segment.0, CAPACITY).unwrap();
    let state = ApiState::<PosixShm>::new(storage, Arc::clone(&catalog), Some(reader));
    let router = create_router(state);

    // Nothing feeds a tag created this way: the tag bus has one writer, the field I/O runtime,
    // and it registers its own tags. A tag registered over the API would only ever report
    // whatever its slot already held.
    let (status, _) = send(
        &router,
        "POST",
        "/api/v1/tags",
        Some(json!({"uri": "external.Plant.Ghost.value", "slot_id": 7})),
    )
    .await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);

    assert!(catalog.read().await.is_empty());
    let updates = one_batch(&router).await;
    assert!(
        updates.is_empty(),
        "the stream must not report a tag nobody is feeding: {updates:?}"
    );
}
