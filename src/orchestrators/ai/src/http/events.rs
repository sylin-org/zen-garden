//! `GET /v1/events` — unified event stream (ORCH-0030 §1).
//!
//! Subscribers connect once and receive a server-sent-events stream
//! filtered by a comma-separated set of glob patterns. The orchestrator
//! has exactly one event stream; per-domain endpoints like
//! `/v1/catalog/events` are retired in favor of this one.
//!
//! Query parameters:
//! - `focus`  — comma-separated list of dotted glob patterns. If
//!              omitted, the subscriber receives every event.
//! - `since`  — resume from this sequence number. Overridden by the
//!              `Last-Event-ID` header if present.
//!
//! Headers:
//! - `Last-Event-ID` — standard SSE resume header. Wins over `?since`.
//!
//! Wire format:
//! ```text
//! event: <topic>
//! id: <seq>
//! data: {"topic": "...", "seq": ..., "at": "...", "payload": {...}}
//! ```
//!
//! When a client reconnects with a sequence number that has fallen out
//! of the bus's history ring, a synthetic `bus.resume.gap` event is
//! emitted before the live tail. Clients should treat this as a signal
//! to re-fetch authoritative state via REST before trusting subsequent
//! events.

use std::convert::Infallible;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use futures_util::stream::BoxStream;
use serde::Deserialize;

use crate::app_state::AppState;
use crate::domain::events::{FocusMatcher, SubscriptionEvent};

#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    /// Comma-separated list of glob patterns. Empty/missing means
    /// "match everything" — useful for dashboards that want the full
    /// firehose.
    pub focus: Option<String>,
    /// Resume from this sequence number. Overridden by the
    /// `Last-Event-ID` header if both are provided.
    pub since: Option<u64>,
}

pub async fn get_events(
    State(state): State<AppState>,
    Query(params): Query<EventsQuery>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Parse focus. Empty/missing → match everything.
    let focus = match params.focus.as_deref() {
        None | Some("") => FocusMatcher::any(),
        Some(raw) => match FocusMatcher::parse(raw) {
            Ok(m) if m.is_empty() => FocusMatcher::any(),
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(focus = %raw, error = %e, "invalid focus pattern");
                FocusMatcher::any()
            }
        },
    };

    // Resume sequence: Last-Event-ID header takes precedence.
    let since = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .or(params.since);

    let mut subscription = state.events.subscribe(focus, since).await;

    let stream: BoxStream<'static, Result<Event, Infallible>> = Box::pin(async_stream::stream! {
        loop {
            match subscription.recv().await {
                SubscriptionEvent::Event(event) => {
                    let payload = serde_json::json!({
                        "topic":   event.topic,
                        "seq":     event.seq,
                        "at":      event.at,
                        "payload": event.payload,
                    });
                    yield Ok(
                        Event::default()
                            .event(&event.topic)
                            .id(event.seq.to_string())
                            .json_data(&payload)
                            .unwrap_or_else(|_| Event::default().data("{}"))
                    );
                }
                SubscriptionEvent::Lagged(skipped) => {
                    // The subscriber fell behind the broadcast channel.
                    // Emit a synthetic event signaling the gap so the
                    // client can re-fetch state via REST.
                    let payload = serde_json::json!({
                        "topic":   "bus.lagged",
                        "skipped": skipped,
                    });
                    yield Ok(
                        Event::default()
                            .event("bus.lagged")
                            .json_data(&payload)
                            .unwrap_or_else(|_| Event::default().data("{}"))
                    );
                }
                SubscriptionEvent::Closed => break,
            }
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
