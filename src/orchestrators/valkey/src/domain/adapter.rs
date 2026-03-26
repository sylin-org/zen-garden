//! Valkey/Redis cluster adapter — implements ClusterAdapter for Sentinel topology.
//!
//! Maps Valkey/Redis concepts to the generic cluster primitives:
//!
//! | Generic | Valkey/Redis |
//! |---------|-------------|
//! | `probe()` | `INFO REPLICATION` (role: master/slave/standalone) |
//! | `bootstrap()` | Configure Sentinel to monitor the primary |
//! | `add_member()` | `REPLICAOF <primary-host> <primary-port>` |
//! | `remove_member()` | `REPLICAOF NO ONE` + deregister from Sentinel |
//! | `health_check()` | `INFO REPLICATION` on primary + `SENTINEL REPLICAS` |
//!
//! ## Sentinel topology
//!
//! Unlike MongoDB/PostgreSQL where replication is built into the data nodes,
//! Redis uses separate Sentinel processes for monitoring and failover.
//! The orchestrator manages both data nodes and Sentinel instances:
//!
//! ```text
//! Sentinel A ──┐
//! Sentinel B ──┼── monitor ──► Primary ◄── replicates ── Replica 1
//! Sentinel C ──┘                                         Replica 2
//! ```

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

use orchestrator_common::cluster::{
    ClusterAdapter, InstanceHealth, MemberHealth, ProbeResult,
};

use super::types::{parse_info_replication, ValkeyInstance, ValkeyRole};

/// Valkey/Redis Sentinel adapter.
///
/// Uses raw Redis protocol (RESP) over TCP — no driver crate needed.
/// Each command is sent as inline text; responses parsed line-by-line.
pub struct ValkeyAdapter;

impl ValkeyAdapter {
    pub fn new() -> Self {
        Self
    }

    /// Send a Redis command and read the response (inline protocol).
    async fn redis_command(endpoint: &str, command: &str) -> anyhow::Result<String> {
        let stream = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            TcpStream::connect(endpoint),
        )
        .await??;

        let (reader, mut writer) = stream.into_split();
        writer
            .write_all(format!("{}\r\n", command).as_bytes())
            .await?;

        let mut buf_reader = BufReader::new(reader);
        let mut response = String::new();

        // Read bulk response (multi-line for INFO, single line for simple commands)
        loop {
            let mut line = String::new();
            match tokio::time::timeout(
                std::time::Duration::from_secs(3),
                buf_reader.read_line(&mut line),
            )
            .await
            {
                Ok(Ok(0)) => break,        // EOF
                Ok(Ok(_)) => {
                    response.push_str(&line);
                    // For INFO commands, empty line signals end of section
                    if line.trim().is_empty() && !response.trim().is_empty() {
                        break;
                    }
                }
                Ok(Err(_)) | Err(_) => break, // Error or timeout
            }
        }

        Ok(response)
    }

    /// Probe via INFO REPLICATION.
    async fn probe_instance(endpoint: &str) -> ProbeResult {
        match Self::redis_command(endpoint, "INFO REPLICATION").await {
            Ok(info) => {
                let (role, _) = parse_info_replication(&info);
                match role {
                    ValkeyRole::Primary | ValkeyRole::Replica | ValkeyRole::Sentinel => {
                        ProbeResult::Active
                    }
                    ValkeyRole::Standalone => ProbeResult::NotInitialized,
                }
            }
            Err(_) => ProbeResult::Unreachable,
        }
    }
}

impl ClusterAdapter for ValkeyAdapter {
    type Instance = ValkeyInstance;

    async fn probe(&self, instance: &ValkeyInstance) -> ProbeResult {
        Self::probe_instance(&instance.redis_endpoint).await
    }

    async fn bootstrap(&self, set_name: &str, instance: &ValkeyInstance) -> anyhow::Result<()> {
        // The primary is already running — bootstrapping for Sentinel means
        // telling Sentinel to monitor this primary.
        tracing::info!(
            set_name,
            endpoint = %instance.redis_endpoint,
            "Valkey bootstrap: primary is ready, Sentinel monitoring would be configured here"
        );

        // In production: send SENTINEL MONITOR to all Sentinel instances
        // SENTINEL MONITOR <name> <ip> <port> <quorum>
        // For now, the primary just needs to be reachable.
        Ok(())
    }

