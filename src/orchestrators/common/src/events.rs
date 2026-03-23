//! Dashboard SSE event broadcast.
//!
//! Provides a `DashboardEvent` type and a helper to create an SSE stream
//! from a broadcast channel — shared across all orchestrator dashboards.

use axum::response::sse::{Event, Sse};
use futures_util::stream::Stream;
use std::convert::Infallible;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

/// Dashboard SSE event.
#[derive(Debug, Clone)]
pub struct DashboardEvent {
    pub event_type: String,
    pub data: String,
}

/// Create an SSE stream from the dashboard broadcast channel.
///
/// Skips lagged events (subscriber fell behind) and sends heartbeat
/// keep-alives every 15 seconds.
pub fn dashboard_sse_stream(
    tx: &broadcast::Sender<DashboardEvent>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>> + use<>> {
    let rx = tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(event) => Some(Ok(Event::default()
            .event(event.event_type)
            .data(event.data))),
        Err(_) => None, // lagged — skip
    });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("heartbeat"),
    )
}
