//! Layer 3: Endpoint resolution (pure computation)
//!
//! Answers "what endpoint should I talk to?" with provenance.
//! No connection lifecycle, no health probes on the happy path.

use anyhow::Result;
use garden_common::HardwareCapabilities;
use std::time::Duration;

// ============================================================================
// Resolution result
// ============================================================================

/// A resolved endpoint and how it was determined.
pub struct Resolved {
    pub endpoint: String,
    pub origin: Origin,
}

/// How the endpoint was determined -- provenance for diagnostics
/// and recovery decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// `--at` flag: user's explicit intent, never re-resolved.
    Flag,
    /// `ZG_STONE` env var: operator intent, never re-resolved.
    Env,
    /// `.tending` file: cached from previous session, flushable on TCP failure.
    Tending,
    /// UDP/mDNS discovery: just found on the network, flushable on TCP failure.
    Discovered,
}

impl Origin {
    /// Whether this origin can be invalidated on connection failure.
    /// Explicit overrides (Flag, Env) are user intent -- never overridden.
    pub fn is_soft(&self) -> bool {
        matches!(self, Self::Tending | Self::Discovered)
    }
}

// ============================================================================
// Cache trait (implemented by stone_cache::Discovery)
// ============================================================================

/// Trait for in-process stone endpoint cache.
pub trait CachedStoneOps: Send + Sync {
    fn get(&self, stone_name: &str) -> Option<CachedStoneInfo>;
    fn insert(&self, endpoint: String, capabilities: HardwareCapabilities);
}

#[derive(Clone)]
pub struct CachedStoneInfo {
    pub endpoint: String,
}

// ============================================================================
// Primary resolution entry point
// ============================================================================

