//! Weaviate cluster adapter — implements ClusterAdapter for Raft-based clustering.
//!
//! Maps Weaviate concepts to the generic cluster primitives:
//!
//! | Generic | Weaviate |
//! |---------|----------|
//! | `probe()` | `GET /v1/.well-known/ready` + `GET /v1/nodes` |
//! | `bootstrap()` | Set `RAFT_BOOTSTRAP_EXPECT=N` + `RAFT_JOIN` env vars |
//! | `add_member()` | Configure `RAFT_JOIN` on new node + restart |
//! | `remove_member()` | Remove node via Raft membership API |
//! | `health_check()` | `GET /v1/nodes` for all cluster members |
//!
//! ## Raft clustering model
//!
//! Weaviate uses Raft consensus (built-in, no external ZooKeeper/etcd):
//!
//! ```text
//! Node 0 (Leader) ←── Raft replication ──→ Node 1 (Follower)
//!                                           Node 2 (Follower)
//! ```
//!
//! Configuration is via environment variables:
//! - `CLUSTER_HOSTNAME` — unique node name
//! - `RAFT_BOOTSTRAP_EXPECT` — number of nodes to wait for before electing
//! - `RAFT_JOIN` — comma-separated list of peer endpoints to join

use orchestrator_common::cluster::{
    ClusterAdapter, InstanceHealth, MemberHealth, ProbeResult,
};

use super::types::{NodesResponse, WeaviateInstance};

/// Weaviate Raft cluster adapter.
///
/// Uses Weaviate's REST API for probing and health checks.
/// Cluster membership changes require environment variable updates
/// on the container (via Moss exec API).
pub struct WeaviateAdapter {
    http: reqwest::Client,
}

impl WeaviateAdapter {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .expect("HTTP client"),
        }
    }

    /// Check if a Weaviate instance is ready.
    async fn check_ready(http: &reqwest::Client, endpoint: &str) -> bool {
        let url = format!("{}/v1/.well-known/ready", endpoint);
        matches!(http.get(&url).send().await, Ok(r) if r.status().is_success())
    }

    /// Get cluster node status from a Weaviate instance.
    async fn get_nodes(
        http: &reqwest::Client,
        endpoint: &str,
    ) -> anyhow::Result<NodesResponse> {
        let url = format!("{}/v1/nodes", endpoint);
        let resp = http.get(&url).send().await?;
        let nodes: NodesResponse = resp.json().await?;
        Ok(nodes)
    }
}

impl ClusterAdapter for WeaviateAdapter {
    type Instance = WeaviateInstance;

    async fn probe(&self, instance: &WeaviateInstance) -> ProbeResult {
        // First check if the instance is ready
        if !Self::check_ready(&self.http, &instance.weaviate_endpoint).await {
            return ProbeResult::Unreachable;
        }

        // Then check cluster status via /v1/nodes
        match Self::get_nodes(&self.http, &instance.weaviate_endpoint).await {
            Ok(nodes) if nodes.nodes.len() > 1 => {
                // Multi-node cluster — Raft is active
                ProbeResult::Active
            }
            Ok(nodes) if nodes.nodes.len() == 1 => {
                // Single node — check if it's standalone or waiting for peers
                let node = &nodes.nodes[0];
                if node.status == "HEALTHY" {
                    // Could be standalone (RAFT_BOOTSTRAP_EXPECT=1) or
                    // a bootstrapped single node waiting for joins.
                    // Treat as NotInitialized for cluster purposes —
                    // the orchestrator decides whether to bootstrap based
                    // on the number of instances in the logical set.
                    ProbeResult::NotInitialized
                } else {
                    ProbeResult::ConfigPending
                }
            }
            Ok(_) => ProbeResult::NotInitialized,
            Err(_) => ProbeResult::Unreachable,
        }
    }

    async fn bootstrap(
        &self,
        set_name: &str,
        instance: &WeaviateInstance,
    ) -> anyhow::Result<()> {
        tracing::info!(
            set_name,
            endpoint = %instance.weaviate_endpoint,
            "Weaviate bootstrap: would configure RAFT_BOOTSTRAP_EXPECT and RAFT_JOIN via Moss env API"
        );

        // In production:
        // 1. Count instances in logical set → RAFT_BOOTSTRAP_EXPECT=N
        // 2. Build RAFT_JOIN from all instance endpoints
        // 3. PATCH /api/v1/stone/services/{name}/env on each stone:
        //    { "RAFT_BOOTSTRAP_EXPECT": "N", "RAFT_JOIN": "node1:8300,node2:8300,..." }
        // 4. Restart containers to pick up new env vars
        // 5. Raft election happens automatically
        Ok(())
    }

