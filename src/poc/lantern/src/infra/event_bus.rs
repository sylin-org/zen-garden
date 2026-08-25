//! Event Bus — domain event dispatch for Lantern
//!
//! Mirrors the Moss EventBus pattern: broadcast channel for fan-out delivery
//! of domain events to SSE listeners and other consumers.

use crate::domain::events::DomainEvent;
use serde::Serialize;
use tokio::sync::broadcast;

const EVENT_CHANNEL_CAPACITY: usize = 256;

/// Event bus for Lantern domain events
#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<DomainEvent>,
}

impl EventBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Self { sender }
    }

    /// Emit a domain event to all listeners
    pub fn emit(&self, event: impl Into<DomainEvent>) {
        let event = event.into();
        let event_type = event.event_type().to_string();

        match self.sender.send(event) {
            Ok(count) => {
                tracing::debug!(event_type, receivers = count, "Event emitted");
            }
            Err(_) => {
                tracing::trace!(event_type, "Event emitted (no receivers)");
            }
        }
    }

    /// Subscribe to domain events
    pub fn subscribe(&self) -> broadcast::Receiver<DomainEvent> {
        self.sender.subscribe()
    }

    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// SSE event payload sent to connected dashboard clients
#[derive(Debug, Clone, Serialize)]
pub struct SseEvent {
    pub timestamp: String,
    pub event_type: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stone_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl From<&DomainEvent> for SseEvent {
    fn from(event: &DomainEvent) -> Self {
        use crate::domain::events::{RegistrationEvent, TopologyEvent};

        match event {
            DomainEvent::Registration(reg) => match reg {
                RegistrationEvent::StoneRegistered {
                    stone_name,
                    timestamp,
                    ..
                } => SseEvent {
                    timestamp: timestamp.to_rfc3339(),
                    event_type: "stone.registered".to_string(),
                    message: format!("Stone {} registered", stone_name),
                    stone_name: Some(stone_name.clone()),
                    data: None,
                },
                RegistrationEvent::StoneHeartbeat {
                    stone_name,
                    timestamp,
                } => SseEvent {
                    timestamp: timestamp.to_rfc3339(),
                    event_type: "stone.heartbeat".to_string(),
                    message: format!("Stone {} heartbeat", stone_name),
                    stone_name: Some(stone_name.clone()),
                    data: None,
                },
                RegistrationEvent::StoneOffline {
                    stone_name,
                    timestamp,
                } => SseEvent {
                    timestamp: timestamp.to_rfc3339(),
                    event_type: "stone.offline".to_string(),
                    message: format!("Stone {} went offline", stone_name),
                    stone_name: Some(stone_name.clone()),
                    data: None,
                },
            },
            DomainEvent::Topology(topo) => match topo {
                TopologyEvent::TopologyRefreshed {
                    stones_count,
                    timestamp,
                } => SseEvent {
                    timestamp: timestamp.to_rfc3339(),
                    event_type: "topology.refreshed".to_string(),
                    message: format!("Topology refreshed ({} stones)", stones_count),
                    stone_name: None,
                    data: None,
                },
            },
        }
    }
}