/// Resolve endpoint from the priority cascade:
/// `--at` > env var > cached tending > auto-discover.
///
/// Returns the endpoint and its provenance. No health probes on the
/// happy path -- 99.9% of the time the tending file is correct.
///
/// `failed_endpoint` -- if the caller already tried an endpoint and got
/// a TCP-level connection failure, pass it here. When Priority 3 (tending)
/// matches the failed IP, the stale tending file is flushed and resolution
/// falls through to discovery.
pub async fn resolve(
    client: &reqwest::Client,
    at: Option<&str>,
    cache: Option<&dyn CachedStoneOps>,
    failed_endpoint: Option<&str>,
) -> Result<Resolved> {
    use crate::discovery;
    use crate::stone_bag::StoneBag;
    use crate::tending;
    use crate::ui::rendering::{self as ui, TerminalInfo};
    use crate::commands::management::tend;

    let term = TerminalInfo::detect();

    // Priority 1: --at flag (explicit override, deterministic)
    if let Some(explicit) = at {
        let endpoint = resolve_target(client, explicit, cache).await?;
        return Ok(Resolved {
            endpoint,
            origin: Origin::Flag,
        });
    }

    // Priority 2: ZG_STONE / GARDEN_STONE environment variable
    if let Ok(env_endpoint) = std::env::var(garden_common::constants::ENV_GARDEN_STONE) {
        tracing::info!(endpoint = %env_endpoint, "Using GARDEN_STONE environment variable");
        let endpoint = resolve_target(client, &env_endpoint, cache).await?;
        return Ok(Resolved {
            endpoint,
            origin: Origin::Env,
        });
    }

    // Priority 3: Cached tending state (no TTL -- persists until stone unreachable)
    //
    // Optimistic: return the cached endpoint without probing.
    //
    // If the caller already tried this endpoint and hit a CONNECTION failure,
    // it passes the dead IP via `failed_endpoint`. We flush stale tending and
    // fall through to discovery.
    if let Ok(tending) = tending::read_tending() {
        if let Some(failed) = failed_endpoint
            && tending.endpoint == failed
        {
            tracing::warn!(
                stone = %tending.stone_name,
                endpoint = %tending.endpoint,
                age_secs = tending.age_seconds(),
                "Tended endpoint matches failed connection -- flushing"
            );
            let _ = tending::clear_tending();

            println!(
                "{}{} \"{}\" at {} unreachable -- rediscovering...",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("warn", term.supports_color),
                tending.stone_name,
                tending.endpoint,
            );
            // fall through to discovery
        } else {
            tracing::info!(
                stone = %tending.stone_name,
                endpoint = %tending.endpoint,
                age_secs = tending.age_seconds(),
                "Using cached tending state"
            );
            return Ok(Resolved {
                endpoint: tending.endpoint,
                origin: Origin::Tending,
            });
        }
    }

    // Priority 4: Auto-discover via UDP broadcast + cache result
    tracing::debug!("No cached tending, attempting auto-discovery");
    println!(
        "{}{} Discovering stones...",
        " ".repeat(ui::constants::DEFAULT_INDENT),
        ui::status_indicator("info", term.supports_color)
    );

    let endpoint = discovery::discover_moss().await.map_err(|_| {
        anyhow::anyhow!(
            "No Zen Garden stones discovered.\n\n\
            Possible causes:\n\
              - No stones present on your network\n\
              - Firewall is blocking UDP broadcast (port {})\n\
              - Stone's garden-moss service is not running\n\n\
            To fix:\n\
              - Create a new stone: Run installer/NewStone-linux-x64.ps1\n\
              - Set tending: garden-rake tend <endpoint>\n\
              - Specify endpoint manually: garden-rake <command> --at http://<IP>:{}\n\
              - Or use a stone name: garden-rake <command> --at <stone-name>\n\
              - Check stone status: ssh stone@<ip> systemctl status garden-moss.service",
            garden_common::constants::DISCOVERY_UDP,
            garden_common::constants::MOSS_HTTP,
        )
    })?;

    tracing::info!(endpoint = %endpoint, "Auto-discovered stone");

    // Fetch stone name for tending (single capabilities call)
    let bag = StoneBag::new(client.clone(), endpoint.clone());
    if let Some(name) = bag.stone_name().await {
        let caps = bag.capabilities_owned().await;
        let _ = tending::write_tending(name.to_string(), endpoint.clone(), caps);

        println!(
            "{}{} Now tending to \"{}\"",
            " ".repeat(ui::constants::DEFAULT_INDENT),
            ui::status_indicator("success", term.supports_color),
            name
        );

        // Notify stone of tending for visual feedback (glow/pulse)
        let api = garden_common::client::StoneApi::new(client.clone(), endpoint.clone());
        let _ = tend::notify_tending(&api).await;
    }

    Ok(Resolved {
        endpoint,
        origin: Origin::Discovered,
    })
}

// ============================================================================
// Target resolution (bare name / URL / IP -> endpoint URL)
// ============================================================================

/// Resolve a user-supplied stone target into a moss HTTP endpoint.
///
/// Accepted forms:
/// - Full URL: `http://<host>:7185` / `https://...`
/// - Host-ish: `<host>:7185`, `<host>.local`, `<ip>:7185`
/// - Stone name: `stone-01` (resolved via `.local` probe, then Lantern fallback)
pub async fn resolve_target(
    client: &reqwest::Client,
    target: &str,
    cache: Option<&dyn CachedStoneOps>,
) -> Result<String> {
    let trimmed = target.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(anyhow::anyhow!("target value cannot be empty"));
    }

    // Already a URL with a scheme -- accept as-is.
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Ok(trimmed.to_string());
    }

    // host:port or hostname.local:port -- normalize to http://
    if trimmed.contains(':') {
        return Ok(format!("http://{}", trimmed));
    }

    // host/IP without port -- default to moss HTTP port.
    if trimmed.contains('.') {
        let http_endpoint = format!("http://{}:{}", trimmed, garden_common::constants::MOSS_HTTP);

        if is_enrolled() {
            let https_endpoint = format!(
                "https://{}:{}",
                trimmed,
                garden_common::constants::MOSS_HTTPS
            );
            if probe_moss_health(client, &https_endpoint).await {
                return Ok(https_endpoint);
            }
        }

        return Ok(http_endpoint);
    }

    // Bare stone name -- resolve via mDNS, discovery, Lantern
    resolve_stone_name(client, trimmed, cache).await
}

