//! Gateway announcement task — ORCH-0004.
//!
//! Parameterized two-registration model:
//! 1. Register mDNS name with Koi (hostname becomes resolvable)
//! 2. Register gateway with Moss (entry appears in topology chirps)
//! 3. Heartbeat both every 30s
//! 4. Graceful deregister on shutdown
//!
//! Each orchestrator provides its own `GatewayAnnounceConfig` with offering
//! name, mDNS name, port, etc. and a closure that resolves the current
//! tended stone endpoint from shared state.

use crate::gateway::{GatewayParams, KoiMdnsClient, MossGatewayClient};
use std::future::Future;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Default heartbeat interval (seconds).
const HEARTBEAT_INTERVAL_SECS: u64 = 30;

/// Default mDNS lease (seconds).
const MDNS_LEASE_SECS: u32 = 60;

/// Configuration for the gateway announcement task.
pub struct GatewayAnnounceConfig {
    /// mDNS service name (e.g. "ZenGarden orchestrator: Ollama").
    pub mdns_name: String,
    /// Offering name for gateway registration (e.g. "ollama", "mongodb").
    pub offering: String,
    /// Fully qualified name for gateway entry (e.g. "ollama:orchestrator").
    pub fqn: String,
    /// Port for mDNS and gateway registration (e.g. 21434 for proxy, 27017 for direct).
    pub port: u16,
    /// Koi endpoint URL.
    pub koi_endpoint: String,
    /// Source identifier for the gateway entry (usually the offering_name).
    pub source: String,
    /// Connection protocol (default: "http"). Use "mongodb" for wire-protocol services.
    pub protocol: Option<String>,
    /// URI template override (default: "{protocol}://{host}:{port}").
    /// Set to `None` to use the default template for the protocol.
    pub uri_template: Option<String>,
}

/// Run the gateway announcement lifecycle.
///
/// Boot → register mDNS → wait for stone → register gateway → heartbeat loop.
/// On shutdown (token cancelled) → deregister both.
///
/// `get_tended_endpoint` is called repeatedly to resolve the current tended
/// stone endpoint. It should return `Some(url)` when a stone is bound.
pub async fn run<F, Fut>(
    config: GatewayAnnounceConfig,
    get_tended_endpoint: F,
    shutdown: CancellationToken,
) where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Option<String>> + Send,
{
    let koi = KoiMdnsClient::new(&config.koi_endpoint);
    let moss_gw = MossGatewayClient::new();

    // ── Phase 1: mDNS announce via Koi ───────────────────────────
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
            .announce(&config.mdns_name, config.port, MDNS_LEASE_SECS, txt)
            .await
        {
            Ok(id) => {
                tracing::info!(
                    mdns_id = %id,
                    name = %config.mdns_name,
                    port = config.port,
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

        if let Some(ep) = get_tended_endpoint().await {
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

    // ── Phase 3: Resolve host identity via Koi ─────────────────────
    // Koi runs on the host and knows the real LAN IP — critical when
    // this code runs inside a Docker container where get_local_ip()
    // returns the container's virtual bridge IP (or fails entirely).
    let (hostname, self_ip) = match koi.get_host_info().await {
        Ok(info) => {
            let ip = info.ip.unwrap_or_else(|| {
                let fallback = garden_common::infra::network::get_local_ip();
                tracing::warn!(fallback = %fallback, "Gateway: Koi returned no LAN IP, using local fallback");
                fallback
            });
            tracing::info!(hostname = %info.hostname, ip = %ip, "Gateway: resolved host identity via Koi");
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

    let protocol = config
        .protocol
        .clone()
        .unwrap_or_else(|| "http".to_string());
    let uri_template = config.uri_template.clone().unwrap_or_else(|| {
        format!("{protocol}://{{host}}:{{port}}")
    });

    let gw_params = GatewayParams {
        fqn: config.fqn.clone(),
        hostname: hostname.clone(),
        ip: self_ip.clone(),
        port: config.port,
        handler_for: vec![config.offering.clone()],
        protocol,
        uri_template: Some(uri_template),
        category: None,
        tags: vec![],
        source: config.source.clone(),
    };

    // ── Phase 4: Register gateway with Moss ──────────────────────
    let mut moss_registered = false;
    match moss_gw
        .register(&stone_endpoint, &config.offering, &gw_params)
        .await
    {
        Ok(resp) => {
            tracing::info!(
                lease_id = %resp.lease_id,
                ttl = resp.ttl_seconds,
                stone = %stone_endpoint,
                "Gateway: registered with Moss"
            );
            moss_registered = true;
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                stone = %stone_endpoint,
                "Gateway: Moss registration failed (will retry in heartbeat)"
            );
        }
    }

    // ── Phase 5: Heartbeat loop ──────────────────────────────────
    let mut interval = tokio::time::interval(Duration::from_secs(HEARTBEAT_INTERVAL_SECS));
    interval.tick().await; // consume immediate first tick

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                tracing::info!("Gateway: deregistering (shutdown)");
                deregister_mdns(&koi, &mdns_id).await;
                if moss_registered {
                    deregister_moss(&moss_gw, &stone_endpoint, &config.offering).await;
                }
                return;
            }
            _ = interval.tick() => {
                // mDNS heartbeat
                if let Err(e) = koi.heartbeat(&mdns_id).await {
                    tracing::warn!(error = %e, "Gateway: mDNS heartbeat failed");
                }

                // Moss heartbeat (re-PUT is idempotent)
                // Re-resolve stone endpoint in case it changed (stone failover)
                let current_endpoint = get_tended_endpoint()
                    .await
                    .unwrap_or_else(|| stone_endpoint.clone());

                match moss_gw.register(&current_endpoint, &config.offering, &gw_params).await {
                    Ok(_) => {
                        if !moss_registered {
                            tracing::info!(
                                stone = %current_endpoint,
                                "Gateway: Moss registration recovered"
                            );
                        }
                        moss_registered = true;
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Gateway: Moss heartbeat failed");
                    }
                }
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

/// Best-effort Moss gateway deregistration.
async fn deregister_moss(moss_gw: &MossGatewayClient, stone_endpoint: &str, offering: &str) {
    if let Err(e) = moss_gw.deregister(stone_endpoint, offering).await {
        tracing::warn!(error = %e, "Gateway: Moss deregistration failed");
    } else {
        tracing::info!("Gateway: Moss gateway deregistered");
    }
}
