//! Connection string publication for clustered services.
//!
//! Each adapter knows how to build a client-facing connection string from
//! its cluster members. The orchestrator publishes this string via the
//! tools stream / gateway so that Moss, Rake, and external clients can
//! discover and connect to the cluster transparently.
//!
//! ## Connection models
//!
//! | Model | Example | Who resolves? |
//! |-------|---------|--------------|
//! | Multi-host driver | `mongodb://h1,h2/?replicaSet=rs0` | Client driver |
//! | Multi-host driver | `postgresql://h1,h2/?target_session_attrs=rw` | libpq |
//! | Sentinel-aware | `redis+sentinel://s1,s2/?service=mymaster` | Redis client |
//! | Any-node | `http://any-node:8080` | Service-internal routing (Raft) |
//! | Proxy | `http://orchestrator:21434` | Orchestrator proxy |

use serde::{Deserialize, Serialize};

use super::logical_set::KnownMember;

/// How clients should connect to this cluster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    /// Primary connection string for read-write access.
    pub connection_string: String,
    /// Protocol scheme (e.g. "mongodb", "postgresql", "redis", "http").
    pub protocol: String,
    /// Whether the connection string includes all members (multi-host)
    /// or points to a single entry point (proxy/any-node).
    pub model: ConnectionModel,
    /// Optional read-only connection string (for read replicas).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only: Option<String>,
}

/// How the client resolves the cluster topology.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionModel {
    /// Connection string lists all members; the client driver handles
    /// failover and primary discovery (MongoDB, PostgreSQL).
    MultiHost,
    /// Connection string uses Sentinel/discovery endpoints; the client
    /// library resolves the current primary (Redis Sentinel).
    SentinelAware,
    /// Any node accepts requests; the service routes internally (Raft).
    AnyNode,
    /// The orchestrator proxies requests on a stable port.
    Proxy,
}

/// Trait for adapters that can publish connection information.
///
/// Implemented alongside `ClusterAdapter` to provide the connection
/// string for a logical set once it's healthy.
pub trait ConnectionPublisher: Send + Sync {
    /// Build connection info for a healthy cluster.
    ///
    /// Returns `None` if the cluster isn't ready (no primary, no members).
    fn connection_info(
        &self,
        set_name: &str,
        members: &[KnownMember],
    ) -> Option<ConnectionInfo>;
}

// ── Built-in helpers ──────────────────────────────────────────────────

/// Build a multi-host MongoDB connection string.
pub fn mongodb_connection(members: &[KnownMember], rs_name: &str) -> ConnectionInfo {
    let hosts: Vec<&str> = members.iter().map(|m| m.endpoint.as_str()).collect();
    ConnectionInfo {
        connection_string: format!(
            "mongodb://{}/?replicaSet={}",
            hosts.join(","),
            rs_name
        ),
        protocol: "mongodb".into(),
        model: ConnectionModel::MultiHost,
        read_only: None,
    }
}

/// Build a multi-host PostgreSQL connection string.
pub fn postgresql_connection(members: &[KnownMember]) -> ConnectionInfo {
    let hosts: Vec<&str> = members.iter().map(|m| m.endpoint.as_str()).collect();
    ConnectionInfo {
        connection_string: format!(
            "postgresql://{}/?target_session_attrs=read-write",
            hosts.join(",")
        ),
        protocol: "postgresql".into(),
        model: ConnectionModel::MultiHost,
        read_only: Some(format!(
            "postgresql://{}/?target_session_attrs=any",
            hosts.join(",")
        )),
    }
}

/// Build a Sentinel-aware Redis/Valkey connection string.
pub fn valkey_connection(
    sentinels: &[KnownMember],
    set_name: &str,
) -> ConnectionInfo {
    let sentinel_hosts: Vec<&str> = sentinels.iter().map(|m| m.endpoint.as_str()).collect();
    ConnectionInfo {
        connection_string: format!(
            "redis+sentinel://{}/?sentinelServiceName={}",
            sentinel_hosts.join(","),
            set_name
        ),
        protocol: "redis".into(),
        model: ConnectionModel::SentinelAware,
        read_only: None,
    }
}

/// Build an any-node connection (Weaviate, services with internal routing).
pub fn any_node_connection(
    members: &[KnownMember],
    protocol: &str,
) -> ConnectionInfo {
    // Pick the first healthy member as the primary endpoint.
    // The service handles internal routing.
    let primary = members
        .first()
        .map(|m| m.endpoint.as_str())
        .unwrap_or("localhost");
    ConnectionInfo {
        connection_string: primary.to_string(),
        protocol: protocol.into(),
        model: ConnectionModel::AnyNode,
        read_only: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn members(endpoints: &[&str]) -> Vec<KnownMember> {
        endpoints
            .iter()
            .enumerate()
            .map(|(i, ep)| KnownMember {
                stone_name: format!("stone-{}", i),
                endpoint: ep.to_string(),
                member_id: i.to_string(),
            })
            .collect()
    }

    #[test]
    fn mongodb_conn_string() {
        let m = members(&["10.0.0.1:27017", "10.0.0.2:27017"]);
        let info = mongodb_connection(&m, "zen-garden");
        assert_eq!(
            info.connection_string,
            "mongodb://10.0.0.1:27017,10.0.0.2:27017/?replicaSet=zen-garden"
        );
        assert_eq!(info.model, ConnectionModel::MultiHost);
        assert!(info.read_only.is_none());
    }

    #[test]
    fn postgresql_conn_string() {
        let m = members(&["10.0.0.1:5432", "10.0.0.2:5432"]);
        let info = postgresql_connection(&m);
        assert!(info.connection_string.contains("target_session_attrs=read-write"));
        assert_eq!(info.model, ConnectionModel::MultiHost);
        assert!(info.read_only.is_some());
        assert!(info.read_only.unwrap().contains("target_session_attrs=any"));
    }

    #[test]
    fn valkey_conn_string() {
        let s = members(&["10.0.0.1:26379", "10.0.0.2:26379"]);
        let info = valkey_connection(&s, "mymaster");
        assert!(info.connection_string.contains("redis+sentinel://"));
        assert!(info.connection_string.contains("sentinelServiceName=mymaster"));
        assert_eq!(info.model, ConnectionModel::SentinelAware);
    }

    #[test]
    fn weaviate_any_node() {
        let m = members(&["http://10.0.0.1:8080", "http://10.0.0.2:8080"]);
        let info = any_node_connection(&m, "http");
        assert_eq!(info.connection_string, "http://10.0.0.1:8080");
        assert_eq!(info.model, ConnectionModel::AnyNode);
    }

    #[test]
    fn empty_members_any_node() {
        let info = any_node_connection(&[], "http");
        assert_eq!(info.connection_string, "localhost");
    }
}
