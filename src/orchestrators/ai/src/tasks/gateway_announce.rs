//! Gateway announcement task — single coordinated gateway for all offerings.
//!
//! Two-registration model:
//! 1. Register mDNS name with Koi (hostname becomes resolvable)
//! 2. Register one gateway entry per active offering type with Moss
//! 3. Heartbeat both every 30s
//! 4. Graceful deregister on shutdown

use crate::AppState;
use orchestrator_common::gateway::{GatewayParams, KoiMdnsClient, MossGatewayClient};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Default heartbeat interval (seconds).
const HEARTBEAT_INTERVAL_SECS: u64 = 30;

/// Default mDNS lease (seconds).
const MDNS_LEASE_SECS: u32 = 60;

/// mDNS service name for the AI orchestrator.
const MDNS_NAME: &str = "ZenGarden orchestrator: AI";

/// Run the gateway announcement lifecycle.
///
/// Boot -> register mDNS -> wait for stone -> register per-offering gateways -> heartbeat loop.
/// On shutdown (token cancelled) -> deregister all.
pub async fn run(state: AppState, shutdown: CancellationToken) {
    let koi = KoiMdnsClient::new(&state.koi_endpoint);
    let moss_gw = MossGatewayClient::new();

    // ── Phase 1: mDNS announce via Koi ───────────────────────────
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

    // ── Phase 2: Wait for tended stone ───────────────────────────
    let stone_endpoint = loop {
        if shutdown.is_cancelled() {
            deregister_mdns(&koi, &mdns_id).await;
            return;
        }

        if let Some(ep) = state.tended_endpoint().await {
            tracing::debug!(endpoint = %ep, "Gateway: stone available for gateway registration");
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

    // ── Phase 3: Resolve host identity via Koi ───────────────────
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

    // ── Phase 4: Register per-offering gateways with Moss ────────
    let mut registered_offerings = Vec::new();
    register_offering_gateways(
        &state,
        &moss_gw,
        &stone_endpoint,
        &hostname,
        &self_ip,
        &mut registered_offerings,
    )
    .await;

    // ── Phase 5: Heartbeat loop ──────────────────────────────────
    let mut interval = tokio::time::interval(Duration::from_secs(HEARTBEAT_INTERVAL_SECS));
    interval.tick().await; // consume immediate first tick

    let mut last_registered_endpoint = stone_endpoint;

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                tracing::info!("Gateway: deregistering (shutdown)");
                deregister_mdns(&koi, &mdns_id).await;
                for offering in &registered_offerings {
                    deregister_moss(&moss_gw, &last_registered_endpoint, offering).await;
                }
                return;
            }
            _ = interval.tick() => {
                // mDNS heartbeat
                if let Err(e) = koi.heartbeat(&mdns_id).await {
                    tracing::warn!(error = %e, "Gateway: mDNS heartbeat failed");
                }

                let current_endpoint = match state.tended_endpoint().await {
                    Some(ep) => ep,
                    None => {
                        tracing::debug!("Gateway: no tended endpoint, skipping heartbeat");
                        continue;
                    }
                };

                // Stone switch: deregister from old stone before registering with new.
                if current_endpoint != last_registered_endpoint && !registered_offerings.is_empty() {
                    tracing::info!(
                        old = %last_registered_endpoint,
                        new = %current_endpoint,
                        "Gateway: tended stone changed, deregistering from old stone"
                    );
                    for offering in &registered_offerings {
                        deregister_moss(&moss_gw, &last_registered_endpoint, offering).await;
                    }
                    registered_offerings.clear();
                }

                // Re-register / heartbeat all offering gateways
                register_offering_gateways(
                    &state,
                    &moss_gw,
                    &current_endpoint,
                    &hostname,
                    &self_ip,
                    &mut registered_offerings,
                )
                .await;

                last_registered_endpoint = current_endpoint;
            }
        }
    }
}

/// Register a gateway entry for each active offering type.
async fn register_offering_gateways(
    state: &AppState,
    moss_gw: &MossGatewayClient,
    stone_endpoint: &str,
    hostname: &str,
    self_ip: &str,
    registered: &mut Vec<String>,
) {
    let kinds: Vec<_> = state.registry.kinds().collect();

    for kind in kinds {
        let offering_name = kind.as_str();

        // Only register offerings that have a proxy port
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

        match moss_gw.register(stone_endpoint, offering_name, &params).await {
            Ok(resp) => {
                tracing::info!(
                    offering = %offering_name,
                    lease_id = %resp.lease_id,
                    ttl = resp.ttl_seconds,
                    "Gateway: registered offering with Moss"
                );
                if !registered.contains(&offering_name.to_string()) {
                    registered.push(offering_name.to_string());
                }
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
