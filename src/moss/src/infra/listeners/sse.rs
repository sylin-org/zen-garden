//! SSE Listener - Real-time client event streaming
//!
//! Listens for domain events and broadcasts them to connected
//! SSE clients (Firefly, Cricket, etc.) via tokio broadcast channel.

use crate::domain::events::{DomainEvent, JobEvent, OfferingEvent, StoneEvent, StorageEvent};
use crate::infra::event_bus::EventListener;
use chrono::Utc;
use garden_common::{
    presence::{event_types, StoneHealthChangedPayload, StoneLoadUpdatedPayload},
    EVENT_DEPLOYED, EVENT_DESTROYED, EVENT_HEALTH_CHANGED, EVENT_REMOVED, EVENT_RENAMED,
    EVENT_STARTED, EVENT_STOPPED, EVENT_UPDATED, SSE_LEVEL_INFO,
};
use serde::Serialize;
use tokio::sync::broadcast;

/// SSE event payload sent to connected clients
#[derive(Debug, Clone, Serialize)]
pub struct SseEvent {
    /// Event timestamp (ISO 8601)
    pub timestamp: String,
    /// Event level (info, warn, error)
    pub level: String,
    /// Event type (service.started, storage.detected, stone.tended, etc.)
    pub event_type: String,
    /// Human-readable message
    pub message: String,
    /// Optional job ID for tracking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    /// Offering name (for offering events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offering: Option<String>,
    /// Offering ID (for offering events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offering_id: Option<String>,
    /// Additional event data as JSON
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl From<&DomainEvent> for SseEvent {
    fn from(event: &DomainEvent) -> Self {
        match event {
            DomainEvent::Offering(e) => Self::from_offering(e),
            DomainEvent::Storage(e) => Self::from_storage(e),
            DomainEvent::Stone(e) => Self::from_stone(e),
            DomainEvent::Job(e) => Self::from_job(e),
        }
    }
}

impl SseEvent {
    fn from_offering(event: &OfferingEvent) -> Self {
        Self {
            timestamp: Utc::now().to_rfc3339(),
            level: SSE_LEVEL_INFO.to_string(),
            event_type: Self::translate_offering_type(event.event_type()).to_string(),
            message: event.to_message(),
            job_id: None,
            offering: Some(event.name().to_string()),
            offering_id: Some(event.offering_id().to_string()),
            data: None,
        }
    }

    fn from_storage(event: &StorageEvent) -> Self {
        let data = match event {
            StorageEvent::SeedBankDetected {
                name,
                device,
                mount_path,
                capacity_gb,
                ..
            } => Some(serde_json::json!({
                "name": name,
                "device": device,
                "mount_path": mount_path,
                "capacity_gb": capacity_gb,
            })),
            StorageEvent::SeedBankRemoved { name, device, .. } => Some(serde_json::json!({
                "name": name,
                "device": device,
            })),
            StorageEvent::SyncStarted { name, .. } => Some(serde_json::json!({ "name": name })),
            StorageEvent::SyncCompleted { name, success, .. } => {
                Some(serde_json::json!({ "name": name, "success": success }))
            }
        };

        Self {
            timestamp: Utc::now().to_rfc3339(),
            level: SSE_LEVEL_INFO.to_string(),
            event_type: event.event_type().to_string(),
            message: event.to_message(),
            job_id: None,
            offering: None,
            offering_id: None,
            data,
        }
    }

    fn from_stone(event: &StoneEvent) -> Self {
        let data = match event {
            StoneEvent::Tended {
                by, from, message, ..
            } => Some(serde_json::json!({
                "by": by,
                "from": from,
                "message": message,
            })),
            StoneEvent::HealthChanged {
                health,
                cpu_percent,
                memory_percent,
                ..
            } => {
                let payload = StoneHealthChangedPayload {
                    health: health.clone(),
                    cpu_percent: *cpu_percent,
                    memory_percent: *memory_percent,
                };
                serde_json::to_value(payload).ok()
            }
            StoneEvent::LoadUpdated {
                cpu_percent,
                memory_percent,
                ..
            } => {
                let payload = StoneLoadUpdatedPayload {
                    cpu_percent: *cpu_percent,
                    memory_percent: *memory_percent,
                };
                serde_json::to_value(payload).ok()
            }
            StoneEvent::NetworkReady { ip, interface, .. } => Some(serde_json::json!({
                "ip": ip,
                "interface": interface,
            })),
        };

        Self {
            timestamp: Utc::now().to_rfc3339(),
            level: SSE_LEVEL_INFO.to_string(),
            event_type: event.event_type().to_string(),
            message: event.to_message(),
            job_id: None,
            offering: None,
            offering_id: None,
            data,
        }
    }

