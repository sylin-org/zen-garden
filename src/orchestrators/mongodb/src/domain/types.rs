//! Core domain types for the MongoDB orchestrator.

use crate::domain::cache_advisor::{CacheHealth, CacheRecommendation, CacheStatus};
use crate::domain::oplog::OplogHealth;
use chrono::{DateTime, Utc};
use garden_common::offerings::OfferingFqn;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Instant;

/// A discovered MongoDB instance running on a stone.
#[derive(Debug, Clone)]
pub struct MongoInstance {
    /// Unique stone identifier (GUIDv7).
    pub stone_id: String,
    /// Human-readable stone name.
    pub stone_name: String,
    /// Moss API endpoint (e.g. `http://192.168.1.5:7185`).
    pub moss_endpoint: String,
    /// MongoDB wire protocol endpoint (e.g. `192.168.1.5:27017`).
    pub mongo_endpoint: String,
    /// FQN of the offering instance (e.g. `mongodb`, `mongodb::analytics`).
    pub fqn: OfferingFqn,
    /// Current health status.
    pub health: InstanceHealth,
    /// Replica set role (if known from rs.status()).
    pub role: Option<ReplicaRole>,
    /// Last time we saw this instance.
    pub last_seen: Instant,
}

/// Health status of a MongoDB instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceHealth {
    /// Instance is healthy and responding to commands.
    Healthy,
    /// Instance was discovered but not yet probed.
    Unknown,
    /// Instance is not responding.
    Unreachable,
    /// Instance is responding but in a degraded state.
    Degraded,
    /// Container is intentionally stopped on the stone (tools stream: ready = false).
    /// Distinct from Unreachable — the offering exists but the container is down.
    Stopped,
}

/// Replica set role for a MongoDB member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ReplicaRole {
    Primary,
    Secondary,
    Arbiter,
    /// Recovering (e.g. initial sync, rollback).
    Recovering,
    /// Node is in STARTUP or STARTUP2 initialization state.
    Startup,
    /// Member is DOWN — unreachable from the reporting member's perspective.
    Down,
    /// Member is performing a rollback.
    Rollback,
    /// Member has been removed from the replica set config.
    Removed,
    /// Role could not be determined.
    Unknown,
}

impl std::fmt::Display for ReplicaRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Primary => write!(f, "PRIMARY"),
            Self::Secondary => write!(f, "SECONDARY"),
            Self::Arbiter => write!(f, "ARBITER"),
            Self::Recovering => write!(f, "RECOVERING"),
            Self::Startup => write!(f, "STARTUP"),
            Self::Down => write!(f, "DOWN"),
            Self::Rollback => write!(f, "ROLLBACK"),
            Self::Removed => write!(f, "REMOVED"),
            Self::Unknown => write!(f, "UNKNOWN"),
        }
    }
}

/// State of a single replica set (one per FQN).
#[derive(Debug, Clone, Serialize)]
pub struct ReplicaSetState {
    /// Replica set name (derived from FQN, e.g. "zen-garden" or "zen-garden-analytics").
    pub rs_name: String,
    /// Whether the replica set has been initialized via rs.initiate().
    pub initialized: bool,
    /// Current members as reported by rs.status().
    pub members: Vec<MemberState>,
    /// Computed connection string for clients.
    pub connection_string: Option<String>,
    /// Last time rs.status() was successfully queried.
    pub last_updated: DateTime<Utc>,
    /// WiredTiger cache status (populated by health monitor).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache: Option<CacheSnapshot>,
    /// Oplog health (populated by health monitor).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oplog: Option<OplogHealth>,
}

/// WiredTiger cache snapshot — status + evaluated recommendations.
#[derive(Debug, Clone, Serialize)]
pub struct CacheSnapshot {
    pub status: CacheStatus,
    pub health: CacheHealth,
    pub recommendations: Vec<CacheRecommendation>,
}

