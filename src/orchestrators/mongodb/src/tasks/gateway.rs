//! MongoDB-specific gateway provider for dynamic per-FQN registration.
//!
//! Implements [`GatewayProvider`] on [`AppState`] and delegates to the
//! common `gateway_sync::run` task.
//!
//! Unlike proxy-style orchestrators (Ollama), MongoDB clients connect
//! directly to mongod instances. Each gateway entry's hostname/IP must
//! point to the **actual MongoDB host**, not the orchestrator host.

use crate::app_state::AppState;
use crate::domain::types::{build_connection_string, derive_replica_set_name};
use orchestrator_common::tasks::gateway_sync::{
    FqnGatewayEntry, GatewayProvider, GatewaySyncConfig,
};
use tokio_util::sync::CancellationToken;

impl GatewayProvider for AppState {
    async fn tended_endpoint(&self) -> Option<String> {
        AppState::tended_endpoint(self).await
    }

    async fn fqn_gateway_entries(&self) -> Vec<FqnGatewayEntry> {
        let fqns = self.distinct_fqns().await;
        let replica_sets = self.replica_sets.read().await;
        let instances = self.instances.read().await;
        let mut entries = Vec::with_capacity(fqns.len());

        for fqn in &fqns {
            let rs = replica_sets.get(fqn);
            let rs_name = rs
                .map(|r| r.rs_name.clone())
                .unwrap_or_else(|| derive_replica_set_name(fqn));

            // Find instances for this FQN to get actual host info
            let fqn_instances: Vec<_> = instances.values().filter(|i| i.fqn == *fqn).collect();

            let (port, uri_template, ip) = if let Some(rs) = rs {
                if rs.initialized && rs.members.len() >= 2 {
                    // Full replica set — literal connection string (no {host} substitution)
                    let conn_str = rs
                        .connection_string
                        .clone()
                        .unwrap_or_else(|| build_connection_string(&rs.members, &rs_name));
                    (27017, conn_str, None)
                } else if rs.initialized && rs.members.len() == 1 {
                    // Single-member RS
                    let member = &rs.members[0];
                    let p = extract_port(&member.endpoint);
                    let member_ip = extract_host(&member.endpoint);
                    (
                        p,
                        format!("mongodb://{{host}}:{{port}}/?replicaSet={rs_name}"),
                        Some(member_ip),
                    )
                } else {
                    // Not initialized — use first instance's IP
                    let inst_ip =
                        fqn_instances.first().map(|i| extract_host(&i.mongo_endpoint));
                    (27017, "mongodb://{host}:{port}".to_string(), inst_ip)
                }
            } else {
                // No RS state — use first instance's IP
                let inst_ip = fqn_instances.first().map(|i| extract_host(&i.mongo_endpoint));
                (27017, "mongodb://{host}:{port}".to_string(), inst_ip)
            };

            // Derive hostname from stone name for Koi-resolvable .local address
            let stone_hostname = fqn_instances
                .first()
                .map(|i| format!("{}.local", i.stone_name));

            entries.push(FqnGatewayEntry {
                fqn: fqn.to_string(),
                protocol: "mongodb".to_string(),
                port,
                uri_template,
                hostname: stone_hostname,
                ip,
                category: Some("orchestrator".to_string()),
                tags: vec![],
            });
        }

        entries
    }
}

/// Launch the dynamic gateway sync task for this MongoDB orchestrator.
pub async fn run(
    state: AppState,
    koi_endpoint: String,
    source: String,
    shutdown: CancellationToken,
) {
    let config = GatewaySyncConfig {
        mdns_name: "ZenGarden orchestrator: MongoDB".to_string(),
        offering: "mongodb".to_string(),
        dashboard_port: state.dashboard_port,
        koi_endpoint,
        source,
    };

    orchestrator_common::tasks::gateway_sync::run(config, state, shutdown).await;
}

/// Extract port from a "host:port" string, defaulting to 27017.
fn extract_port(endpoint: &str) -> u16 {
    endpoint
        .rsplit(':')
        .next()
        .and_then(|p| p.parse().ok())
        .unwrap_or(27017)
}

/// Extract host from a "host:port" string.
fn extract_host(endpoint: &str) -> String {
    match endpoint.rsplit_once(':') {
        Some((host, _port)) => host.to_string(),
        None => endpoint.to_string(),
    }
}
