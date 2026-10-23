//! Server-Sent Events (SSE) telemetry stream with locked 100ms batch throttling.

use crate::api::state::ApiState;
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use mythos_pal::PlatformShm;
use serde::Serialize;
use std::convert::Infallible;
use std::time::Duration;
use tokio::time::interval;
use tokio_stream::wrappers::IntervalStream;
use tokio_stream::Stream;
use tokio_stream::StreamExt;

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
/// Dispatches SSE events throttled to 100ms (10 Hz) batch intervals.
pub async fn telemetry_stream_handler<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static> {
    let tick_interval = interval(Duration::from_millis(100));
    let stream = IntervalStream::new(tick_interval).then(move |_| {
        let state = state.clone();
        async move {
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
            Ok::<Event, Infallible>(Event::default().event("tag_update").data(json_data))
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default().interval(Duration::from_secs(15)))
}
