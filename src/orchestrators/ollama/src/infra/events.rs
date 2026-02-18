//! SSE event broadcast for the dashboard.
//!
//! The dashboard subscribes to `/api/events` and receives live updates
//! whenever the registry, metrics, or health changes.

use crate::app_state::DashboardEvent;
use axum::response::sse::{Event, Sse};
use futures_util::stream::Stream;
use std::convert::Infallible;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

/// Create an SSE stream from the dashboard broadcast channel.
pub fn dashboard_sse_stream(
    tx: &broadcast::Sender<DashboardEvent>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
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
