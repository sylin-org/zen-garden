//! Offering Lifecycle Events
//!
//! Unified event types for offering lifecycle changes. These events are emitted
//! by lifecycle operations and consumed by listeners for:
//! - Chirp announcements (UDP broadcast to garden)
//! - SSE events (real-time UI updates)
//! - Timer management (nurturing schedules)
//! - Future: webhooks, audit logging, metrics

use chrono::{DateTime, Utc};
use garden_common::{
    EVENT_DEPLOYED, EVENT_STARTED, EVENT_STOPPED, EVENT_REMOVED,
    EVENT_DESTROYED, EVENT_UPDATED, EVENT_RENAMED, EVENT_HEALTH_CHANGED,
};
use serde::{Deserialize, Serialize};

/// Offering lifecycle event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OfferingEvent {
    /// Offering deployed (container created and started)
    Deployed {
        offering_id: String,
        name: String,
        stone_id: String,
        image: String,
        timestamp: DateTime<Utc>,
    },

    /// Offering started (container started)
    Started {
        offering_id: String,
        name: String,
        stone_id: String,
        timestamp: DateTime<Utc>,
    },

    /// Offering stopped (container stopped)
    Stopped {
        offering_id: String,
        name: String,
        stone_id: String,
        timestamp: DateTime<Utc>,
    },

    /// Offering removed (container deleted, data preserved)
    Removed {
        offering_id: String,
        name: String,
        stone_id: String,
        timestamp: DateTime<Utc>,
    },

    /// Offering destroyed (container + data deleted)
    Destroyed {
        offering_id: String,
        name: String,
        stone_id: String,
        timestamp: DateTime<Utc>,
    },

    /// Offering updated (new image version)
    Updated {
        offering_id: String,
        name: String,
        stone_id: String,
        from_image: String,
        to_image: String,
        timestamp: DateTime<Utc>,
    },

    /// Offering renamed
    Renamed {
        offering_id: String,
        old_name: String,
        new_name: String,
        stone_id: String,
        timestamp: DateTime<Utc>,
    },

    /// Offering health changed
    HealthChanged {
        offering_id: String,
        name: String,
        stone_id: String,
        status: String,
        timestamp: DateTime<Utc>,
    },
}

impl OfferingEvent {
    /// Get the offering_id from any event variant
    pub fn offering_id(&self) -> &str {
        match self {
            Self::Deployed { offering_id, .. } => offering_id,
            Self::Started { offering_id, .. } => offering_id,
            Self::Stopped { offering_id, .. } => offering_id,
            Self::Removed { offering_id, .. } => offering_id,
            Self::Destroyed { offering_id, .. } => offering_id,
            Self::Updated { offering_id, .. } => offering_id,
            Self::Renamed { offering_id, .. } => offering_id,
            Self::HealthChanged { offering_id, .. } => offering_id,
        }
    }

    /// Get the offering name from any event variant
    pub fn name(&self) -> &str {
        match self {
            Self::Deployed { name, .. } => name,
            Self::Started { name, .. } => name,
            Self::Stopped { name, .. } => name,
            Self::Removed { name, .. } => name,
            Self::Destroyed { name, .. } => name,
            Self::Updated { name, .. } => name,
            Self::Renamed { new_name, .. } => new_name,
            Self::HealthChanged { name, .. } => name,
        }
    }

    /// Get the stone_id from any event variant
    pub fn stone_id(&self) -> &str {
        match self {
            Self::Deployed { stone_id, .. } => stone_id,
            Self::Started { stone_id, .. } => stone_id,
            Self::Stopped { stone_id, .. } => stone_id,
            Self::Removed { stone_id, .. } => stone_id,
            Self::Destroyed { stone_id, .. } => stone_id,
            Self::Updated { stone_id, .. } => stone_id,
            Self::Renamed { stone_id, .. } => stone_id,
            Self::HealthChanged { stone_id, .. } => stone_id,
        }
    }

    /// Get event type as a string for logging/display
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::Deployed { .. } => EVENT_DEPLOYED,
            Self::Started { .. } => EVENT_STARTED,
            Self::Stopped { .. } => EVENT_STOPPED,
            Self::Removed { .. } => EVENT_REMOVED,
            Self::Destroyed { .. } => EVENT_DESTROYED,
            Self::Updated { .. } => EVENT_UPDATED,
            Self::Renamed { .. } => EVENT_RENAMED,
            Self::HealthChanged { .. } => EVENT_HEALTH_CHANGED,
        }
    }

    /// Check if this event should trigger a chirp announcement
    pub fn should_chirp(&self) -> bool {
        match self {
            // These change the garden topology
            Self::Deployed { .. } => true,
            Self::Removed { .. } => true,
            Self::Destroyed { .. } => true,
            Self::Renamed { .. } => true,
            Self::Updated { .. } => true,
            Self::HealthChanged { .. } => true,
            // Start/stop don't change topology (service exists but state changes)
            Self::Started { .. } => false,
            Self::Stopped { .. } => false,
        }
    }

    /// Check if this event should trigger timer management
    pub fn should_manage_timers(&self) -> bool {
        match self {
            // Deploy creates timer, remove/destroy deletes it
            Self::Deployed { .. } => true,
            Self::Removed { .. } => true,
            Self::Destroyed { .. } => true,
            Self::Renamed { .. } => true, // May need to rename timer
            // Others don't affect timers
            _ => false,
        }
    }

    /// Create a human-readable message for SSE/logging
    pub fn to_message(&self) -> String {
        match self {
            Self::Deployed { name, .. } => format!("Service {} deployed", name),
            Self::Started { name, .. } => format!("Service {} started", name),
            Self::Stopped { name, .. } => format!("Service {} stopped", name),
            Self::Removed { name, .. } => format!("Service {} removed", name),
            Self::Destroyed { name, .. } => format!("Service {} destroyed", name),
            Self::Updated { name, from_image, to_image, .. } => {
                format!("Service {} updated from {} to {}", name, from_image, to_image)
            }
            Self::Renamed { old_name, new_name, .. } => {
                format!("Service {} renamed to {}", old_name, new_name)
            }
            Self::HealthChanged { name, status, .. } => {
                format!("Service {} health: {}", name, status)
            }
        }
    }
}

