//! Resilient Tools API stream runner with automatic stone failover.
//!
//! Wraps `tools_stream::subscribe_tools_stream()` with:
//! - Exponential backoff on transient failures
//! - Endpoint blacklisting after N consecutive failures
//! - Automatic failover to alternative stones (from registry, then Koi mDNS)
//! - Local-first resolution (try the host's own Moss before remote stones)
//!
//! Orchestrators provide a [`StreamContext`] with their specific configuration
//! and callbacks, then call [`run_resilient_stream`] which handles the rest.

use crate::discovery;
use crate::persistence::TendedStone;
use crate::tools_stream::{self, ToolStreamEvent};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// Async callback that returns a `Vec<String>`.
type AsyncEndpointsFn = Box<dyn Fn() -> Pin<Box<dyn Future<Output = Vec<String>> + Send>> + Send + Sync>;

/// Async callback that receives a `TendedStone`.
type AsyncTendedFn = Box<dyn Fn(TendedStone) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// Async callback that receives an endpoint string.
type AsyncEndpointFn = Box<dyn Fn(String) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// Configuration for the resilient stream runner.
pub struct StreamConfig {
    /// Max consecutive failures before blacklisting + failover (default: 3).
    pub max_failures: u32,
    /// How long a failed endpoint stays blacklisted (default: 120s).
    pub blacklist_secs: u64,
    /// Initial backoff on failure (default: 1s).
    pub initial_backoff_secs: u64,
    /// Maximum backoff cap (default: 60s).
    pub max_backoff_secs: u64,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            max_failures: 3,
            blacklist_secs: 120,
            initial_backoff_secs: 1,
            max_backoff_secs: 60,
        }
    }
}

/// Context provided by the orchestrator to the resilient stream runner.
///
/// Uses closures instead of a trait to avoid async-trait dependencies and keep
/// the interface simple. Each closure captures the orchestrator's `AppState`.
pub struct StreamContext<F, E>
where
    F: Fn(&str) -> bool + Clone + Send + Sync + 'static,
    E: Fn(ToolStreamEvent) + Clone + Send + Sync + 'static,
{
    /// Koi mDNS endpoint for fallback re-discovery.
    pub koi_endpoint: String,

    /// Optional: local stone endpoint to try first (e.g. `http://localhost:7185`).
    /// If the orchestrator runs on a stone, this hits local Moss with no network hop.
    pub local_endpoint: Option<String>,

    /// Whether an explicit stone override is set (`--stone` / `GARDEN_STONE`).
    /// When true, failover is disabled — we only retry the explicit endpoint.
    pub explicit_stone: Option<String>,

    /// Filter predicate for tool FQIDs (e.g. `|fqid| fqid.starts_with("offering:mongodb")`).
    pub fqid_filter: F,

    /// Callback invoked for each filtered tool stream event.
    pub on_event: E,

    /// Returns candidate Moss endpoints from the orchestrator's registry/catalog.
    /// Called during failover to find alternative stones without hitting Koi.
    pub candidate_endpoints: AsyncEndpointsFn,

    /// Called when a new stone is selected (initial resolution or failover).
    /// The orchestrator should update its tending state.
    pub on_stone_selected: AsyncTendedFn,

    /// Called after switching to a new stone (initial resolution and failover).
    /// The orchestrator should re-bootstrap from the new stone's topology.
    pub on_stone_switched: AsyncEndpointFn,

    /// Stream configuration.
    pub config: StreamConfig,
}

