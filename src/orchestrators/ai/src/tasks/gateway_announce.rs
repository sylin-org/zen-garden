//! Gateway announcement task — adapted from ORCH-0004.
//!
//! The AI orchestrator has two registration responsibilities:
//!
//! 1. **Self-registration** — register the orchestrator as its own service
//!    via mDNS (discoverable on the network).
//!
//! 2. **Per-offering gateway registration** — for each AI offering it manages,
//!    register as the gateway/proxy for that offering's FQN. This mirrors
//!    exactly what the Ollama orchestrator does for `ollama:orchestrator`:
//!    each offering gets its own `PUT /api/v1/garden/gateway/{offering}`
//!    entry so that Koan apps looking up "ollama" or "comfyui" find the
//!    AI orchestrator's proxy endpoint.

use std::time::Duration;

use orchestrator_common::gateway::{GatewayParams, KoiMdnsClient, MossGatewayClient};
use tokio_util::sync::CancellationToken;

use crate::app_state::AppState;

const HEARTBEAT_INTERVAL_SECS: u64 = 30;
const MDNS_LEASE_SECS: u32 = 60;
const MDNS_NAME: &str = "ZenGarden orchestrator: AI";

/// All offering types this orchestrator handles.
/// Each gets its own gateway registration with Moss.
const MANAGED_OFFERINGS: &[&str] = &[
    "ollama",
    "ollama-cpu",
    "comfyui",
    "whispercpp",
    "speaches",
    "speaches-cpu",
    "openedai-speech",
    "openedai-speech-min",
    "infinity",
    "infinity-cpu",
    "libretranslate",
];

/// Run the gateway announcement lifecycle.
pub async fn run(state: AppState, shutdown: CancellationToken) {
    let koi = KoiMdnsClient::new(&state.koi_endpoint);
    let moss_gw = MossGatewayClient::new();

    // ── Phase 1: mDNS self-registration via Koi ─────────────────
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
    //
    // Each offering gets its own gateway entry — exactly as the original
    // Ollama orchestrator registers "ollama:orchestrator". This way Koan
    // apps that look up "ollama" or "comfyui" find this orchestrator's
    // proxy endpoint.
    let offering_params: Vec<(&str, GatewayParams)> = MANAGED_OFFERINGS
        .iter()
        .map(|&offering| {
            let params = GatewayParams {
                fqn: garden_common::offerings::OfferingFqn::with_instance(
                    offering,
                    "orchestrator",
                )
                .expect("valid FQN")
                .to_string(),
                hostname: hostname.clone(),
                ip: self_ip.clone(),
                port: state.proxy_port,
                handler_for: vec![offering.to_string()],
                protocol: "http".to_string(),
                uri_template: Some("http://{host}:{port}".to_string()),
                category: Some("ai".to_string()),
                tags: vec!["ai".to_string(), "orchestrator".to_string()],
                source: "zen-garden.ai.orchestrator".to_string(),
            };
            (offering, params)
        })
        .collect();

    let mut registered_offerings: Vec<&str> = Vec::new();
    for (offering, params) in &offering_params {
        match moss_gw
            .register(&stone_endpoint, offering, params)
            .await
        {
            Ok(resp) => {
                tracing::info!(
                    offering = %offering,
                    lease_id = %resp.lease_id,
                    "Gateway: registered {offering}:orchestrator with Moss"
                );
                registered_offerings.push(offering);
            }
            Err(e) => {
                tracing::warn!(
                    offering = %offering,
                    error = %e,
                    "Gateway: Moss registration failed for {offering} (will retry)"
                );
            }
        }
    }

    tracing::info!(
        count = registered_offerings.len(),
        total = MANAGED_OFFERINGS.len(),
        "Gateway: initial per-offering registration complete"
    );

    // ── Phase 5: Heartbeat loop ──────────────────────────────────
    let mut interval = tokio::time::interval(Duration::from_secs(HEARTBEAT_INTERVAL_SECS));
    interval.tick().await;

    let mut last_registered_endpoint = stone_endpoint.clone();

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                tracing::info!("Gateway: deregistering all (shutdown)");
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

                // Resolve current stone for failover detection.
                let current_endpoint = match state.tended_endpoint().await {
                    Some(ep) => ep,
                    None => {
                        tracing::debug!("Gateway: no tended endpoint, skipping heartbeat");
                        continue;
                    }
                };

                // Stone switch: deregister all from old stone before re-registering.
                if current_endpoint != last_registered_endpoint {
                    tracing::info!(
                        old = %last_registered_endpoint,
                        new = %current_endpoint,
                        "Gateway: tended stone changed, deregistering from old"
                    );
                    for offering in &registered_offerings {
                        deregister_moss(&moss_gw, &last_registered_endpoint, offering).await;
                    }
                    registered_offerings.clear();
                }

                // Re-register/heartbeat all offerings with current stone.
                for (offering, params) in &offering_params {
                    match moss_gw.register(&current_endpoint, offering, params).await {
                        Ok(_) => {
                            if !registered_offerings.contains(offering) {
                                tracing::info!(
                                    offering = %offering,
                                    stone = %current_endpoint,
                                    "Gateway: {offering}:orchestrator recovered"
                                );
                                registered_offerings.push(offering);
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                offering = %offering,
                                error = %e,
                                "Gateway: heartbeat failed for {offering}"
                            );
                        }
                    }
                }

                // Only advance the tracked endpoint when at least one offering
                // is successfully registered. If all registrations fail (stone
                // momentarily unavailable), keep the old endpoint so the next
                // heartbeat retries the stone-switch deregistration path.
                if !registered_offerings.is_empty() {
                    last_registered_endpoint = current_endpoint;
                }
            }
        }
    }
}

async fn deregister_mdns(koi: &KoiMdnsClient, id: &str) {
    if let Err(e) = koi.unregister(id).await {
        tracing::warn!(error = %e, "Gateway: mDNS deregistration failed");
    } else {
        tracing::info!("Gateway: mDNS deregistered");
    }
}

async fn deregister_moss(moss_gw: &MossGatewayClient, stone_endpoint: &str, offering: &str) {
    if let Err(e) = moss_gw.deregister(stone_endpoint, offering).await {
        tracing::warn!(
            offering = %offering,
            error = %e,
            "Gateway: Moss deregistration failed"
        );
    }
}