/// Builder helpers for creating events with current timestamp
impl OfferingEvent {
    pub fn deployed(offering_id: impl Into<String>, name: impl Into<String>, stone_id: impl Into<String>, image: impl Into<String>) -> Self {
        Self::Deployed {
            offering_id: offering_id.into(),
            name: name.into(),
            stone_id: stone_id.into(),
            image: image.into(),
            timestamp: Utc::now(),
        }
    }

    pub fn started(offering_id: impl Into<String>, name: impl Into<String>, stone_id: impl Into<String>) -> Self {
        Self::Started {
            offering_id: offering_id.into(),
            name: name.into(),
            stone_id: stone_id.into(),
            timestamp: Utc::now(),
        }
    }

    pub fn stopped(offering_id: impl Into<String>, name: impl Into<String>, stone_id: impl Into<String>) -> Self {
        Self::Stopped {
            offering_id: offering_id.into(),
            name: name.into(),
            stone_id: stone_id.into(),
            timestamp: Utc::now(),
        }
    }

    pub fn removed(offering_id: impl Into<String>, name: impl Into<String>, stone_id: impl Into<String>) -> Self {
        Self::Removed {
            offering_id: offering_id.into(),
            name: name.into(),
            stone_id: stone_id.into(),
            timestamp: Utc::now(),
        }
    }

    pub fn destroyed(offering_id: impl Into<String>, name: impl Into<String>, stone_id: impl Into<String>) -> Self {
        Self::Destroyed {
            offering_id: offering_id.into(),
            name: name.into(),
            stone_id: stone_id.into(),
            timestamp: Utc::now(),
        }
    }

    pub fn updated(
        offering_id: impl Into<String>,
        name: impl Into<String>,
        stone_id: impl Into<String>,
        from_image: impl Into<String>,
        to_image: impl Into<String>,
    ) -> Self {
        Self::Updated {
            offering_id: offering_id.into(),
            name: name.into(),
            stone_id: stone_id.into(),
            from_image: from_image.into(),
            to_image: to_image.into(),
            timestamp: Utc::now(),
        }
    }

    pub fn renamed(
        offering_id: impl Into<String>,
        old_name: impl Into<String>,
        new_name: impl Into<String>,
        stone_id: impl Into<String>,
    ) -> Self {
        Self::Renamed {
            offering_id: offering_id.into(),
            old_name: old_name.into(),
            new_name: new_name.into(),
            stone_id: stone_id.into(),
            timestamp: Utc::now(),
        }
    }

    pub fn health_changed(
        offering_id: impl Into<String>,
        name: impl Into<String>,
        stone_id: impl Into<String>,
        status: impl Into<String>,
    ) -> Self {
        Self::HealthChanged {
            offering_id: offering_id.into(),
            name: name.into(),
            stone_id: stone_id.into(),
            status: status.into(),
            timestamp: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_builders() {
        let event = OfferingEvent::deployed("id-1", "mongodb", "stone-01", "mongo:7");
        assert_eq!(event.offering_id(), "id-1");
        assert_eq!(event.name(), "mongodb");
        assert_eq!(event.stone_id(), "stone-01");
        assert_eq!(event.event_type(), "deployed");
        assert!(event.should_chirp());
        assert!(event.should_manage_timers());
    }

    #[test]
    fn test_started_stopped_no_chirp() {
        let started = OfferingEvent::started("id-1", "mongodb", "stone-01");
        let stopped = OfferingEvent::stopped("id-1", "mongodb", "stone-01");

        assert!(!started.should_chirp());
        assert!(!stopped.should_chirp());
        assert!(!started.should_manage_timers());
        assert!(!stopped.should_manage_timers());
    }

    #[test]
    fn test_to_message() {
        let event = OfferingEvent::updated("id-1", "mongodb", "stone-01", "mongo:6", "mongo:7");
        assert_eq!(event.to_message(), "Service mongodb updated from mongo:6 to mongo:7");
    }
}