    fn from_job(event: &JobEvent) -> Self {
        let data = match event {
            JobEvent::Started { operation, .. } => Some(serde_json::json!({
                "operation": operation,
            })),
            JobEvent::Progress { .. } => None,
            JobEvent::Completed { .. } => None,
            JobEvent::Failed { error, .. } => Some(serde_json::json!({
                "error": error,
            })),
        };

        Self {
            timestamp: Utc::now().to_rfc3339(),
            level: event.level().to_string(),
            event_type: event.event_type().to_string(),
            message: event.to_message(),
            job_id: Some(event.job_id().to_string()),
            offering: Some(event.offering().to_string()),
            offering_id: None,
            data,
        }
    }

    /// Translate internal event types to presence protocol vocabulary
    fn translate_offering_type(event_type: &str) -> &'static str {
        match event_type {
            EVENT_DEPLOYED => event_types::SERVICE_STARTED,
            EVENT_STARTED => event_types::SERVICE_STARTED,
            EVENT_STOPPED => event_types::SERVICE_STOPPED,
            EVENT_REMOVED => event_types::SERVICE_STOPPED,
            EVENT_DESTROYED => event_types::SERVICE_STOPPED,
            EVENT_UPDATED => event_types::SERVICE_UPDATED,
            EVENT_RENAMED => event_types::SERVICE_RENAMED,
            EVENT_HEALTH_CHANGED => event_types::SERVICE_HEALTH_CHANGED,
            _ => event_types::SERVICE_STARTED, // Fallback for unknown types
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

    /// Get the sender for sharing with other components
    pub fn sender(&self) -> broadcast::Sender<SseEvent> {
        self.tx.clone()
    }
}

#[async_trait::async_trait]
impl EventListener for SseListener {
    async fn on_event(&self, event: &DomainEvent) {
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
        super::names::SSE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sse_event_conversion_offering() {
        let event = DomainEvent::Offering(OfferingEvent::deployed(
            "id-1", "mongodb", "stone-01", "mongo:7",
        ));
        let sse_event = SseEvent::from(&event);

        assert_eq!(sse_event.event_type, event_types::SERVICE_STARTED); // Translated!
        assert_eq!(sse_event.message, "Service mongodb deployed");
        assert_eq!(sse_event.offering, Some("mongodb".to_string()));
        assert_eq!(sse_event.offering_id, Some("id-1".to_string()));
    }

    #[tokio::test]
    async fn test_sse_event_conversion_storage() {
        let event = DomainEvent::Storage(StorageEvent::seed_bank_detected(
            "backup",
            "/dev/sdb1",
            "/mnt/backup",
            500,
        ));
        let sse_event = SseEvent::from(&event);

        assert_eq!(sse_event.event_type, event_types::STORAGE_DETECTED);
        assert!(sse_event.message.contains("backup"));
        assert!(sse_event.data.is_some());
    }

    #[tokio::test]
    async fn test_sse_event_conversion_stone() {
        let event = DomainEvent::Stone(StoneEvent::tended(
            "rake",
            "leo-laptop",
            Some("Hello".to_string()),
        ));
        let sse_event = SseEvent::from(&event);

        assert_eq!(sse_event.event_type, event_types::STONE_TENDED);
        assert!(sse_event.message.contains("rake"));
        assert!(sse_event.data.is_some());
    }

    #[tokio::test]
    async fn test_sse_listener_broadcast() {
        let (listener, mut rx) = SseListener::with_channel(16);

        let event = DomainEvent::Offering(OfferingEvent::started("id-1", "mongodb", "stone-01"));
        listener.on_event(&event).await;

        let received = rx.recv().await.unwrap();
        assert_eq!(received.event_type, event_types::SERVICE_STARTED);
        assert_eq!(received.message, "Service mongodb started");
    }
}