/// Run the tools stream with automatic reconnection and stone failover.
///
/// Resolution priority for the initial stone:
/// 1. Explicit stone override (`--stone` flag) — no failover
/// 2. Local stone endpoint (localhost Moss) — fast, no network
/// 3. Candidate endpoints from orchestrator registry
/// 4. Koi mDNS re-discovery
///
/// On stream failure:
/// - Exponential backoff (1s → 2s → 4s → ... → 60s)
/// - After `max_failures` consecutive errors: blacklist endpoint, pick next stone
/// - Blacklisted endpoints expire after `blacklist_secs`
pub async fn run_resilient_stream<F, E>(
    ctx: StreamContext<F, E>,
    shutdown: CancellationToken,
) where
    F: Fn(&str) -> bool + Clone + Send + Sync + 'static,
    E: Fn(ToolStreamEvent) + Clone + Send + Sync + 'static,
{
    // ── Phase 1: Resolve initial stone ──────────────────────────

    // Explicit stone — no failover, just use it directly
    if let Some(ref explicit) = ctx.explicit_stone {
        tracing::info!(endpoint = %explicit, "using explicit stone (failover disabled)");
        let tended = TendedStone {
            stone_name: "explicit".to_string(),
            stone_id: None,
            endpoint: explicit.clone(),
            last_seen: chrono::Utc::now(),
        };
        (ctx.on_stone_selected)(tended).await;
        (ctx.on_stone_switched)(explicit.clone()).await;
        stream_loop_no_failover(&ctx, explicit.clone(), &shutdown).await;
        return;
    }

    let initial = loop {
        if shutdown.is_cancelled() {
            return;
        }
        match resolve_initial_stone(&ctx, &shutdown).await {
            Some(ep) => break ep,
            None => {
                if shutdown.is_cancelled() {
                    return;
                }
                // resolve_initial_stone only returns None on shutdown
            }
        }
    };

    // Bootstrap from the initial stone's topology
    (ctx.on_stone_switched)(initial.clone()).await;

    // ── Phase 2: Stream loop with failover ──────────────────────

    let mut current_endpoint = initial;
    let mut blacklist: HashMap<String, Instant> = HashMap::new();
    let mut backoff_secs = ctx.config.initial_backoff_secs;
    let mut consecutive_failures = 0u32;

    loop {
        if shutdown.is_cancelled() {
            return;
        }

        let result = tools_stream::subscribe_tools_stream(
            &current_endpoint,
            ctx.fqid_filter.clone(),
            ctx.on_event.clone(),
        )
        .await;

        match result {
            Ok(()) => {
                tracing::info!("tools stream ended normally, reconnecting...");
                backoff_secs = ctx.config.initial_backoff_secs;
                consecutive_failures = 0;
            }
            Err(e) => {
                consecutive_failures += 1;
                tracing::warn!(
                    error = %e,
                    backoff = backoff_secs,
                    failures = consecutive_failures,
                    endpoint = %current_endpoint,
                    "tools stream error"
                );

                // After max_failures: blacklist and try another stone
                if consecutive_failures >= ctx.config.max_failures {
                    let blacklist_duration = Duration::from_secs(ctx.config.blacklist_secs);
                    blacklist.insert(
                        current_endpoint.clone(),
                        Instant::now() + blacklist_duration,
                    );
                    tracing::warn!(
                        endpoint = %current_endpoint,
                        blacklist_secs = ctx.config.blacklist_secs,
                        "stone blacklisted after {} consecutive failures",
                        consecutive_failures,
                    );

                    // Expire old blacklist entries
                    let now = Instant::now();
                    blacklist.retain(|_, expiry| *expiry > now);

                    if let Some(new_ep) = pick_next_stone(&ctx, &blacklist).await {
                        tracing::info!(
                            old = %current_endpoint,
                            new = %new_ep,
                            "failover: switched stone"
                        );
                        current_endpoint = new_ep;
                        consecutive_failures = 0;
                        backoff_secs = ctx.config.initial_backoff_secs;

                        // Let the orchestrator re-bootstrap from new topology
                        (ctx.on_stone_switched)(current_endpoint.clone()).await;
                        continue;
                    }

                    tracing::warn!("no alternative stones available, continuing with current");
                }

                backoff_secs = (backoff_secs * 2).min(ctx.config.max_backoff_secs);
            }
        }

        tokio::select! {
            _ = shutdown.cancelled() => return,
            _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => continue,
        }
    }
}

/// Stream loop for explicit stone mode — no failover, just exponential backoff.
async fn stream_loop_no_failover<F, E>(
    ctx: &StreamContext<F, E>,
    endpoint: String,
    shutdown: &CancellationToken,
) where
    F: Fn(&str) -> bool + Clone + Send + Sync + 'static,
    E: Fn(ToolStreamEvent) + Clone + Send + Sync + 'static,
{
    let mut backoff_secs = ctx.config.initial_backoff_secs;

    loop {
        if shutdown.is_cancelled() {
            return;
        }

        let result = tools_stream::subscribe_tools_stream(
            &endpoint,
            ctx.fqid_filter.clone(),
            ctx.on_event.clone(),
        )
        .await;

        match result {
            Ok(()) => {
                tracing::info!("tools stream ended normally, reconnecting...");
                backoff_secs = ctx.config.initial_backoff_secs;
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    backoff = backoff_secs,
                    endpoint = %endpoint,
                    "tools stream error (explicit stone, no failover)"
                );
                backoff_secs = (backoff_secs * 2).min(ctx.config.max_backoff_secs);
            }
        }

        tokio::select! {
            _ = shutdown.cancelled() => return,
            _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => continue,
        }
    }
}

