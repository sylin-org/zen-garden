//! Presence streaming API endpoint
//!
//! Stone Presence Protocol (PRESENCE-0001) implementation.
//! Streams domain events to connected clients (Firefly, Cricket, etc.)
//! via the unified pulse channel (domain-only, translated to presence vocabulary).

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use chrono::Timelike;
use futures_util::stream::Stream;
use serde::Deserialize;
use std::convert::Infallible;
use tokio_stream::StreamExt;

use crate::domain::traits::CompanionOps;
use crate::domain::StoneEvent;
use crate::infra::PulseEvent;
use crate::AppState;
use garden_common::presence::{
    event_types, ClientNotification, EventFilter, OfferingState, PresenceSnapshot, StoneState,
    StoragePresence,
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
/// **Flow:**
/// 1. Generate snapshot from AppState
/// 2. Subscribe to pulse (unified pulse channel)
/// 3. Filter for Domain events only, apply category filter
/// 4. Translate to presence vocabulary and emit as SSE
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

    // Subscribe to pulse channel (unified domain + transport events)
    let rx = state.pulse_stream();

    // Create the inner event stream: snapshot first, then filtered domain events
    let inner = futures_util::stream::once(async move {
        Event::default()
            .event(event_types::PRESENCE_SNAPSHOT)
            .data(snapshot_json)
    })
    .chain(
        tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(move |result| {
            let filter = filter.clone();
            match result {
                Ok(PulseEvent::Domain(pulse)) => {
                    // Apply category filter
                    if !filter.allows(&pulse.category) {
                        return None;
                    }
                    // Translate to presence vocabulary
                    Some(pulse.to_presence_event())
                }
                Ok(PulseEvent::Transport(_)) => {
                    // Presence stream is domain-only — skip transport events
                    None
                }
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
                    yield Ok::<Event, Infallible>(
                        Event::default().event("server.shutdown").data("{}")
                    );
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Generate presence snapshot from current state
pub(crate) async fn generate_snapshot(state: &AppState) -> PresenceSnapshot {
    let offerings_guard = state.offerings.read().await;

    // Map all offerings (managed + adopted + borrowed)
    let offerings: Vec<OfferingState> = offerings_guard
        .iter()
        .map(|o| OfferingState {
            name: o.name.to_string(),
            status: format!("{:?}", o.status).to_lowercase(),
            health: format!("{:?}", o.health).to_lowercase(),
        })
        .collect();

    // Compute stone state
    let uptime = state.start_time.elapsed().as_secs();

    // Get real resources from system monitor (fallback to zeros if not yet collected)
    let (cpu_percent, memory_percent, disk_percent) = {
        let resources = state.current.resources.system.read().await;
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

    // FIREFLY-0003: GPU utilization
    let gpu_percent = {
        let gpu = state.current.resources.gpu.read().await;
        gpu.unwrap_or(0.0) as f64
    };
    let gpu_active = gpu_percent > 10.0;

    // FIREFLY-0003: Network rates
    let (net_rx, net_tx) = {
        let network = state.current.resources.network.read().await;
        network
            .as_ref()
            .map(|n| {
                (
                    n.rx_bytes_per_sec.unwrap_or(0),
                    n.tx_bytes_per_sec.unwrap_or(0),
                )
            })
            .unwrap_or((0, 0))
    };

    // FIREFLY-0003: Capability flags
    let has_gpu = {
        let caps = state.current.capabilities.read().await;
        caps.as_ref()
            .map(|c| !c.hardware.gpus.is_empty())
            .unwrap_or(false)
    };

    let has_cricket = state.companion.registry.get("cricket").await.is_some();

    // FIREFLY-0003: Seed bank summary (only if one is plugged in)
    let seed_bank = {
        let map = state.current.storage.volumes.read().await;
        map.values().find_map(|v| {
            let mgmt = v.management()?;
            Some(StoragePresence {
                name: mgmt.name.clone(),
                used_gb: v.used_bytes() / 1_073_741_824,
                total_gb: v.capacity_bytes() / 1_073_741_824,
            })
        })
    };

    // FIREFLY-0003: Local time as decimal hour
    let hour = {
        let now = chrono::Local::now();
        now.hour() as f64 + (now.minute() as f64 / 60.0)
    };

    let health = compute_health(cpu_percent, memory_percent);

    PresenceSnapshot {
        stone: StoneState {
            name: state.current.stone.name.clone(),
            health,
            cpu_percent,
            memory_percent,
            disk_percent,
            uptime_seconds: uptime,
            pond_active: state
                .security
                .pond
                .active
                .load(std::sync::atomic::Ordering::Relaxed),
            // FIREFLY-0003 fields
            io_percent: 0.0, // Placeholder until I/O collection is implemented
            gpu_percent,
            net_rx_bytes_per_sec: net_rx,
            net_tx_bytes_per_sec: net_tx,
            has_gpu,
            gpu_active,
            is_lantern: offerings_guard.iter().any(|o| o.offering == "lantern"),
            has_cricket,
            hour,
            seed_bank,
        },
        offerings,
        timestamp: chrono::Utc::now(),
    }
}

/// Compute stone health from resources
fn compute_health(cpu: f64, memory: f64) -> String {
    if cpu > 95.0 || memory > 95.0 {
        garden_common::constants::VITALITY_WILTING.to_string()
    } else if cpu > 80.0 || memory > 80.0 {
        garden_common::constants::VITALITY_WITHERING.to_string()
    } else {
        garden_common::constants::VITALITY_THRIVING.to_string()
    }
}

/// POST /api/v1/stone/presence/notify - Client-initiated presence notification
///
/// Allows clients (Rake, Lantern) to send notifications that get broadcast to all presence clients.
/// Used for visual feedback like "I'm tending to you" that creates a temporary glow/pulse.
///
/// **Use case**: When Rake runs `tend stone-01`, it POSTs to Moss, which emits
/// `stone.tended` event via EventBus → PulseDomainBridge → all SSE subscribers.
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

    // Emit StoneEvent via EventBus (will flow through PulseDomainBridge → presence stream)
    let stone_event = StoneEvent::tended(
        &notification.client,
        notification.from_host.as_deref().unwrap_or("unknown"),
        notification.message.clone(),
    );

    state.event_bus.emit(stone_event);

    tracing::info!(
        event_type = event_types::STONE_TENDED,
        "Broadcasted presence notification to {} pulse subscribers",
        state.pulse.receiver_count()
    );

    Ok(StatusCode::ACCEPTED)
}
