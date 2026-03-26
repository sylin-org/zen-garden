//! Valkey/Redis-specific domain types.

use std::time::Instant;

use orchestrator_common::cluster::InstanceHealth;
use serde::{Deserialize, Serialize};

/// A discovered Valkey/Redis instance running on a stone.
#[derive(Debug, Clone)]
pub struct ValkeyInstance {
    pub stone_id: String,
    pub stone_name: String,
    pub moss_endpoint: String,
    /// Redis wire protocol endpoint (e.g. `"192.168.1.5:6379"`).
    pub redis_endpoint: String,
    pub health: InstanceHealth,
    /// Replication role.
    pub role: Option<ValkeyRole>,
    pub last_seen: Instant,
    /// Server version (e.g. "7.2.6", "8.0.0" for Valkey).
    pub server_version: Option<String>,
    /// Number of connected replicas (primary only).
    pub connected_replicas: u32,
}

impl orchestrator_common::cluster::ClusterInstance for ValkeyInstance {
    fn endpoint(&self) -> &str {
        &self.redis_endpoint
    }

    fn stone_id(&self) -> &str {
        &self.stone_id
    }

    fn stone_name(&self) -> &str {
        &self.stone_name
    }

    fn health(&self) -> &InstanceHealth {
        &self.health
    }
}

/// Valkey/Redis replication role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValkeyRole {
    /// Primary (master) — accepts reads and writes.
    Primary,
    /// Replica (slave) — read-only, replicates from primary.
    Replica,
    /// Sentinel — monitors primaries and orchestrates failover.
    Sentinel,
    /// Standalone — not configured for replication.
    Standalone,
}

impl std::fmt::Display for ValkeyRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Primary => write!(f, "primary"),
            Self::Replica => write!(f, "replica"),
            Self::Sentinel => write!(f, "sentinel"),
            Self::Standalone => write!(f, "standalone"),
        }
    }
}

/// Parse the `INFO REPLICATION` response from Redis/Valkey.
///
/// Returns the role and connected_slaves count.
pub fn parse_info_replication(response: &str) -> (ValkeyRole, u32) {
    let mut role = ValkeyRole::Standalone;
    let mut connected_slaves = 0u32;

    for line in response.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("role:") {
            role = match value {
                "master" => ValkeyRole::Primary,
                "slave" => ValkeyRole::Replica,
                "sentinel" => ValkeyRole::Sentinel,
                _ => ValkeyRole::Standalone,
            };
        }
        if let Some(value) = line.strip_prefix("connected_slaves:") {
            connected_slaves = value.parse().unwrap_or(0);
        }
    }

    (role, connected_slaves)
}

/// Build a Redis connection string with Sentinel support.
pub fn build_connection_string(primary: &str, sentinels: &[&str], set_name: &str) -> String {
    if sentinels.is_empty() {
        format!("redis://{}", primary)
    } else {
        // Sentinel-aware connection format
        let sentinel_list: Vec<String> = sentinels
            .iter()
            .map(|s| format!("sentinel://{}", s))
            .collect();
        format!(
            "redis+sentinel://{}/?sentinelServiceName={}",
            sentinel_list.join(","),
            set_name
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_primary_info() {
        let info = "# Replication\r\nrole:master\r\nconnected_slaves:2\r\n";
        let (role, slaves) = parse_info_replication(info);
        assert_eq!(role, ValkeyRole::Primary);
        assert_eq!(slaves, 2);
    }

    #[test]
    fn parse_replica_info() {
        let info = "# Replication\r\nrole:slave\r\nconnected_slaves:0\r\n";
        let (role, slaves) = parse_info_replication(info);
        assert_eq!(role, ValkeyRole::Replica);
        assert_eq!(slaves, 0);
    }

    #[test]
    fn parse_unknown_role() {
        let info = "# Replication\r\nrole:unknown\r\n";
        let (role, _) = parse_info_replication(info);
        assert_eq!(role, ValkeyRole::Standalone);
    }

    #[test]
    fn connection_string_direct() {
        let conn = build_connection_string("10.0.0.1:6379", &[], "mymaster");
        assert_eq!(conn, "redis://10.0.0.1:6379");
    }

    #[test]
    fn connection_string_sentinel() {
        let conn = build_connection_string(
            "10.0.0.1:6379",
            &["10.0.0.1:26379", "10.0.0.2:26379"],
            "mymaster",
        );
        assert!(conn.contains("sentinel://"));
        assert!(conn.contains("sentinelServiceName=mymaster"));
    }

    #[test]
    fn cluster_instance_trait() {
        let inst = ValkeyInstance {
            stone_id: "id-1".into(),
            stone_name: "stone-a".into(),
            moss_endpoint: "http://10.0.0.1:7185".into(),
            redis_endpoint: "10.0.0.1:6379".into(),
            health: InstanceHealth::Healthy,
            role: Some(ValkeyRole::Primary),
            last_seen: Instant::now(),
            server_version: Some("8.0.0".into()),
            connected_replicas: 1,
        };

        use orchestrator_common::cluster::ClusterInstance;
        assert_eq!(inst.endpoint(), "10.0.0.1:6379");
        assert_eq!(inst.stone_name(), "stone-a");
    }
}