/// Resolve the initial stone endpoint.
///
/// Priority cascade:
/// 1. Local stone (localhost Moss) — fast, no network
/// 2. Candidates from orchestrator's registry
/// 3. Koi mDNS discovery
///
/// Retries every 10s until a healthy stone is found or shutdown is requested.
async fn resolve_initial_stone<F, E>(
    ctx: &StreamContext<F, E>,
    shutdown: &CancellationToken,
) -> Option<String>
where
    F: Fn(&str) -> bool + Clone + Send + Sync + 'static,
    E: Fn(ToolStreamEvent) + Clone + Send + Sync + 'static,
{
    loop {
        if shutdown.is_cancelled() {
            return None;
        }

        // 1. Try local stone
        if let Some(ref local_ep) = ctx.local_endpoint {
            if discovery::check_stone_health(local_ep).await {
                tracing::info!(endpoint = %local_ep, "using local stone");
                notify_stone_selected(ctx, local_ep, "local", None).await;
                return Some(local_ep.clone());
            }
            tracing::debug!(endpoint = %local_ep, "local stone not reachable");
        }

        // 2. Try candidate endpoints from registry
        let candidates = (ctx.candidate_endpoints)().await;
        for ep in &candidates {
            if discovery::check_stone_health(ep).await {
                tracing::info!(endpoint = %ep, "using stone from registry");
                notify_stone_selected(ctx, ep, "registry", None).await;
                return Some(ep.clone());
            }
        }

        // 3. Try Koi mDNS discovery
        match discovery::discover_stones(&ctx.koi_endpoint).await {
            Ok(stones) if !stones.is_empty() => {
                for stone in &stones {
                    let ep = stone.endpoint();
                    if discovery::check_stone_health(&ep).await {
                        tracing::info!(
                            stone = %stone.stone_name,
                            endpoint = %ep,
                            "discovered stone via Koi mDNS"
                        );
                        let tended = TendedStone {
                            stone_name: stone.stone_name.clone(),
                            stone_id: stone.stone_id.clone(),
                            endpoint: ep.clone(),
                            last_seen: chrono::Utc::now(),
                        };
                        (ctx.on_stone_selected)(tended).await;
                        return Some(ep);
                    }
                }
                tracing::warn!(
                    count = stones.len(),
                    "discovered stones via mDNS but none are healthy"
                );
            }
            Ok(_) => {
                tracing::info!("no stones found via Koi mDNS, retrying in 10s");
            }
            Err(e) => {
                tracing::warn!(error = %e, "Koi mDNS discovery failed, retrying in 10s");
            }
        }

        tokio::select! {
            _ = shutdown.cancelled() => return None,
            _ = tokio::time::sleep(Duration::from_secs(10)) => continue,
        }
    }
}

/// Pick the next stone during failover, skipping blacklisted endpoints.
///
/// Priority:
/// 1. Candidate endpoints from registry (minus blacklisted)
/// 2. Koi mDNS re-discovery (minus blacklisted)
async fn pick_next_stone<F, E>(
    ctx: &StreamContext<F, E>,
    blacklist: &HashMap<String, Instant>,
) -> Option<String>
where
    F: Fn(&str) -> bool + Clone + Send + Sync + 'static,
    E: Fn(ToolStreamEvent) + Clone + Send + Sync + 'static,
{
    let now = Instant::now();

    // 1. Try candidates from registry
    let candidates = (ctx.candidate_endpoints)().await;
    for ep in &candidates {
        if blacklist.get(ep.as_str()).is_some_and(|expiry| *expiry > now) {
            continue;
        }
        if discovery::check_stone_health(ep).await {
            notify_stone_selected(ctx, ep, "registry-failover", None).await;
            return Some(ep.clone());
        }
    }

    // 2. Fallback: Koi mDNS re-discovery
    match discovery::discover_stones(&ctx.koi_endpoint).await {
        Ok(stones) => {
            for stone in &stones {
                let ep = stone.endpoint();
                if blacklist.get(ep.as_str()).is_some_and(|expiry| *expiry > now) {
                    continue;
                }
                if discovery::check_stone_health(&ep).await {
                    let tended = TendedStone {
                        stone_name: stone.stone_name.clone(),
                        stone_id: stone.stone_id.clone(),
                        endpoint: ep.clone(),
                        last_seen: chrono::Utc::now(),
                    };
                    (ctx.on_stone_selected)(tended).await;
                    return Some(ep);
                }
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "Koi re-discovery failed during failover");
        }
    }

    None
}

/// Notify the orchestrator that a stone was selected (for local/registry picks
/// where we don't have full DiscoveredStone metadata).
async fn notify_stone_selected<F, E>(
    ctx: &StreamContext<F, E>,
    endpoint: &str,
    source: &str,
    stone_id: Option<&str>,
) where
    F: Fn(&str) -> bool + Clone + Send + Sync + 'static,
    E: Fn(ToolStreamEvent) + Clone + Send + Sync + 'static,
{
    let tended = TendedStone {
        stone_name: source.to_string(),
        stone_id: stone_id.map(|s| s.to_string()),
        endpoint: endpoint.to_string(),
        last_seen: chrono::Utc::now(),
    };
    (ctx.on_stone_selected)(tended).await;
}
