//! Dynamic per-FQN gateway synchronization — ORCH-0004 extension.
//!
//! Registers and maintains **one Moss gateway entry per FQN group** so that
//! `find <offering>` / `find <offering>:<variant>` each resolve to the correct
//! connection string. Also registers one mDNS name for the orchestrator dashboard.
//!
//! Usage: implement [`GatewayProvider`] for your orchestrator's state, then call
//! [`run`] as a background task.
//!
//! ```ignore
//! tokio::spawn(gateway_sync::run(
//!     gateway_sync::GatewaySyncConfig { ... },
//!     state.clone(),
//!     shutdown.clone(),
//! ));
//! ```

use crate::gateway::{GatewayParams, KoiMdnsClient, MossGatewayClient};
use std::collections::HashSet;
use std::future::Future;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// mDNS heartbeat interval.
const HEARTBEAT_SECS: u64 = 30;

/// mDNS lease duration.
const MDNS_LEASE_SECS: u32 = 60;

/// How often we re-scan FQN groups to update gateway registrations.
const FQN_SCAN_SECS: u64 = 15;

/// Configuration for the gateway sync task.
pub struct GatewaySyncConfig {
    /// mDNS service name (e.g. `"mongodb-orchestrator"`).
    pub mdns_name: String,
    /// Offering family (e.g. `"mongodb"`, `"redis"`).
    pub offering: String,
    /// Dashboard port for mDNS (the orchestrator management UI).
    pub dashboard_port: u16,
    /// Koi endpoint URL.
    pub koi_endpoint: String,
    /// Source identifier for gateway entries.
    pub source: String,
}

/// Connection info for a single FQN group, returned by [`GatewayProvider`].
pub struct FqnGatewayEntry {
    /// FQN (e.g. `"mongodb"`, `"mongodb:dev"`). Used as the Moss gateway key.
    pub fqn: String,
    /// Wire protocol (e.g. `"mongodb"`, `"redis"`).
    pub protocol: String,
    /// Port (for {port} template substitution; ignored when uri_template is a literal).
    pub port: u16,
    /// URI template or literal connection string.
    /// - Single instance: `"mongodb://{host}:{port}"`
    /// - Replica set: `"mongodb://h1:27017,h2:27017/?replicaSet=zen-garden"`
    pub uri_template: String,
    /// Hostname of the actual service (for `{host}` substitution).
    /// If `None`, falls back to the orchestrator's own hostname.
    pub hostname: Option<String>,
    /// IP of the actual service. If `None`, falls back to the orchestrator's own IP.
    pub ip: Option<String>,
    /// Category for service discovery (e.g. `"orchestrator"`, `"data"`).
    /// Defaults to `"orchestrator"` if not set.
    pub category: Option<String>,
    /// Tags for service discovery filtering.
    pub tags: Vec<String>,
}

/// Trait for orchestrator state that can provide FQN gateway entries.
///
/// The gateway sync task calls these methods periodically to discover
/// which FQN groups exist and what their connection details are.
pub trait GatewayProvider: Send + Sync + Clone + 'static {
    /// Return the currently tended stone endpoint, if any.
    fn tended_endpoint(&self) -> impl Future<Output = Option<String>> + Send;

    /// Return gateway entries for all current FQN groups.
    ///
    /// Called every ~15 seconds. Return an empty vec if no instances are registered yet.
    fn fqn_gateway_entries(&self) -> impl Future<Output = Vec<FqnGatewayEntry>> + Send;
}