    async fn add_member(
        &self,
        set_name: &str,
        instance: &ValkeyInstance,
    ) -> anyhow::Result<()> {
        // Tell the new instance to replicate from the primary.
        // This is a single Redis command — no Moss exec API needed.
        tracing::info!(
            set_name,
            endpoint = %instance.redis_endpoint,
            stone = %instance.stone_name,
            "Valkey add_member: would send REPLICAOF to configure replication"
        );

        // In production:
        // 1. Resolve primary endpoint for this set
        // 2. Send: REPLICAOF <primary-host> <primary-port>
        // 3. Sentinel auto-detects new replica
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
            "Valkey remove_member: would send REPLICAOF NO ONE + deregister"
        );

        // In production:
        // 1. Send REPLICAOF NO ONE to the replica (makes it standalone)
        // 2. Send SENTINEL RESET <set_name> to Sentinels (clears stale member)
        Ok(())
    }

    async fn health_check(&self, set_name: &str) -> Vec<MemberHealth> {
        tracing::debug!(set_name, "Valkey health_check called");

        // In production: query INFO REPLICATION on the known primary
        // to get connected_slaves and their lag.
        // Also query SENTINEL REPLICAS <set_name> for Sentinel's view.
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn test_instance(endpoint: &str) -> ValkeyInstance {
        ValkeyInstance {
            stone_id: "test-id".into(),
            stone_name: "test-stone".into(),
            moss_endpoint: "http://localhost:7185".into(),
            redis_endpoint: endpoint.into(),
            health: InstanceHealth::Unknown,
            role: None,
            last_seen: Instant::now(),
            server_version: None,
            connected_replicas: 0,
        }
    }

    #[test]
    fn adapter_types_compile() {
        fn assert_adapter<A: ClusterAdapter>() {}
        assert_adapter::<ValkeyAdapter>();
    }

    #[test]
    fn instance_registry_with_valkey() {
        use orchestrator_common::cluster::InstanceRegistry;
        let mut reg = InstanceRegistry::<ValkeyInstance>::new();
        let inst = test_instance("10.0.0.1:6379");
        assert!(reg.upsert(inst));
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn logical_set_with_valkey_drift() {
        use orchestrator_common::cluster::{KnownMember, LogicalSet};
        let mut set = LogicalSet::new("mymaster");
        set.upsert_member(KnownMember {
            stone_name: "stone-cache1".into(),
            endpoint: "10.0.0.1:6379".into(),
            member_id: "primary".into(),
        });
        set.upsert_member(KnownMember {
            stone_name: "stone-cache2".into(),
            endpoint: "10.0.0.2:6379".into(),
            member_id: "replica-1".into(),
        });

        let current = vec![
            ("stone-cache1".into(), "10.0.0.50:6379".into()),
            ("stone-cache2".into(), "10.0.0.51:6379".into()),
        ];

        let mapping = set.compute_drift_mapping(&current);
        assert_eq!(mapping.len(), 2);
    }

    #[test]
    fn classify_probes_sentinel_topology() {
        use orchestrator_common::cluster::{classify_probes, ProbeResult, SetAction};

        // Two standalone instances — bootstrap
        let probes = vec![
            ("10.0.0.1:6379".into(), ProbeResult::NotInitialized),
            ("10.0.0.2:6379".into(), ProbeResult::NotInitialized),
        ];
        assert!(matches!(
            classify_probes(&probes, "mymaster", 2),
            SetAction::Bootstrap { .. }
        ));

        // Primary active, replica unreachable — still healthy
        let probes = vec![
            ("10.0.0.1:6379".into(), ProbeResult::Active),
            ("10.0.0.2:6379".into(), ProbeResult::Unreachable),
        ];
        assert!(matches!(
            classify_probes(&probes, "mymaster", 2),
            SetAction::Healthy
        ));
    }
}
