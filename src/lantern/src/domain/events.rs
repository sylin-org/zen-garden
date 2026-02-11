//! Domain events for Lantern
//!
//! These events represent state changes in the garden as observed by Lantern.
//! They are emitted by registration, aggregation, and topology operations,
//! and consumed by SSE listeners for real-time dashboard updates.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Unified domain event for Lantern
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "domain", rename_all = "snake_case")]
pub enum DomainEvent {
    /// Stone registration/heartbeat events
    Registration(RegistrationEvent),
    /// Garden-wide topology changes
    Topology(TopologyEvent),
}

impl DomainEvent {
    pub fn event_type(&self) -> &str {
        match self {
            Self::Registration(e) => e.event_type(),
            Self::Topology(e) => e.event_type(),
        }
    }
}

impl From<RegistrationEvent> for DomainEvent {
    fn from(e: RegistrationEvent) -> Self {
        Self::Registration(e)
    }
}

impl From<TopologyEvent> for DomainEvent {
    fn from(e: TopologyEvent) -> Self {
        Self::Topology(e)
    }
}

/// Stone registration lifecycle events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RegistrationEvent {
    /// New stone registered for the first time
    StoneRegistered {
        stone_id: Option<String>,
        stone_name: String,
        endpoint: String,
        timestamp: DateTime<Utc>,
    },
    /// Existing stone sent heartbeat
    StoneHeartbeat {
        stone_name: String,
        timestamp: DateTime<Utc>,
    },
    /// Stone went offline (TTL expired)
    StoneOffline {
        stone_name: String,
        timestamp: DateTime<Utc>,
    },
}

impl RegistrationEvent {
    pub fn event_type(&self) -> &str {
        match self {
            Self::StoneRegistered { .. } => "stone.registered",
            Self::StoneHeartbeat { .. } => "stone.heartbeat",
            Self::StoneOffline { .. } => "stone.offline",
        }
    }

    pub fn stone_registered(
        stone_id: Option<String>,
        stone_name: impl Into<String>,
        endpoint: impl Into<String>,
    ) -> Self {
        Self::StoneRegistered {
            stone_id,
            stone_name: stone_name.into(),
            endpoint: endpoint.into(),
            timestamp: Utc::now(),
        }
    }

    pub fn stone_heartbeat(stone_name: impl Into<String>) -> Self {
        Self::StoneHeartbeat {
            stone_name: stone_name.into(),
            timestamp: Utc::now(),
        }
    }

    pub fn stone_offline(stone_name: impl Into<String>) -> Self {
        Self::StoneOffline {
            stone_name: stone_name.into(),
            timestamp: Utc::now(),
        }
    }
}

/// Garden topology change events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TopologyEvent {
    /// Topology was refreshed from aggregation
    TopologyRefreshed {
        stones_count: usize,
        timestamp: DateTime<Utc>,
    },
}

impl TopologyEvent {
    pub fn event_type(&self) -> &str {
        match self {
            Self::TopologyRefreshed { .. } => "topology.refreshed",
        }
    }

    pub fn topology_refreshed(stones_count: usize) -> Self {
        Self::TopologyRefreshed {
            stones_count,
            timestamp: Utc::now(),
        }
    }
}