/// Run the dynamic gateway sync lifecycle.
///
/// 1. Register mDNS name with Koi (for orchestrator dashboard)
/// 2. Wait for tended stone
/// 3. Periodically sync per-FQN Moss gateway registrations
/// 4. On shutdown, deregister everything
pub async fn run<P: GatewayProvider>(
    config: GatewaySyncConfig,
    provider: P,
    shutdown: CancellationToken,
) {
    let koi = KoiMdnsClient::new(&config.koi_endpoint);
    let moss = MossGatewayClient::new();

    // ── Phase 1: mDNS announce (orchestrator dashboard) ────────
    let mdns_id = loop {
        if shutdown.is_cancelled() {
            return;
        }

        let txt = garden_common::mdns::build_http_txt(
            &garden_common::mdns::HttpServiceComponent::Orchestrator {
                offering: config.offering.clone(),
            },
            "/",
            env!("CARGO_PKG_VERSION"),
        );

        match koi
            .announce(&config.mdns_name, config.dashboard_port, MDNS_LEASE_SECS, txt)
            .await
        {
            Ok(id) => {
                tracing::info!(
                    mdns_id = %id,
                    name = %config.mdns_name,
                    port = config.dashboard_port,
                    "GatewaySync: mDNS registered"
                );
                break id;
            }
            Err(e) => {
                tracing::warn!(error = %e, "GatewaySync: mDNS announce failed, retrying in 10s");
                tokio::select! {
                    _ = shutdown.cancelled() => return,
                    _ = tokio::time::sleep(Duration::from_secs(10)) => continue,
                }
            }
        }
    };

    // ── Phase 2: Wait for tended stone ─────────────────────────
    let stone_endpoint = loop {
        if shutdown.is_cancelled() {
            deregister_mdns(&koi, &mdns_id).await;
            return;
        }

        if let Some(ep) = provider.tended_endpoint().await {
            tracing::debug!(endpoint = %ep, "GatewaySync: stone available");
            break ep;
        }

        tokio::select! {
            _ = shutdown.cancelled() => {
                deregister_mdns(&koi, &mdns_id).await;
                return;
            }
            _ = tokio::time::sleep(Duration::from_secs(5)) => continue,
        }
    };

    // ── Phase 3: Resolve host identity ─────────────────────────
    let self_ip = garden_common::infra::network::get_local_ip();
    let hostname = match koi.get_hostname().await {
        Ok(h) => {
            tracing::info!(hostname = %h, "GatewaySync: resolved hostname via Koi");
            h
        }
        Err(e) => {
            tracing::warn!(error = %e, fallback = %self_ip, "GatewaySync: hostname lookup failed");
            self_ip.clone()
        }
    };

    // ── Phase 4: Heartbeat + dynamic FQN registration loop ─────
    let mut registered_fqns: HashSet<String> = HashSet::new();
    let mut mdns_tick = tokio::time::interval(Duration::from_secs(HEARTBEAT_SECS));
    let mut fqn_tick = tokio::time::interval(Duration::from_secs(FQN_SCAN_SECS));
    mdns_tick.tick().await;
    fqn_tick.tick().await;

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                tracing::info!("GatewaySync: shutting down, deregistering all");
                deregister_mdns(&koi, &mdns_id).await;
                let current_ep = provider.tended_endpoint().await
                    .unwrap_or_else(|| stone_endpoint.clone());
                for fqn in &registered_fqns {
                    deregister_moss(&moss, &current_ep, fqn).await;
                }
                return;
            }
            _ = mdns_tick.tick() => {
                if let Err(e) = koi.heartbeat(&mdns_id).await {
                    tracing::warn!(error = %e, "GatewaySync: mDNS heartbeat failed");
                }
            }
            _ = fqn_tick.tick() => {
                let current_ep = provider.tended_endpoint().await
                    .unwrap_or_else(|| stone_endpoint.clone());

                sync_fqn_gateways(
                    &provider,
                    &moss,
                    &current_ep,
                    &hostname,
                    &self_ip,
                    &config.source,
                    &mut registered_fqns,
                ).await;
            }
        }
    }
}

/// Synchronize Moss gateway registrations with current FQN groups.
async fn sync_fqn_gateways<P: GatewayProvider>(
    provider: &P,
    moss: &MossGatewayClient,
    stone_endpoint: &str,
    hostname: &str,
    self_ip: &str,
    source: &str,
    registered: &mut HashSet<String>,
) {
    let entries = provider.fqn_gateway_entries().await;
    let current_fqns: HashSet<String> = entries.iter().map(|e| e.fqn.clone()).collect();

    // Deregister stale FQNs
    let stale: Vec<String> = registered.difference(&current_fqns).cloned().collect();
    for fqn in stale {
        tracing::info!(fqn = %fqn, "GatewaySync: deregistering stale FQN");
        deregister_moss(moss, stone_endpoint, &fqn).await;
        registered.remove(&fqn);
    }

    // Register/refresh each current FQN
    for entry in entries {
        // Use per-entry host identity when available (direct-connect services
        // like MongoDB), otherwise fall back to orchestrator's own identity
        // (proxy services like Ollama).
        let entry_hostname = entry.hostname.as_deref().unwrap_or(hostname);
        let entry_ip = entry.ip.as_deref().unwrap_or(self_ip);

        let params = GatewayParams {
            fqn: entry.fqn.clone(),
            hostname: entry_hostname.to_string(),
            ip: entry_ip.to_string(),
            port: entry.port,
            handler_for: vec![entry.fqn.clone()],
            protocol: entry.protocol,
            uri_template: Some(entry.uri_template),
            category: entry.category,
            tags: entry.tags,
            source: source.to_string(),
        };

        match moss.register(stone_endpoint, &entry.fqn, &params).await {
            Ok(_) => {
                if registered.insert(entry.fqn.clone()) {
                    tracing::info!(fqn = %entry.fqn, "GatewaySync: registered FQN gateway");
                }
            }
            Err(e) => {
                tracing::warn!(fqn = %entry.fqn, error = %e, "GatewaySync: FQN registration failed");
            }
        }
    }
}

/// Best-effort mDNS deregistration.
async fn deregister_mdns(koi: &KoiMdnsClient, id: &str) {
    if let Err(e) = koi.unregister(id).await {
        tracing::warn!(error = %e, "GatewaySync: mDNS deregistration failed");
    } else {
        tracing::info!("GatewaySync: mDNS deregistered");
    }
}

/// Best-effort Moss gateway deregistration.
async fn deregister_moss(moss: &MossGatewayClient, stone_endpoint: &str, fqn: &str) {
    if let Err(e) = moss.deregister(stone_endpoint, fqn).await {
        tracing::warn!(fqn = %fqn, error = %e, "GatewaySync: Moss deregistration failed");
    } else {
        tracing::info!(fqn = %fqn, "GatewaySync: Moss gateway deregistered");
    }
}
