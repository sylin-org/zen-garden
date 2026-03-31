//! Gateway announcement task — event-driven registration.
//!
//! Registers offering FQNs with Moss ONLY when healthy instances exist.
//! Deregisters when the last instance of an offering goes offline.
//!
//! Architecture:
//! 1. Register mDNS name with Koi (hostname becomes resolvable)
//! 2. Subscribe to registry.updated events from the dashboard channel
//! 3. On each event, diff current offerings vs registered offerings
//! 4. Register new, deregister removed
//! 5. Periodic heartbeat for mDNS lease renewal
//! 6. Graceful deregister on shutdown

use crate::AppState;
use orchestrator_common::gateway::{GatewayParams, KoiMdnsClient, MossGatewayClient};
use std::collections::HashSet;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// mDNS lease TTL (seconds).
const MDNS_LEASE_SECS: u32 = 60;

/// mDNS heartbeat interval — must be less than lease TTL.
const MDNS_HEARTBEAT_SECS: u64 = 30;

/// mDNS service name for the AI orchestrator.
const MDNS_NAME: &str = "ZenGarden orchestrator: AI";

pub async fn run(state: AppState, shutdown: CancellationToken) {
    let koi = KoiMdnsClient::new(&state.koi_endpoint);
    let moss_gw = MossGatewayClient::new();

    // ── Phase 1: mDNS announce via Koi ──────────────────────────
    let mdns_id = loop {
        if shutdown.is_cancelled() {
            return;
        }

        let txt = garden_common::mdns::build_http_txt(
            &garden_common::mdns::HttpServiceComponent::Orchestrator {
                offering: "ai".to_string(),
            },
            "/",
            env!("CARGO_PKG_VERSION"),
        );

        match koi
            .announce(MDNS_NAME, state.dashboard_port, MDNS_LEASE_SECS, txt)
            .await
        {
            Ok(id) => {
                tracing::info!(
                    mdns_id = %id,
                    name = MDNS_NAME,
                    port = state.dashboard_port,
                    "Gateway: mDNS registered via Koi"
                );
                break id;
            }
            Err(e) => {
                tracing::warn!(error = %e, "Gateway: mDNS announce failed, retrying in 10s");
                tokio::select! {
                    _ = shutdown.cancelled() => return,
                    _ = tokio::time::sleep(Duration::from_secs(10)) => continue,
                }
            }
        }
    };

    // ── Phase 2: Wait for tended stone ──────────────────────────
    let stone_endpoint = loop {
        if shutdown.is_cancelled() {
            deregister_mdns(&koi, &mdns_id).await;
            return;
        }

        if let Some(ep) = state.tended_endpoint().await {
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

    // ── Phase 3: Resolve host identity ──────────────────────────
    let (hostname, self_ip) = match koi.get_host_info().await {
        Ok(info) => {
            let ip = info.ip.unwrap_or_else(|| {
                let fallback = garden_common::infra::network::get_local_ip();
                tracing::warn!(
                    fallback = %fallback,
                    "Gateway: Koi returned no LAN IP, using local fallback"
                );
                fallback
            });
            tracing::info!(
                hostname = %info.hostname,
                ip = %ip,
                "Gateway: resolved host identity via Koi"
            );
            (info.hostname, ip)
        }
        Err(e) => {
            let ip = garden_common::infra::network::get_local_ip();
            tracing::warn!(
                error = %e,
                fallback_ip = %ip,
                "Gateway: Koi host info failed, using local IP detection"
            );
            (ip.clone(), ip)
        }
    };

    // ── Phase 4: Event-driven registration loop ─────────────────
    //
    // Subscribe to dashboard events. On each "registry.updated" event,
    // compute which offerings have healthy instances and diff against
    // what's currently registered. Register new, deregister removed.
    //
    // Also runs a periodic mDNS heartbeat and Moss lease renewal.

    let mut event_rx = state.dashboard_tx.subscribe();
    let mut mdns_interval = tokio::time::interval(Duration::from_secs(MDNS_HEARTBEAT_SECS));
    mdns_interval.tick().await; // consume immediate first tick

    let mut registered: HashSet<String> = HashSet::new();
    let mut current_endpoint = stone_endpoint;

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                tracing::info!("Gateway: deregistering all (shutdown)");
                deregister_mdns(&koi, &mdns_id).await;
                for offering in &registered {
                    deregister_moss(&moss_gw, &current_endpoint, offering).await;
                }
                return;
            }

            // React to instance registry changes
            event = event_rx.recv() => {
                match event {
                    Ok(ev) if ev.event_type == "registry.updated" => {
                        // Check if tended stone changed
                        if let Some(ep) = state.tended_endpoint().await {
                            if ep != current_endpoint {
                                tracing::info!(
                                    old = %current_endpoint,
                                    new = %ep,
                                    "Gateway: tended stone changed"
                                );
                                for offering in &registered {
                                    deregister_moss(&moss_gw, &current_endpoint, offering).await;
                                }
                                registered.clear();
                                current_endpoint = ep;
                            }
                        }

                        sync_registrations(
                            &state, &moss_gw, &current_endpoint,
                            &hostname, &self_ip, &mut registered,
                        ).await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "Gateway: event consumer lagged");
                        // Catch up by doing a full sync
                        sync_registrations(
                            &state, &moss_gw, &current_endpoint,
                            &hostname, &self_ip, &mut registered,
                        ).await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::info!("Gateway: event channel closed, stopping");
                        return;
                    }
                    _ => {} // ignore other event types
                }
            }

            // Periodic mDNS heartbeat + Moss lease renewal
            _ = mdns_interval.tick() => {
                if let Err(e) = koi.heartbeat(&mdns_id).await {
                    tracing::warn!(error = %e, "Gateway: mDNS heartbeat failed");
                }

                // Renew Moss leases for currently registered offerings
                // (also catches up if an event was missed)
                sync_registrations(
                    &state, &moss_gw, &current_endpoint,
                    &hostname, &self_ip, &mut registered,
                ).await;
            }
        }
    }
}

