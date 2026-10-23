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
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TagValueUpdate {
    pub uri: String,
    pub slot_id: u32,
    pub value: f64,
    pub quality: String,
    pub timestamp_ns: u64,
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

                let val = slot.value.as_f64();
                updates.push(TagValueUpdate {
                    uri: tag_meta.uri.clone(),
                    slot_id: tag_meta.slot_id,
                    value: val,
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
