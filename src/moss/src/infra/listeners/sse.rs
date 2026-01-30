//! SSE Listener - Real-time client event streaming
//!
//! Listens for offering lifecycle events and broadcasts them to connected
//! SSE clients via tokio broadcast channel.

use crate::domain::events::OfferingEvent;
use crate::infra::event_bus::EventListener;
use chrono::Utc;
use garden_common::SSE_LEVEL_INFO;
use serde::Serialize;
use tokio::sync::broadcast;

/// SSE event payload sent to connected clients
#[derive(Debug, Clone, Serialize)]
pub struct SseEvent {
    /// Event timestamp (ISO 8601)
    pub timestamp: String,
    /// Event level (info, warn, error)
    pub level: String,
    /// Event type (deployed, started, etc.)
    pub event_type: String,
    /// Human-readable message
    pub message: String,
    /// Optional job ID for tracking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    /// Offering name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offering: Option<String>,
    /// Offering ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offering_id: Option<String>,
}

impl From<&OfferingEvent> for SseEvent {
    fn from(event: &OfferingEvent) -> Self {
        Self {
            timestamp: Utc::now().to_rfc3339(),
            level: SSE_LEVEL_INFO.to_string(),
            event_type: event.event_type().to_string(),
            message: event.to_message(),
            job_id: None,
            offering: Some(event.name().to_string()),
            offering_id: Some(event.offering_id().to_string()),
        }
    }
}

/// Listener that broadcasts events to SSE clients
pub struct SseListener {
    /// Broadcast sender for SSE events
    tx: broadcast::Sender<SseEvent>,
}

impl SseListener {
    /// Create a new SSE listener with the given broadcast sender
    ///
    /// The sender should be the same one used by the SSE endpoint.
    pub fn new(tx: broadcast::Sender<SseEvent>) -> Self {
        Self { tx }
    }

    /// Create a new SSE listener with its own channel
    ///
    /// Returns the listener and a receiver for testing.
    pub fn with_channel(capacity: usize) -> (Self, broadcast::Receiver<SseEvent>) {
        let (tx, rx) = broadcast::channel(capacity);
        (Self { tx }, rx)
    }

    /// Get a receiver for the SSE events
    pub fn subscribe(&self) -> broadcast::Receiver<SseEvent> {
        self.tx.subscribe()
    }
}

#[async_trait::async_trait]
impl EventListener for SseListener {
    async fn on_event(&self, event: &OfferingEvent) {
        let sse_event = SseEvent::from(event);

        match self.tx.send(sse_event) {
            Ok(count) => {
                tracing::trace!(
                    event_type = event.event_type(),
                    receivers = count,
                    "SSE event broadcast"
                );
            }
            Err(_) => {
                // No receivers - this is fine
                tracing::trace!(
                    event_type = event.event_type(),
                    "SSE event dropped (no receivers)"
                );
            }
        }
    }

    fn name(&self) -> &'static str {
        "sse"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sse_event_conversion() {
        let event = OfferingEvent::deployed("id-1", "mongodb", "stone-01", "mongo:7");
        let sse_event = SseEvent::from(&event);

        assert_eq!(sse_event.event_type, "deployed");
        assert_eq!(sse_event.message, "Service mongodb deployed");
        assert_eq!(sse_event.offering, Some("mongodb".to_string()));
        assert_eq!(sse_event.offering_id, Some("id-1".to_string()));
    }

    #[tokio::test]
    async fn test_sse_listener_broadcast() {
        let (listener, mut rx) = SseListener::with_channel(16);

        let event = OfferingEvent::started("id-1", "mongodb", "stone-01");
        listener.on_event(&event).await;

        let received = rx.recv().await.unwrap();
        assert_eq!(received.event_type, "started");
        assert_eq!(received.message, "Service mongodb started");
    }
}
