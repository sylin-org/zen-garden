//! Domain Events
//!
//! Unified event types for all domain changes. These events are emitted
//! by lifecycle operations and consumed by listeners for:
//! - Chirp announcements (UDP broadcast to garden)
//! - SSE events (real-time UI updates)
//! - Timer management (nurturing schedules)
//! - Companion notifications (Firefly, Cricket)
//! - Future: webhooks, audit logging, metrics

use chrono::{DateTime, Utc};
use garden_common::{
    EVENT_DEPLOYED, EVENT_STARTED, EVENT_STOPPED, EVENT_REMOVED,
    EVENT_DESTROYED, EVENT_UPDATED, EVENT_RENAMED, EVENT_HEALTH_CHANGED,
    presence::event_types,
};
use serde::{Deserialize, Serialize};

// ============================================================================
// Domain Event (unified wrapper)
// ============================================================================

/// Unified domain event dispatched through the EventBus
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "domain", rename_all = "snake_case")]
pub enum DomainEvent {
    /// Offering lifecycle events (deploy, start, stop, etc.)
    Offering(OfferingEvent),
    /// Storage events (seed bank detected, removed, etc.)
    Storage(StorageEvent),
    /// Stone-level events (tended, health, load)
    Stone(StoneEvent),
}

impl DomainEvent {
    /// Get the event type string for SSE
    pub fn event_type(&self) -> &str {
        match self {
            Self::Offering(e) => e.event_type(),
            Self::Storage(e) => e.event_type(),
            Self::Stone(e) => e.event_type(),
        }
    }

    /// Create a human-readable message
    pub fn to_message(&self) -> String {
        match self {
            Self::Offering(e) => e.to_message(),
            Self::Storage(e) => e.to_message(),
            Self::Stone(e) => e.to_message(),
        }
    }

    /// Check if this event should trigger a chirp announcement
    pub fn should_chirp(&self) -> bool {
        match self {
            Self::Offering(e) => e.should_chirp(),
            Self::Storage(_) => false, // Storage is local-only
            Self::Stone(_) => false,   // Stone events are local-only
        }
    }
}

// Convenience conversions
impl From<OfferingEvent> for DomainEvent {
    fn from(e: OfferingEvent) -> Self {
        Self::Offering(e)
    }
}

impl From<StorageEvent> for DomainEvent {
    fn from(e: StorageEvent) -> Self {
        Self::Storage(e)
    }
}

impl From<StoneEvent> for DomainEvent {
    fn from(e: StoneEvent) -> Self {
        Self::Stone(e)
    }
}

// ============================================================================
// Storage Events
// ============================================================================

/// Storage-related events (seed banks, devices)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StorageEvent {
    /// Seed bank detected and mounted
    SeedBankDetected {
        name: String,
        device: String,
        mount_path: String,
        capacity_gb: u64,
        timestamp: DateTime<Utc>,
    },
    /// Seed bank removed/unmounted
    SeedBankRemoved {
        name: String,
        device: String,
        timestamp: DateTime<Utc>,
    },
    /// Seed bank sync started
    SyncStarted {
        name: String,
        timestamp: DateTime<Utc>,
    },
    /// Seed bank sync completed
    SyncCompleted {
        name: String,
        success: bool,
        timestamp: DateTime<Utc>,
    },
}

impl StorageEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::SeedBankDetected { .. } => event_types::STORAGE_DETECTED,
            Self::SeedBankRemoved { .. } => event_types::STORAGE_REMOVED,
            Self::SyncStarted { .. } => event_types::STORAGE_SYNC_STARTED,
            Self::SyncCompleted { .. } => event_types::STORAGE_SYNC_COMPLETED,
        }
    }

    pub fn to_message(&self) -> String {
        match self {
            Self::SeedBankDetected { name, device, .. } => {
                format!("Seed bank '{}' detected on {}", name, device)
            }
            Self::SeedBankRemoved { name, .. } => {
                format!("Seed bank '{}' removed", name)
            }
            Self::SyncStarted { name, .. } => {
                format!("Seed bank '{}' sync started", name)
            }
            Self::SyncCompleted { name, success, .. } => {
                if *success {
                    format!("Seed bank '{}' sync completed", name)
                } else {
                    format!("Seed bank '{}' sync failed", name)
                }
            }
        }
    }

    // Builder helpers
    pub fn seed_bank_detected(
        name: impl Into<String>,
        device: impl Into<String>,
        mount_path: impl Into<String>,
        capacity_gb: u64,
    ) -> Self {
        Self::SeedBankDetected {
            name: name.into(),
            device: device.into(),
            mount_path: mount_path.into(),
            capacity_gb,
            timestamp: Utc::now(),
        }
    }

    pub fn seed_bank_removed(name: impl Into<String>, device: impl Into<String>) -> Self {
        Self::SeedBankRemoved {
            name: name.into(),
            device: device.into(),
            timestamp: Utc::now(),
        }
    }
}

// ============================================================================
// Stone Events
// ============================================================================

/// Stone-level events (tended, health, load)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StoneEvent {
    /// Stone was tended (user interaction)
    Tended {
        by: String,
        from: String,
        message: Option<String>,
        timestamp: DateTime<Utc>,
    },
    /// Stone health changed
    HealthChanged {
        health: String,
        cpu_percent: f64,
        memory_percent: f64,
        timestamp: DateTime<Utc>,
    },
    /// Stone load updated
    LoadUpdated {
        cpu_percent: f64,
        memory_percent: f64,
        timestamp: DateTime<Utc>,
    },
}

impl StoneEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::Tended { .. } => event_types::STONE_TENDED,
            Self::HealthChanged { .. } => event_types::STONE_HEALTH_CHANGED,
            Self::LoadUpdated { .. } => event_types::STONE_LOAD_UPDATED,
        }
    }

    pub fn to_message(&self) -> String {
        match self {
            Self::Tended { by, .. } => format!("Stone tended by {}", by),
            Self::HealthChanged { health, .. } => format!("Stone health: {}", health),
            Self::LoadUpdated { cpu_percent, memory_percent, .. } => {
                format!("Stone load: CPU {:.0}%, Memory {:.0}%", cpu_percent, memory_percent)
            }
        }
    }

    // Builder helpers
    pub fn tended(by: impl Into<String>, from: impl Into<String>, message: Option<String>) -> Self {
        Self::Tended {
            by: by.into(),
            from: from.into(),
            message,
            timestamp: Utc::now(),
        }
    }

    pub fn health_changed(health: impl Into<String>, cpu_percent: f64, memory_percent: f64) -> Self {
        Self::HealthChanged {
            health: health.into(),
            cpu_percent,
            memory_percent,
            timestamp: Utc::now(),
        }
    }

    pub fn load_updated(cpu_percent: f64, memory_percent: f64) -> Self {
        Self::LoadUpdated {
            cpu_percent,
            memory_percent,
            timestamp: Utc::now(),
        }
    }
}

// ============================================================================
// Offering Events (existing)
// ============================================================================

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