/// State of a single member within a replica set (from rs.status()).
#[derive(Debug, Clone, Serialize)]
pub struct MemberState {
    /// MongoDB wire protocol endpoint (e.g. "192.168.1.5:27017").
    pub endpoint: String,
    /// Human-readable stone name hosting this member.
    pub stone_name: String,
    /// Replica set role.
    pub role: ReplicaRole,
    /// Whether this member is healthy (from rs.status() health field).
    pub healthy: bool,
    /// Replication lag in seconds (for secondaries).
    pub lag_seconds: Option<f64>,
    /// Last heartbeat received from this member.
    pub last_heartbeat: Option<DateTime<Utc>>,
}

/// Derives a replica set name from a MongoDB FQN.
///
/// - `mongodb` (no instance) → `"zen-garden"`
/// - `mongodb::analytics`    → `"zen-garden-analytics"`
/// - `mongodb::logs`         → `"zen-garden-logs"`
pub fn derive_replica_set_name(fqn: &OfferingFqn) -> String {
    match &fqn.instance {
        Some(instance) => format!("zen-garden-{instance}"),
        None => "zen-garden".to_string(),
    }
}

/// Build a MongoDB connection string from replica set members.
///
/// Format: `mongodb://host1:port1,host2:port2,.../?replicaSet=<rs_name>`
pub fn build_connection_string(members: &[MemberState], rs_name: &str) -> String {
    let hosts: Vec<&str> = members.iter().map(|m| m.endpoint.as_str()).collect();
    format!("mongodb://{}/?replicaSet={}", hosts.join(","), rs_name)
}

/// A pending membership action queued for eventual execution.
///
/// Actions are persisted to disk so they survive orchestrator restart.
/// The bootstrap task executor checks the queue each cycle and executes
/// actions when the target becomes reachable.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum PendingAction {
    /// Remove a member from the replica set and logical set.
    RemoveMember {
        /// MongoDB wire endpoint (e.g. "stone-quartz-fen.local:27017").
        mongo_endpoint: String,
        /// FQN of the logical set (e.g. "mongodb", "mongodb::analytics").
        fqn: OfferingFqn,
        /// When the action was requested.
        requested_at: DateTime<Utc>,
    },
}

impl PendingAction {
    /// The target endpoint this action applies to.
    pub fn target_endpoint(&self) -> &str {
        match self {
            PendingAction::RemoveMember { mongo_endpoint, .. } => mongo_endpoint,
        }
    }
}

/// Load pending actions from the data directory.
pub async fn load_pending_actions(data_dir: &str) -> Vec<PendingAction> {
    let path = Path::new(data_dir).join("pending-actions.json");
    match tokio::fs::read_to_string(&path).await {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => vec![],
    }
}

/// Persist pending actions to the data directory.
pub async fn save_pending_actions(data_dir: &str, actions: &[PendingAction]) {
    let path = Path::new(data_dir).join("pending-actions.json");
    match serde_json::to_string_pretty(actions) {
        Ok(json) => {
            if let Err(e) = tokio::fs::write(&path, json).await {
                tracing::warn!(error = %e, "failed to persist pending actions");
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to serialize pending actions");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_replica_set_name() {
        let fqn_default = OfferingFqn::new("mongodb").unwrap();
        assert_eq!(derive_replica_set_name(&fqn_default), "zen-garden");

        let fqn_analytics = OfferingFqn::with_instance("mongodb", "analytics").unwrap();
        assert_eq!(
            derive_replica_set_name(&fqn_analytics),
            "zen-garden-analytics"
        );

        let fqn_logs = OfferingFqn::with_instance("mongodb", "logs").unwrap();
        assert_eq!(derive_replica_set_name(&fqn_logs), "zen-garden-logs");
    }

    #[test]
    fn test_build_connection_string() {
        let members = vec![
            MemberState {
                endpoint: "192.168.1.5:27017".into(),
                stone_name: "stone-a".into(),
                role: ReplicaRole::Primary,
                healthy: true,
                lag_seconds: None,
                last_heartbeat: None,
            },
            MemberState {
                endpoint: "192.168.1.6:27017".into(),
                stone_name: "stone-b".into(),
                role: ReplicaRole::Secondary,
                healthy: true,
                lag_seconds: Some(0.5),
                last_heartbeat: None,
            },
        ];

        let conn = build_connection_string(&members, "zen-garden");
        assert_eq!(
            conn,
            "mongodb://192.168.1.5:27017,192.168.1.6:27017/?replicaSet=zen-garden"
        );
    }
}
