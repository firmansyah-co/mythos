//! Server-Sent Events (SSE) telemetry stream with locked 100ms batch throttling.

use crate::api::state::ApiState;
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use mythos_pal::PlatformShm;
use serde::Serialize;
use std::convert::Infallible;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use tokio::time::interval;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;

/// Tag update payload broadcast in 100ms batches.
///
/// # Wire shape (the HMI consumer is built against this, so it is a contract)
///
/// `value` carries the tag's **true** JSON type — `true`/`false` for BOOL, an integer for the
/// integer types, a number for the float types — and `type` names that type
/// (`"BOOL"`, `"INT"`, `"DINT"`, `"REAL"`, `"LREAL"`, `"UINT"`, `"UDINT"`, `"LINT"`, `"ULINT"`,
/// `"BYTE"`). A consumer must branch on `type` when it needs to know the type; it must not infer
/// one from the number's shape.
///
/// A tag whose slot carries no type code (`0`) or an unrecognised one is reported as
/// `type: "UNSPECIFIED"` with `value: null`. That is deliberate: the previous implementation read
/// every slot through `as_f64` regardless of type, so a DINT holding 42 surfaced as a denormal
/// (~1e-322) and looked like a broken instrument rather than an undeclared type. `null` plus an
/// explicit type is the honest representation of "the writer did not say what this is".
///
/// 64-bit integers are emitted as JSON numbers. `serde_json` preserves the full `i64`/`u64` range
/// in the wire text, but JavaScript clients lose precision above 2^53; a client that must be exact
/// for `LINT`/`ULINT` should parse with a BigInt-aware reader. This is stated rather than solved
/// here because the alternative (stringly-typed values) makes every consumer branch on `type`
/// before it can do arithmetic, which is worse for the common case.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TagValueUpdate {
    pub uri: String,
    pub slot_id: u32,
    /// Declared tag type name, or `"UNSPECIFIED"` when the slot carries no usable type code.
    #[serde(rename = "type")]
    pub type_name: &'static str,
    /// The value, decoded according to the slot's declared type. `null` when unspecified.
    pub value: serde_json::Value,
    pub quality: String,
    pub timestamp_ns: u64,
}

/// Decode a slot value according to its declared `type_code`.
///
/// Returns the wire type name and the value. The type code is the *only* source of truth: reading
/// a slot with an accessor that does not match how it was written produces a meaningless number,
/// which is exactly the defect this replaces.
pub(crate) fn decode_slot_value(
    type_code: u16,
    value: &mythos_shm::TagValueRaw,
) -> (&'static str, serde_json::Value) {
    use serde_json::Value;
    match type_code {
        1 => ("BOOL", Value::Bool(value.as_bool())),
        2 => ("INT", Value::from(value.as_i16())),
        3 => ("DINT", Value::from(value.as_i32())),
        4 => ("REAL", Value::from(value.as_f32())),
        5 => ("LREAL", Value::from(value.as_f64())),
        6 => ("UINT", Value::from(value.as_u16())),
        7 => ("UDINT", Value::from(value.as_u32())),
        8 => ("LINT", Value::from(value.as_i64())),
        9 => ("ULINT", Value::from(value.as_u64())),
        10 => ("BYTE", Value::from(value.as_u8())),
        // No declared type: report it as such rather than guessing f64 and emitting a denormal.
        _ => ("UNSPECIFIED", Value::Null),
    }
}

/// 100ms batched telemetry payload envelope.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TelemetryBatch {
    pub timestamp_ms: u64,
    pub batch_window_ms: u32,
    pub updates: Vec<TagValueUpdate>,
}

/// GET /api/v1/telemetry/stream
///
/// Dispatches SSE events throttled to 100ms (10 Hz) batch intervals, and relays HMI
/// definition-change notifications (e.g. `FACEPLATE_PROPAGATED`) on the same stream so an
/// open operator station refreshes without reloading and without a second connection.
pub async fn telemetry_stream_handler<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static> {
    // A relay task owns the tick and the broadcast receiver: the SSE response body is a
    // single stream, and both sources must feed it.
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(64);
    let mut hmi_events = state.hmi_events.subscribe();

    tokio::spawn(async move {
        let mut tick_interval = interval(Duration::from_millis(100));
        loop {
            tokio::select! {
                _ = tick_interval.tick() => {
                    let event = build_telemetry_event(&state).await;
                    if tx.send(event).await.is_err() {
                        // Every browser closed the stream; stop working for it.
                        break;
                    }
                }
                received = hmi_events.recv() => {
                    match received {
                        Ok(hmi) => {
                            let data = serde_json::to_string(&hmi.data)
                                .unwrap_or_else(|_| "{}".to_string());
                            let event = Event::default().event(hmi.event).data(data);
                            if tx.send(Ok(event)).await.is_err() {
                                break;
                            }
                        }
                        // A lagging subscriber missed notifications but the stream is alive;
                        // dropping the gap is better than closing a live operator station.
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    });

    Sse::new(ReceiverStream::new(rx))
        .keep_alive(KeepAlive::default().interval(Duration::from_secs(15)))
}

/// Build one `tag_update` event from the current SHM snapshot.
async fn build_telemetry_event<S: PlatformShm + 'static>(
    state: &ApiState<S>,
) -> Result<Event, Infallible> {
    let catalog_lock = state.tag_catalog.read().await;
    let tags: Vec<crate::tag::TagMetadata> = catalog_lock.iter().cloned().collect();
    drop(catalog_lock);

    let mut updates = Vec::new();

    if let Some(shm_reader_arc) = &state.shm_reader {
        let mut reader = shm_reader_arc.write().await;
        // Advance read snapshot
        let _ = reader.update();

        for tag_meta in tags {
            if let Ok(slot) = reader.read_tag(tag_meta.slot_id as usize) {
                let quality_str = if slot.quality.is_good() {
                    "GOOD"
                } else if slot.quality.is_bad() {
                    "BAD"
                } else {
                    "UNCERTAIN"
                };

                let (type_name, value) = decode_slot_value(slot.type_code, &slot.value);
                updates.push(TagValueUpdate {
                    uri: tag_meta.uri.clone(),
                    slot_id: tag_meta.slot_id,
                    type_name,
                    value,
                    quality: quality_str.to_string(),
                    timestamp_ns: slot.timestamp_ns,
                });
            }
        }
    }

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let batch = TelemetryBatch {
        timestamp_ms: now_ms,
        batch_window_ms: 100,
        updates,
    };

    let json_data = serde_json::to_string(&batch).unwrap_or_else(|_| "{}".to_string());
    Ok(Event::default().event("tag_update").data(json_data))
}