// ============================================================================
// Internals
// ============================================================================

fn is_enrolled() -> bool {
    let hostname = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_default();
    crate::enrollment::is_enrolled(&hostname)
}

async fn resolve_stone_name(
    client: &reqwest::Client,
    stone_name: &str,
    cache: Option<&dyn CachedStoneOps>,
) -> Result<String> {
    use garden_common::LanternTopology;

    let requested = stone_name.trim_end_matches(".local");
    let requested_lower = requested.to_lowercase();

    // 1) Check cache (case-insensitive)
    if let Some(cache) = cache {
        if let Some(cached) = cache.get(requested).or_else(|| cache.get(&requested_lower)) {
            if probe_moss_health(client, &cached.endpoint).await {
                return Ok(cached.endpoint);
            }
            tracing::debug!(stone = %requested, "Cached endpoint unreachable, trying other methods");
        }
    }

    // 2) mDNS-style hostname: stone-01.local:7185
    let mdns_host = format!("{}.local", requested_lower);

    if is_enrolled() {
        let https_endpoint = format!(
            "https://{}:{}",
            mdns_host,
            garden_common::constants::MOSS_HTTPS
        );
        if probe_moss_health(client, &https_endpoint).await {
            return Ok(https_endpoint);
        }
    }

    let mdns_endpoint = format!(
        "http://{}:{}",
        mdns_host,
        garden_common::constants::MOSS_HTTP
    );
    if probe_moss_health(client, &mdns_endpoint).await {
        return Ok(mdns_endpoint);
    }

    // 3) UDP Discovery -- find stone by name or id
    let mut discovered_responses = Vec::new();
    let _stone_count = crate::discovery::discover_all_moss_stream_async(
        Duration::from_secs(3),
        |response, _instant| {
            discovered_responses.push(response);
        },
    )
    .await;

    for response in discovered_responses {
        let endpoint = response.address.http_base();
        let endpoint = endpoint.trim_end_matches('/').to_string();
        let api = garden_common::client::StoneApi::new(client.clone(), endpoint.clone());
        if let Ok(caps) = api.stone().capabilities_core().await {
            if let Some(cache) = cache {
                cache.insert(endpoint.clone(), caps.clone());
            }
            if caps.stone_name.eq_ignore_ascii_case(requested) {
                return Ok(endpoint);
            }
            if let Some(ref stone_id) = caps.stone_id
                && stone_id.eq_ignore_ascii_case(requested)
            {
                return Ok(endpoint);
            }
        }
    }

    // 4) Lantern fallback
    crate::discovery::discover_lantern_background();
    if let Some(lantern) = crate::discovery::get_cached_lantern() {
        let url = format!("{}/api/v1/stones", lantern.trim_end_matches('/'));
        match client
            .get(&url)
            .timeout(Duration::from_secs(3))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(topology) = resp.json::<LanternTopology>().await {
                    if let Some(stone) = topology.stones.iter().find(|s| {
                        s.name.eq_ignore_ascii_case(requested)
                            || s.stone_id
                                .as_ref()
                                .map(|id| id.eq_ignore_ascii_case(requested))
                                .unwrap_or(false)
                    }) {
                        return Ok(stone.endpoint.trim_end_matches('/').to_string());
                    }
                }
            }
            Ok(resp) => {
                tracing::warn!(status = ?resp.status(), "Lantern returned non-success");
            }
            Err(e) => {
                tracing::warn!(error = ?e, "Failed to query Lantern for stone name resolution");
            }
        }
    }

    Err(anyhow::anyhow!(
        "Could not resolve '{}' to a moss endpoint.\n\n\
        Try one of:\n\
          - garden-rake tend auto (auto-discover)\n\
          - garden-rake observe (to see discovered endpoints)\n\
          - garden-rake tend http://<ip>:7185",
        stone_name
    ))
}

async fn probe_moss_health(client: &reqwest::Client, endpoint: &str) -> bool {
    let url = format!("{}/health", endpoint.trim_end_matches('/'));
    match client
        .get(&url)
        .timeout(Duration::from_millis(800))
        .send()
        .await
    {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}
