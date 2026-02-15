//! Presence SSE stream — real-time events for the dashboard
//!
//! Mirrors the Moss SSE pattern: subscribe → broadcast stream → filter_map → SSE response.

use std::convert::Infallible;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream::{self, Stream, StreamExt};
use tokio_stream::wrappers::BroadcastStream;

use crate::infra::event_bus::SseEvent;
use crate::AppState;

/// GET /api/v1/garden/presence/stream — SSE stream of garden events
pub async fn get_presence_stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // 1. Build initial snapshot of current topology
    let snapshot = {
        let topology = state.topology.read().await;
        let stones: Vec<serde_json::Value> = topology
            .stones
            .values()
            .map(|entry| {
                serde_json::json!({
                    "stone_id": entry.stone_id,
                    "stone_name": entry.stone_name,
                    "endpoint": entry.address.http_base(),
                    "health": entry.health,
                    "status": format!("{:?}", entry.status).to_lowercase(),
                    "services_count": entry.services.len(),
                })
            })
            .collect();

        serde_json::json!({
            "event_type": "snapshot",
            "stones": stones,
            "stones_count": stones.len(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        })
    };

    let snapshot_event = Event::default()
        .event("snapshot")
        .data(snapshot.to_string());
    let snapshot_stream = stream::once(async move { Ok::<Event, Infallible>(snapshot_event) });

    // 2. Subscribe to live domain events
    let rx = state.event_bus.subscribe();
    let live_stream = BroadcastStream::new(rx).filter_map(|result| async move {
        match result {
            Ok(domain_event) => {
                let sse_event = SseEvent::from(&domain_event);
                let data = serde_json::to_string(&sse_event).ok()?;
                Some(Ok(Event::default().event(sse_event.event_type).data(data)))
            }
            Err(_) => None, // Skip lagged messages
        }
    });

    let stream = snapshot_stream.chain(live_stream);
    Sse::new(stream).keep_alive(KeepAlive::default())
}
