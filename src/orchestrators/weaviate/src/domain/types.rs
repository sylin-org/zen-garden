//! Weaviate-specific domain types.

use std::time::Instant;

use orchestrator_common::cluster::InstanceHealth;
use serde::{Deserialize, Serialize};

/// A discovered Weaviate instance running on a stone.
#[derive(Debug, Clone)]
pub struct WeaviateInstance {
    pub stone_id: String,
    pub stone_name: String,
    pub moss_endpoint: String,
    /// Weaviate REST API endpoint (e.g. `"http://192.168.1.5:8080"`).
    pub weaviate_endpoint: String,
    /// Weaviate gRPC endpoint (e.g. `"192.168.1.5:50051"`).
    pub grpc_endpoint: Option<String>,
    pub health: InstanceHealth,
    /// Raft cluster role.
    pub role: Option<WeaviateRole>,
    pub last_seen: Instant,
    /// Weaviate server version (e.g. "1.29.0").
    pub server_version: Option<String>,
    /// Raft node name (hostname used for cluster identity).
    pub raft_node_name: Option<String>,
}

impl orchestrator_common::cluster::ClusterInstance for WeaviateInstance {
    fn endpoint(&self) -> &str {
        &self.weaviate_endpoint
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

/// Weaviate cluster role (Raft-based).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WeaviateRole {
    /// Raft leader — handles writes, replicates to followers.
    Leader,
    /// Raft follower — receives replicated writes from leader.
    Follower,
    /// Joining — node is joining the cluster but hasn't completed sync.
    Joining,
    /// Standalone — single-node mode (RAFT_BOOTSTRAP_EXPECT=1).
    Standalone,
}

impl std::fmt::Display for WeaviateRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Leader => write!(f, "leader"),
            Self::Follower => write!(f, "follower"),
            Self::Joining => write!(f, "joining"),
            Self::Standalone => write!(f, "standalone"),
        }
    }
}

/// Weaviate node status from the `/v1/nodes` REST API.
#[derive(Debug, Clone, Deserialize)]
pub struct NodeStatus {
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub shards: Option<Vec<ShardStatus>>,
    #[serde(rename = "gitHash")]
    pub git_hash: Option<String>,
    pub version: Option<String>,
}

/// Shard status within a Weaviate node.
#[derive(Debug, Clone, Deserialize)]
pub struct ShardStatus {
    pub name: String,
    pub class: String,
    #[serde(rename = "objectCount")]
    pub object_count: Option<u64>,
}

/// Response from `GET /v1/nodes`.
#[derive(Debug, Clone, Deserialize)]
pub struct NodesResponse {
    pub nodes: Vec<NodeStatus>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cluster_instance_trait() {
        let inst = WeaviateInstance {
            stone_id: "id-1".into(),
            stone_name: "stone-a".into(),
            moss_endpoint: "http://10.0.0.1:7185".into(),
            weaviate_endpoint: "http://10.0.0.1:8080".into(),
            grpc_endpoint: Some("10.0.0.1:50051".into()),
            health: InstanceHealth::Healthy,
            role: Some(WeaviateRole::Leader),
            last_seen: Instant::now(),
            server_version: Some("1.29.0".into()),
            raft_node_name: Some("weaviate-node-0".into()),
        };

        use orchestrator_common::cluster::ClusterInstance;
        assert_eq!(inst.endpoint(), "http://10.0.0.1:8080");
        assert_eq!(inst.stone_name(), "stone-a");
        assert_eq!(*inst.health(), InstanceHealth::Healthy);
    }

    #[test]
    fn parse_nodes_response() {
        let json = r#"{"nodes":[{"name":"weaviate-0","status":"HEALTHY","version":"1.29.0"}]}"#;
        let resp: NodesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.nodes.len(), 1);
        assert_eq!(resp.nodes[0].name, "weaviate-0");
        assert_eq!(resp.nodes[0].status, "HEALTHY");
    }
}
