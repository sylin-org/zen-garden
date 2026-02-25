//! Core domain types for the MongoDB orchestrator.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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
    /// FQN of the offering instance (e.g. `mongodb`, `mongodb:analytics`).
    pub fqn: String,
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
    /// Node is in STARTUP2 or similar initialization state.
    Startup,
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
/// - `"mongodb"` → `"zen-garden"`
/// - `"mongodb:analytics"` → `"zen-garden-analytics"`
/// - `"mongodb:logs"` → `"zen-garden-logs"`
pub fn derive_replica_set_name(fqn: &str) -> String {
    match fqn.strip_prefix("mongodb:") {
        Some(suffix) if !suffix.is_empty() => format!("zen-garden-{suffix}"),
        _ => "zen-garden".to_string(),
    }
}

/// Build a MongoDB connection string from replica set members.
///
/// Format: `mongodb://host1:port1,host2:port2,.../?replicaSet=<rs_name>`
pub fn build_connection_string(members: &[MemberState], rs_name: &str) -> String {
    let hosts: Vec<&str> = members.iter().map(|m| m.endpoint.as_str()).collect();
    format!("mongodb://{}/?replicaSet={}", hosts.join(","), rs_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_replica_set_name() {
        assert_eq!(derive_replica_set_name("mongodb"), "zen-garden");
        assert_eq!(
            derive_replica_set_name("mongodb:analytics"),
            "zen-garden-analytics"
        );
        assert_eq!(derive_replica_set_name("mongodb:logs"), "zen-garden-logs");
        assert_eq!(derive_replica_set_name("mongodb:"), "zen-garden");
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
