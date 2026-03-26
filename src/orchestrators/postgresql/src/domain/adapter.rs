//! PostgreSQL cluster adapter — implements ClusterAdapter for streaming replication.
//!
//! Maps PostgreSQL concepts to the generic cluster primitives:
//!
//! | Generic | PostgreSQL |
//! |---------|-----------|
//! | `probe()` | `SELECT pg_is_in_recovery()` + `pg_stat_replication` |
//! | `bootstrap()` | Configure primary + create replication slot |
//! | `add_member()` | `pg_basebackup` + configure standby + start |
//! | `remove_member()` | Drop replication slot + stop standby |
//! | `health_check()` | `pg_stat_replication` on primary for all standbys |

use orchestrator_common::cluster::{
    ClusterAdapter, InstanceHealth, MemberHealth, ProbeResult,
};

use super::types::PgInstance;

/// PostgreSQL streaming replication adapter.
///
/// Uses `tokio-postgres` for wire-protocol communication.
/// Each method connects to the target instance directly.
pub struct PgAdapter;

impl PgAdapter {
    pub fn new() -> Self {
        Self
    }

    /// Connect to a PostgreSQL instance and check replication status.
    async fn connect_and_probe(endpoint: &str) -> ProbeResult {
        // Parse host:port
        let (host, port) = match endpoint.rsplit_once(':') {
            Some((h, p)) => (h, p.parse::<u16>().unwrap_or(5432)),
            None => (endpoint, 5432),
        };

        let config = format!(
            "host={} port={} user=postgres connect_timeout=5",
            host, port
        );

        let (client, connection) = match tokio_postgres::connect(&config, tokio_postgres::NoTls).await
        {
            Ok(pair) => pair,
            Err(_) => return ProbeResult::Unreachable,
        };

        // Spawn connection handler
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                tracing::debug!(error = %e, "PostgreSQL connection closed");
            }
        });

        // Check if this instance is in recovery (standby) or primary
        match client.query_one("SELECT pg_is_in_recovery()", &[]).await {
            Ok(row) => {
                let in_recovery: bool = row.get(0);
                if in_recovery {
                    // This is a standby — replication is configured
                    ProbeResult::Active
                } else {
                    // This is a primary — check if it has replication slots
                    match client
                        .query(
                            "SELECT slot_name FROM pg_replication_slots WHERE slot_type = 'physical'",
                            &[],
                        )
                        .await
                    {
                        Ok(rows) if !rows.is_empty() => ProbeResult::Active,
                        _ => ProbeResult::NotInitialized,
                    }
                }
            }
            Err(_) => ProbeResult::Unreachable,
        }
    }

    /// Query primary for standby health via pg_stat_replication.
    async fn query_replication_status(endpoint: &str) -> Vec<MemberHealth> {
        let (host, port) = match endpoint.rsplit_once(':') {
            Some((h, p)) => (h, p.parse::<u16>().unwrap_or(5432)),
            None => (endpoint, 5432),
        };

        let config = format!(
            "host={} port={} user=postgres connect_timeout=5",
            host, port
        );

        let (client, connection) =
            match tokio_postgres::connect(&config, tokio_postgres::NoTls).await {
                Ok(pair) => pair,
                Err(_) => return vec![],
            };

        tokio::spawn(async move {
            let _ = connection.await;
        });

        // Primary is always a healthy member
        let mut members = vec![MemberHealth {
            endpoint: endpoint.to_string(),
            stone_name: host.to_string(),
            healthy: true,
            lag_seconds: None,
        }];

        // Query standby status
        let query = r#"
            SELECT client_addr, state,
                   EXTRACT(EPOCH FROM (now() - replay_lag)) as lag_secs
            FROM pg_stat_replication
        "#;

        if let Ok(rows) = client.query(query, &[]).await {
            for row in &rows {
                let addr: Option<std::net::IpAddr> = row.get(0);
                let state: Option<&str> = row.get(1);
                let lag: Option<f64> = row.get(2);

                if let Some(addr) = addr {
                    members.push(MemberHealth {
                        endpoint: format!("{}:{}", addr, port),
                        stone_name: addr.to_string(),
                        healthy: state == Some("streaming"),
                        lag_seconds: lag,
                    });
                }
            }
        }

        members
    }
}

impl ClusterAdapter for PgAdapter {
    type Instance = PgInstance;

    async fn probe(&self, instance: &PgInstance) -> ProbeResult {
        Self::connect_and_probe(&instance.pg_endpoint).await
    }

