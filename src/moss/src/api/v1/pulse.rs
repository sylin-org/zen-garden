//! Pulse streaming API endpoint
//!
//! Live instrument panel for stone observability.
//! Streams ALL events (domain + transport) via a single SSE endpoint.

use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{Html, IntoResponse},
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::stream::Stream;
use std::convert::Infallible;
use tokio_stream::StreamExt;

use crate::infra::PulseEvent;
use crate::AppState;

const PULSE_HTML: &str = include_str!("../../../assets/pulse.html");

/// GET /pulse
///
/// Returns the pulse instrument panel HTML page.
/// Connects to `/api/v1/stone/pulse/stream` for live events
/// and polls `/api/v1/garden/topology` for current state.
pub async fn get_pulse_page() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        Html(PULSE_HTML),
    )
}

/// GET /api/v1/stone/pulse/stream - Full firehose SSE stream
///
/// Streams ALL pulse events (both transport and domain) to connected clients.
/// Used by the pulse instrument panel for real-time observability.
///
/// Event types:
/// - `domain.*` — Domain events (service.started, stone.tended, etc.)
/// - `transport.*` — Raw UDP announcements (stone_chirp, election_request, etc.)
pub async fn stream_pulse(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    tracing::info!("Pulse client connected");

    // MOSS-0004: child token for cooperative shutdown
    let token = state.shutdown_token.child_token();

    // Subscribe to pulse channel
    let rx = state.pulse_tx.subscribe();

    let inner = tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|result| {
        match result {
            Ok(pulse_event) => {
                let (event_type, data) = match &pulse_event {
                    PulseEvent::Domain(d) => {
                        let etype = format!("domain.{}", d.event_type);
                        let data = serde_json::to_string(d).unwrap_or_default();
                        (etype, data)
                    }
                    PulseEvent::Transport(t) => {
                        let etype = format!("transport.{}", t.announcement_type);
                        let data = serde_json::to_string(t).unwrap_or_default();
                        (etype, data)
                    }
                };
                Some(Event::default().event(event_type).data(data))
            }
            Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                tracing::warn!("Pulse client lagged {} events", n);
                None
            }
        }
    });

    // MOSS-0004: Wrap in cancellation-aware stream
    let stream = async_stream::stream! {
        tokio::pin!(inner);
        loop {
            tokio::select! {
                item = inner.next() => {
                    match item {
                        Some(event) => yield Ok::<Event, Infallible>(event),
                        None => break,
                    }
                }
                _ = token.cancelled() => {
                    tracing::debug!("Pulse stream: shutdown token cancelled");
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}
