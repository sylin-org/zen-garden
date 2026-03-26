//! PostgreSQL-specific domain types.

use std::time::Instant;

use orchestrator_common::cluster::InstanceHealth;
use serde::{Deserialize, Serialize};

/// A discovered PostgreSQL instance running on a stone.
#[derive(Debug, Clone)]
pub struct PgInstance {
    pub stone_id: String,
    pub stone_name: String,
    pub moss_endpoint: String,
    /// PostgreSQL connection endpoint (e.g. `"192.168.1.5:5432"`).
    pub pg_endpoint: String,
    pub health: InstanceHealth,
    /// Streaming replication role.
    pub role: Option<PgRole>,
    pub last_seen: Instant,
    /// PostgreSQL server version (e.g. "16.2").
    pub server_version: Option<String>,
}

impl orchestrator_common::cluster::ClusterInstance for PgInstance {
    fn endpoint(&self) -> &str {
        &self.pg_endpoint
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

/// PostgreSQL replication role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PgRole {
    /// Read-write primary (accepts writes, streams WAL to standbys).
    Primary,
    /// Hot standby (read-only, receives WAL stream from primary).
    Standby,
    /// Not yet configured for replication.
    Standalone,
}

impl std::fmt::Display for PgRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Primary => write!(f, "primary"),
            Self::Standby => write!(f, "standby"),
            Self::Standalone => write!(f, "standalone"),
        }
    }
}

/// Build a PostgreSQL connection string from primary + standbys.
///
/// Uses `target_session_attrs=read-write` for automatic primary routing.
pub fn build_connection_string(primary: &str, standbys: &[&str]) -> String {
    let mut hosts = vec![primary];
    hosts.extend(standbys);
    format!(
        "postgresql://{}/?target_session_attrs=read-write",
        hosts.join(",")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_string_single() {
        let conn = build_connection_string("10.0.0.1:5432", &[]);
        assert_eq!(
            conn,
            "postgresql://10.0.0.1:5432/?target_session_attrs=read-write"
        );
    }

    #[test]
    fn connection_string_multi() {
        let conn = build_connection_string("10.0.0.1:5432", &["10.0.0.2:5432", "10.0.0.3:5432"]);
        assert!(conn.contains("10.0.0.1:5432,10.0.0.2:5432,10.0.0.3:5432"));
        assert!(conn.contains("target_session_attrs=read-write"));
    }

    #[test]
    fn cluster_instance_trait() {
        let inst = PgInstance {
            stone_id: "id-1".into(),
            stone_name: "stone-a".into(),
            moss_endpoint: "http://10.0.0.1:7185".into(),
            pg_endpoint: "10.0.0.1:5432".into(),
            health: InstanceHealth::Healthy,
            role: Some(PgRole::Primary),
            last_seen: Instant::now(),
            server_version: Some("16.2".into()),
        };

        use orchestrator_common::cluster::ClusterInstance;
        assert_eq!(inst.endpoint(), "10.0.0.1:5432");
        assert_eq!(inst.stone_name(), "stone-a");
        assert_eq!(*inst.health(), InstanceHealth::Healthy);
    }
}
