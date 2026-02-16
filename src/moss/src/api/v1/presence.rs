//! Presence streaming API endpoint
//!
//! Stone Presence Protocol (PRESENCE-0001) implementation.
//! Streams domain events to connected clients (Firefly, Cricket, etc.)
//! via the unified SseEvent channel.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures_util::stream::Stream;
use serde::Deserialize;
use std::convert::Infallible;
use tokio_stream::StreamExt;

use crate::domain::StoneEvent;
use crate::infra::SseEvent;
use crate::AppState;
use garden_common::presence::{
    event_types, ClientNotification, EventFilter, OfferingState, PresenceSnapshot, StoneState,
};

#[derive(Debug, Deserialize)]
pub struct PresenceQuery {
    /// Comma-separated event categories to filter (e.g., "service,stone,storage")
    categories: Option<String>,
}

/// GET /api/v1/stone/presence/stream - Local stone presence stream
///
/// **Scope:** Stone-level (local events only)
/// **Consumer:** Local Companions (Cricket, Firefly, OLED), Rake presence command
///
/// Returns SSE stream of domain events in presence vocabulary.
/// Only emits events relevant to THIS stone.
///
/// **URI Semantics (API-0001):**
/// - `/api/v1/stone/*` - Stone-scoped operations (this stone only)
/// - `/api/v1/garden/*` - Garden-scoped operations (all stones, via Lantern)
///
/// **Flow:**
/// 1. Generate snapshot from AppState
/// 2. Subscribe to sse_tx (unified SSE event channel)
/// 3. Filter events by category if requested
/// 4. Emit as SSE
///
/// **Query Parameters:**
/// - `categories`: Comma-separated event categories (e.g., "service,stone,storage")
pub async fn stream_stone_presence(
    Query(query): Query<PresenceQuery>,
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    tracing::info!("Presence client connected");

    // MOSS-0004: child token for cooperative shutdown
    let token = state.shutdown_token.child_token();

    // Parse event filter from query params
    let filter = if let Some(cats) = query.categories {
        let categories = cats.split(',').map(|s| s.trim().to_string()).collect();
        EventFilter { categories }
    } else {
        EventFilter::allow_all()
    };

    // Generate initial snapshot
    let snapshot = generate_snapshot(&state).await;
    let snapshot_json = serde_json::to_string(&snapshot).unwrap_or_default();

    // Subscribe to SSE events (unified channel from SseListener)
    let rx = state.sse_tx.subscribe();

    // Create the inner event stream: snapshot first, then filtered live events
    let inner = futures_util::stream::once(async move {
        Event::default()
            .event(event_types::PRESENCE_SNAPSHOT)
            .data(snapshot_json)
    })
    .chain(
        tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(move |result| {
            let filter = filter.clone();
            match result {
                Ok(sse_event) => translate_to_presence(sse_event, &filter),
                Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                    tracing::warn!("Presence client lagged {} events", n);
                    None
                }
            }
        }),
    );

    // MOSS-0004: Wrap in cancellation-aware stream. When the shutdown token
    // is cancelled, the stream ends — unblocking axum's graceful drain
    // instead of hanging indefinitely on persistent SSE connections.
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
                    tracing::debug!("Presence stream: shutdown token cancelled");
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Generate presence snapshot from current state
async fn generate_snapshot(state: &AppState) -> PresenceSnapshot {
    let offerings_guard = state.offerings.read().await;

    // Map all offerings (managed + adopted + borrowed)
    let offerings: Vec<OfferingState> = offerings_guard
        .iter()
        .map(|o| OfferingState {
            name: o.name.clone(),
            status: format!("{:?}", o.status).to_lowercase(),
            health: format!("{:?}", o.health).to_lowercase(),
        })
        .collect();

    // Compute stone state
    let uptime = state.start_time.elapsed().as_secs();

    // Get real metrics from system monitor (fallback to zeros if not yet collected)
    let (cpu_percent, memory_percent, disk_percent) = {
        let resources = state.system_resources.read().await;
        if let Some(ref res) = *resources {
            // Use primary mount point (root or largest disk) for summary disk %
            let primary_disk_percent = res
                .storage
                .iter()
                .find(|s| s.mount_point == "/" || s.mount_point == "C:\\\\")
                .or_else(|| res.storage.iter().max_by_key(|s| s.total_gb))
                .map(|s| s.used_percent as f64)
                .unwrap_or(0.0);

            (
                res.cpu.usage_percent as f64,
                res.memory.used_percent as f64,
                primary_disk_percent,
            )
        } else {
            (0.0, 0.0, 0.0)
        }
    };

    let health = compute_health(cpu_percent, memory_percent);

    PresenceSnapshot {
        stone: StoneState {
            name: state.stone_name.clone(),
            health,
            cpu_percent,
            memory_percent,
            disk_percent,
            uptime_seconds: uptime,
            pond_active: false, // Pond security not yet implemented
        },
        offerings,
        timestamp: chrono::Utc::now(),
    }
}