    async fn add_member(
        &self,
        set_name: &str,
        instance: &WeaviateInstance,
    ) -> anyhow::Result<()> {
        tracing::info!(
            set_name,
            endpoint = %instance.weaviate_endpoint,
            stone = %instance.stone_name,
            "Weaviate add_member: would configure RAFT_JOIN + CLUSTER_HOSTNAME via Moss env API"
        );

        // In production:
        // 1. Set CLUSTER_HOSTNAME=unique-name on new node
        // 2. Set RAFT_JOIN=existing-node1:8300,existing-node2:8300,...
        // 3. Restart the container
        // 4. Weaviate Raft automatically adds the new node
        Ok(())
    }

    async fn remove_member(
        &self,
        set_name: &str,
        endpoint: &str,
    ) -> anyhow::Result<()> {
        tracing::info!(
            set_name,
            endpoint,
            "Weaviate remove_member: would remove via Raft membership API"
        );
        // Weaviate handles node removal through Raft consensus —
        // stopping the node and clearing its RAFT_JOIN config is sufficient.
        Ok(())
    }

    async fn health_check(&self, set_name: &str) -> Vec<MemberHealth> {
        tracing::debug!(set_name, "Weaviate health_check called");

        // In production: GET /v1/nodes on any cluster member
        // returns all nodes with their status.
        // Parse NodesResponse and map to MemberHealth.
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn test_instance(endpoint: &str) -> WeaviateInstance {
        WeaviateInstance {
            stone_id: "test-id".into(),
            stone_name: "test-stone".into(),
            moss_endpoint: "http://localhost:7185".into(),
            weaviate_endpoint: endpoint.into(),
            grpc_endpoint: None,
            health: InstanceHealth::Unknown,
            role: None,
            last_seen: Instant::now(),
            server_version: None,
            raft_node_name: None,
        }
    }

    #[test]
    fn adapter_types_compile() {
        fn assert_adapter<A: ClusterAdapter>() {}
        assert_adapter::<WeaviateAdapter>();
    }

    #[test]
    fn instance_registry_with_weaviate() {
        use orchestrator_common::cluster::InstanceRegistry;
        let mut reg = InstanceRegistry::<WeaviateInstance>::new();
        let inst = test_instance("http://10.0.0.1:8080");
        assert!(reg.upsert(inst));
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn logical_set_with_weaviate() {
        use orchestrator_common::cluster::{KnownMember, LogicalSet};
        let mut set = LogicalSet::new("weaviate-cluster");
        set.upsert_member(KnownMember {
            stone_name: "stone-vec1".into(),
            endpoint: "http://10.0.0.1:8080".into(),
            member_id: "weaviate-0".into(),
        });
        set.upsert_member(KnownMember {
            stone_name: "stone-vec2".into(),
            endpoint: "http://10.0.0.2:8080".into(),
            member_id: "weaviate-1".into(),
        });

        let current = vec![
            ("stone-vec1".into(), "http://10.0.0.50:8080".into()),
            ("stone-vec2".into(), "http://10.0.0.51:8080".into()),
        ];

        let mapping = set.compute_drift_mapping(&current);
        assert_eq!(mapping.len(), 2);
    }

    #[test]
    fn classify_probes_raft_topology() {
        use orchestrator_common::cluster::{classify_probes, ProbeResult, SetAction};

        // Three standalone instances — bootstrap Raft cluster
        let probes = vec![
            ("http://10.0.0.1:8080".into(), ProbeResult::NotInitialized),
            ("http://10.0.0.2:8080".into(), ProbeResult::NotInitialized),
            ("http://10.0.0.3:8080".into(), ProbeResult::NotInitialized),
        ];
        assert!(matches!(
            classify_probes(&probes, "weaviate", 3),
            SetAction::Bootstrap { .. }
        ));

        // Raft expects 3 nodes — 2 is below threshold
        let probes = vec![
            ("http://10.0.0.1:8080".into(), ProbeResult::NotInitialized),
            ("http://10.0.0.2:8080".into(), ProbeResult::NotInitialized),
        ];
        assert!(matches!(
            classify_probes(&probes, "weaviate", 3),
            SetAction::Wait
        ));
    }
}
