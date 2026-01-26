//! Presence streaming API endpoint
//!
//! Stone Presence Protocol (PRESENCE-0001) implementation.
//! Translates DomainEvents to garden-native presence vocabulary at SSE boundary.

use axum::{
    extract::{Query, State},
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::stream::Stream;
use std::convert::Infallible;
use tokio_stream::StreamExt;
use serde::Deserialize;

use crate::AppState;
use garden_common::presence::{event_types, EventFilter, PresenceSnapshot, StoneState, ServiceState};

#[derive(Debug, Deserialize)]
pub struct PresenceQuery {
    /// Comma-separated event categories to filter (e.g., "service,stone")
    categories: Option<String>,
}

/// GET /api/v1/stone/presence/stream - Local stone presence stream
///
/// **Scope:** Stone-level (local events only)
/// **Consumer:** Local adapters (Cricket, Firefly, OLED), Rake presence command
/// 
/// Returns SSE stream of domain events translated to presence vocabulary.
/// Only emits events relevant to THIS stone (filters out garden-wide events).
/// 
/// **URI Semantics (API-0001):**
/// - `/api/v1/stone/*` - Stone-scoped operations (this stone only)
/// - `/api/v1/garden/*` - Garden-scoped operations (all stones, via Lantern)
/// 
/// **Flow:**
/// 1. Generate snapshot from AppState
/// 2. Subscribe to EventBus (MossEvent stream for now)
/// 3. **Filter** to local stone events only (future: when DomainEvent integration complete)
/// 4. Translate each event to garden-native vocabulary
/// 5. Emit as SSE
/// 
/// **Query Parameters:**
/// - `categories`: Comma-separated event categories (e.g., "service,stone")
pub async fn stream_stone_presence(
    Query(query): Query<PresenceQuery>,
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    tracing::info!("Local presence adapter connected");
    
    // Parse event filter from query params
    let filter = if let Some(cats) = query.categories {
        let categories = cats.split(',').map(|s| s.trim().to_string()).collect();
        EventFilter { categories }
    } else {
        EventFilter::allow_all()
    };
    
    let stone_name = state.stone_name.clone();
    
    // Generate initial snapshot
    let snapshot = generate_snapshot(&state).await;
    let snapshot_json = serde_json::to_string(&snapshot).unwrap_or_default();
    
    // Subscribe to domain events
    // TODO: Use EventBus when available
    // For now, use existing event_tx (MossEvent)
    let rx = state.event_tx.subscribe();
    
    // Create stream: snapshot first, then filtered + translated events
    let stream = futures_util::stream::once(async move {
        Event::default()
            .event(event_types::PRESENCE_SNAPSHOT)
            .data(snapshot_json)
    })
    .chain(
        tokio_stream::wrappers::BroadcastStream::new(rx)
            .filter_map(move |result| {
                let filter = filter.clone();
                let stone_name = stone_name.clone();
                match result {
                    Ok(event) => translate_to_presence(&event, &filter, &stone_name),
                    Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                        tracing::warn!("Presence adapter lagged {} events", n);
                        None
                    }
                }
            })
    )
    .map(Ok);
    
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Generate presence snapshot from current state
async fn generate_snapshot(state: &AppState) -> PresenceSnapshot {
    let registry = state.registry.read().await;
    
    // Map services
    let services: Vec<ServiceState> = registry
        .iter()
        .map(|svc| ServiceState {
            name: svc.name.clone(),
            state: format!("{:?}", svc.status), // Convert ServiceStatus to String
            health: "healthy".to_string(), // TODO: Real health check
        })
        .collect();
    
    // Compute stone state
    let uptime = state.start_time.elapsed().as_secs();
    
    // TODO: Real metrics from system monitor
    let cpu_percent = 25.0;
    let memory_percent = 45.0;
    let disk_percent = 60.0;
    
    let health = compute_health(cpu_percent, memory_percent);
    
    PresenceSnapshot {
        stone: StoneState {
            name: state.stone_name.clone(),
            health,
            cpu_percent,
            memory_percent,
            disk_percent,
            uptime_seconds: uptime,
            pond_active: false, // TODO: Real pond status
        },
        services,
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

/// Translate MossEvent to presence SSE event
/// 
/// This is temporary. When EventBus integration is complete,
/// this will translate DomainEvent instead.
fn translate_to_presence(moss_event: &crate::MossEvent, filter: &EventFilter, _stone_name: &str) -> Option<Event> {
    // Parse message for event type
    // This is hacky but temporary until EventBus integration
    
    if moss_event.message.contains("started successfully") && filter.allows(event_types::CATEGORY_SERVICE) {
        let service = extract_service_name(&moss_event.message)?;
        let data = serde_json::json!({
            "service": service,
            "timestamp": moss_event.timestamp,
        });
        Some(Event::default()
            .event(event_types::SERVICE_STARTED)
            .data(data.to_string()))
    } else if moss_event.message.contains("stopped") && filter.allows(event_types::CATEGORY_SERVICE) {
        let service = extract_service_name(&moss_event.message)?;
        let data = serde_json::json!({
            "service": service,
            "timestamp": moss_event.timestamp,
        });
        Some(Event::default()
            .event(event_types::SERVICE_STOPPED)
            .data(data.to_string()))
    } else if moss_event.message.contains("Stone load:") && filter.allows(event_types::CATEGORY_STONE) {
        // Parse load message
        Some(Event::default()
            .event(event_types::STONE_LOAD_UPDATED)
            .data(serde_json::json!({
                "timestamp": moss_event.timestamp,
                "message": moss_event.message,
            }).to_string()))
    } else {
        None // Skip events that don't map to presence or are filtered out
    }
}

/// Extract service name from message (temporary hack)
fn extract_service_name(message: &str) -> Option<String> {
    // TODO: Remove this when DomainEvent integration is complete
    // Message format: "Service <name> started successfully"
    let parts: Vec<&str> = message.split_whitespace().collect();
    if parts.len() >= 2 && parts[0] == "Service" {
        Some(parts[1].to_string())
    } else {
        None
    }
}