/// Compute which offerings should be registered based on healthy instances,
/// then register new ones and deregister stale ones.
async fn sync_registrations(
    state: &AppState,
    moss_gw: &MossGatewayClient,
    stone_endpoint: &str,
    hostname: &str,
    self_ip: &str,
    registered: &mut HashSet<String>,
) {
    // Compute the set of offering kinds that have healthy instances + proxy ports
    let desired: HashSet<String> = {
        let instances = state.instances.read().await;
        let mut set = HashSet::new();
        for inst in instances.values() {
            if inst.is_routable() && inst.kind.proxy_port().is_some() {
                set.insert(inst.kind.as_str().to_string());
            }
        }
        set
    };

    // Deregister offerings that no longer have healthy instances
    let to_remove: Vec<String> = registered
        .difference(&desired)
        .cloned()
        .collect();
    for offering in &to_remove {
        tracing::info!(offering = %offering, "Gateway: deregistering (no healthy instances)");
        deregister_moss(moss_gw, stone_endpoint, offering).await;
        registered.remove(offering);
    }

    // Register offerings that are new or need lease renewal
    for offering_name in &desired {
        let kind = match crate::domain::types::OfferingKind::from_str(offering_name) {
            Some(k) => k,
            None => continue,
        };

        let proxy_port = match kind.proxy_port() {
            Some(p) => p,
            None => continue,
        };

        let fqn = match garden_common::offerings::OfferingFqn::with_instance(
            offering_name,
            "orchestrator",
        ) {
            Ok(f) => f.to_string(),
            Err(_) => continue,
        };

        let params = GatewayParams {
            fqn,
            hostname: hostname.to_string(),
            ip: self_ip.to_string(),
            port: proxy_port,
            handler_for: vec![offering_name.to_string()],
            protocol: "http".to_string(),
            uri_template: Some(format!("http://{{host}}:{proxy_port}")),
            category: None,
            tags: vec![],
            source: "zen-garden.ai.orchestrator".to_string(),
        };

        match moss_gw
            .register(stone_endpoint, offering_name, &params)
            .await
        {
            Ok(resp) => {
                if registered.insert(offering_name.clone()) {
                    // First time registering this offering
                    tracing::info!(
                        offering = %offering_name,
                        lease_id = %resp.lease_id,
                        ttl = resp.ttl_seconds,
                        "Gateway: registered offering with Moss"
                    );
                }
                // else: silent lease renewal (logged at debug)
            }
            Err(e) => {
                tracing::warn!(
                    offering = %offering_name,
                    error = %e,
                    "Gateway: Moss registration failed for offering"
                );
            }
        }
    }
}

/// Best-effort mDNS deregistration.
async fn deregister_mdns(koi: &KoiMdnsClient, id: &str) {
    if let Err(e) = koi.unregister(id).await {
        tracing::warn!(error = %e, "Gateway: mDNS deregistration failed");
    } else {
        tracing::info!("Gateway: mDNS deregistered");
    }
}

/// Best-effort Moss gateway deregistration for one offering.
async fn deregister_moss(moss_gw: &MossGatewayClient, stone_endpoint: &str, offering: &str) {
    if let Err(e) = moss_gw.deregister(stone_endpoint, offering).await {
        tracing::warn!(offering = %offering, error = %e, "Gateway: Moss deregistration failed");
    } else {
        tracing::info!(offering = %offering, "Gateway: Moss gateway deregistered");
    }
}