/// Compute stone health from metrics
fn compute_health(cpu: f64, memory: f64) -> String {
    if cpu > 95.0 || memory > 95.0 {
        "wilting".to_string()
    } else if cpu > 80.0 || memory > 80.0 {
        "withering".to_string()
    } else {
        "thriving".to_string()
    }
}

/// Translate SseEvent to presence SSE event
///
/// Filters by category and converts to the format expected by Companions.
fn translate_to_presence(sse_event: SseEvent, filter: &EventFilter) -> Option<Event> {
    // Determine category from event type
    let category = if sse_event
        .event_type
        .starts_with(event_types::PREFIX_SERVICE)
    {
        event_types::CATEGORY_SERVICE
    } else if sse_event.event_type.starts_with(event_types::PREFIX_STONE) {
        event_types::CATEGORY_STONE
    } else if sse_event
        .event_type
        .starts_with(event_types::PREFIX_STORAGE)
    {
        event_types::CATEGORY_STORAGE
    } else if sse_event.event_type.starts_with(event_types::PREFIX_JOB) {
        event_types::CATEGORY_JOB
    } else {
        return None; // Unknown category
    };

    // Apply filter
    if !filter.allows(category) {
        return None;
    }

    // Build event data
    let mut data = serde_json::json!({
        "timestamp": sse_event.timestamp,
        "message": sse_event.message,
    });

    // Add optional fields
    if let Some(ref offering) = sse_event.offering {
        data["service"] = serde_json::Value::String(offering.clone());
    }
    if let Some(serde_json::Value::Object(map)) = sse_event.data.as_ref() {
        // Merge extra data
        for (k, v) in map {
            data[k] = v.clone();
        }
    }

    Some(
        Event::default()
            .event(&sse_event.event_type)
            .data(data.to_string()),
    )
}

/// POST /api/v1/stone/presence/notify - Client-initiated presence notification
///
/// Allows clients (Rake, Lantern) to send notifications that get broadcast to all presence clients.
/// Used for visual feedback like "I'm tending to you" that creates a temporary glow/pulse.
///
/// **Use case**: When Rake runs `tend stone-01`, it POSTs to Moss, which emits
/// `stone.tended` event via EventBus → SseListener → all SSE subscribers.
///
/// **Payload**:
/// ```json
/// {
///   "event_type": "tended",
///   "client": "rake",
///   "from_host": "leo-laptop",
///   "message": "Tending started"
/// }
/// ```
pub async fn notify_presence(
    State(state): State<AppState>,
    Json(notification): Json<ClientNotification>,
) -> Result<StatusCode, (StatusCode, String)> {
    tracing::info!(
        event = %notification.event_type,
        client = %notification.client,
        "Received client presence notification"
    );

    // Only support "tended" for now
    if notification.event_type != "tended" {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Unknown event type: {}", notification.event_type),
        ));
    }

    // Emit StoneEvent via EventBus (will flow through SseListener → presence stream)
    let stone_event = StoneEvent::tended(
        &notification.client,
        notification.from_host.as_deref().unwrap_or("unknown"),
        notification.message.clone(),
    );

    state.event_bus.emit(stone_event);

    tracing::info!(
        event_type = event_types::STONE_TENDED,
        "Broadcasted presence notification to {} SSE subscribers",
        state.sse_tx.receiver_count()
    );

    Ok(StatusCode::ACCEPTED)
}