    async fn bootstrap(&self, set_name: &str, instance: &PgInstance) -> anyhow::Result<()> {
        tracing::info!(
            set_name,
            endpoint = %instance.pg_endpoint,
            "Bootstrapping PostgreSQL primary (creating replication slot)"
        );

        let (host, port) = instance
            .pg_endpoint
            .rsplit_once(':')
            .unwrap_or((&instance.pg_endpoint, "5432"));

        let config = format!(
            "host={} port={} user=postgres connect_timeout=5",
            host, port
        );

        let (client, connection) =
            tokio_postgres::connect(&config, tokio_postgres::NoTls).await?;

        tokio::spawn(async move {
            let _ = connection.await;
        });

        // Create a physical replication slot for the set
        let slot_name = format!("zen_{}", set_name.replace('-', "_"));
        client
            .execute(
                "SELECT pg_create_physical_replication_slot($1) WHERE NOT EXISTS (SELECT 1 FROM pg_replication_slots WHERE slot_name = $1)",
                &[&slot_name],
            )
            .await?;

        tracing::info!(set_name, slot = %slot_name, "PostgreSQL primary bootstrapped");
        Ok(())
    }

    async fn add_member(
        &self,
        set_name: &str,
        instance: &PgInstance,
    ) -> anyhow::Result<()> {
        // In PostgreSQL, adding a standby requires:
        // 1. pg_basebackup from primary to standby
        // 2. Configure standby.signal + primary_conninfo
        // 3. Start PostgreSQL on the standby
        //
        // These are OS-level operations that need Moss exec API — not wire protocol.
        // For now, log the intent. Full implementation needs Moss integration.
        tracing::info!(
            set_name,
            endpoint = %instance.pg_endpoint,
            stone = %instance.stone_name,
            "PostgreSQL add_member: standby setup requires Moss exec API (not yet integrated)"
        );
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
            "PostgreSQL remove_member: drop replication slot + stop standby"
        );
        // Would drop the replication slot on the primary
        // and signal the standby to stop via Moss
        Ok(())
    }

    async fn health_check(&self, set_name: &str) -> Vec<MemberHealth> {
        // In production, would resolve the primary endpoint for this set.
        // For now, this is a structural validation — the trait compiles and
        // the semantics map correctly to PostgreSQL replication.
        tracing::debug!(set_name, "PostgreSQL health_check called");
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn test_instance(endpoint: &str) -> PgInstance {
        PgInstance {
            stone_id: "test-id".into(),
            stone_name: "test-stone".into(),
            moss_endpoint: "http://localhost:7185".into(),
            pg_endpoint: endpoint.into(),
            health: InstanceHealth::Unknown,
            role: None,
            last_seen: Instant::now(),
            server_version: None,
        }
    }

    #[test]
    fn adapter_types_compile() {
        // Validates that PgAdapter satisfies ClusterAdapter trait bounds
        fn assert_adapter<A: ClusterAdapter>() {}
        assert_adapter::<PgAdapter>();
    }

    #[test]
    fn instance_registry_with_pg() {
        // Validates that PgInstance works with generic InstanceRegistry
        use orchestrator_common::cluster::InstanceRegistry;
        let mut reg = InstanceRegistry::<PgInstance>::new();
        let inst = test_instance("10.0.0.1:5432");
        assert!(reg.upsert(inst));
        assert_eq!(reg.len(), 1);
        assert!(reg.get("10.0.0.1:5432").is_some());
    }

    #[test]
    fn logical_set_with_pg_drift() {
        // Validates that LogicalSet drift mapping works for PostgreSQL endpoints
        use orchestrator_common::cluster::{KnownMember, LogicalSet};
        let mut set = LogicalSet::new("pg-primary");
        set.upsert_member(KnownMember {
            stone_name: "stone-db1".into(),
            endpoint: "10.0.0.1:5432".into(),
            member_id: "primary".into(),
        });
        set.upsert_member(KnownMember {
            stone_name: "stone-db2".into(),
            endpoint: "10.0.0.2:5432".into(),
            member_id: "standby-1".into(),
        });

        let current = vec![
            ("stone-db1".into(), "10.0.0.50:5432".into()),
            ("stone-db2".into(), "10.0.0.51:5432".into()),
        ];

        let mapping = set.compute_drift_mapping(&current);
        assert_eq!(mapping.len(), 2);
        assert_eq!(mapping["10.0.0.1:5432"], "10.0.0.50:5432");
    }

    #[test]
    fn classify_probes_with_pg_semantics() {
        use orchestrator_common::cluster::{classify_probes, ProbeResult, SetAction};

        // Two standalones → bootstrap
        let probes = vec![
            ("10.0.0.1:5432".into(), ProbeResult::NotInitialized),
            ("10.0.0.2:5432".into(), ProbeResult::NotInitialized),
        ];
        assert!(matches!(
            classify_probes(&probes, "pg-set", 2),
            SetAction::Bootstrap { .. }
        ));

        // One active (primary) + one unreachable → healthy
        let probes = vec![
            ("10.0.0.1:5432".into(), ProbeResult::Active),
            ("10.0.0.2:5432".into(), ProbeResult::Unreachable),
        ];
        assert!(matches!(
            classify_probes(&probes, "pg-set", 2),
            SetAction::Healthy
        ));
    }
}
