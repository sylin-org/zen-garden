//! Stone discovery task for the MongoDB orchestrator.
//!
//! Two-phase discovery:
//! 1. Find a stone to tend to (via Koi mDNS or explicit `--stone`)
//! 2. Subscribe to the Tools API stream for MongoDB offerings
//!
//! When an explicit `--stone` is set, phase 1 is skipped.

use crate::app_state::AppState;
use crate::domain::types::{InstanceHealth, MongoInstance};
use orchestrator_common::discovery;
use orchestrator_common::persistence::TendedStone;
use orchestrator_common::tools_stream::{self, ToolStreamEvent};
use orchestrator_common::topology;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// Run the discovery task.
///
/// 1. Discover or use explicit stone → bind as tended
/// 2. Bootstrap from topology (one-shot scan for existing MongoDB instances)
/// 3. Subscribe to tools stream for real-time changes
/// 4. On stream error, reconnect with backoff
pub async fn run(state: AppState, shutdown: CancellationToken) {
    // ── Phase 1: Find a stone to tend to ──────────────────────
    let stone_endpoint = if let Some(ref explicit) = state.explicit_stone {
        tracing::info!(endpoint = %explicit, "using explicit stone (skipping discovery)");
        let stone = TendedStone {
            stone_name: "explicit".to_string(),
            stone_id: None,
            endpoint: explicit.clone(),
            last_seen: chrono::Utc::now(),
        };
        state.tend_to(stone).await;
        explicit.clone()
    } else {
        // Discover stones via Koi mDNS
        loop {
            if shutdown.is_cancelled() {
                return;
            }

            tracing::info!("discovering stones via Koi mDNS...");
            match discovery::discover_stones(&state.koi_endpoint).await {
                Ok(stones) if !stones.is_empty() => {
                    let stone = &stones[0];
                    tracing::info!(
                        stone = %stone.stone_name,
                        ip = %stone.ip,
                        port = stone.api_port,
                        "discovered stone, binding as tended"
                    );

                    let tended = TendedStone {
                        stone_name: stone.stone_name.clone(),
                        stone_id: stone.stone_id.clone(),
                        endpoint: stone.endpoint(),
                        last_seen: chrono::Utc::now(),
                    };
                    let endpoint = tended.endpoint.clone();
                    state.tend_to(tended).await;
                    break endpoint;
                }
                Ok(_) => {
                    tracing::info!("no stones found, retrying in 10s...");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "stone discovery failed, retrying in 10s...");
                }
            }

            tokio::select! {
                _ = shutdown.cancelled() => return,
                _ = tokio::time::sleep(Duration::from_secs(10)) => continue,
            }
        }
    };

    // ── Phase 2: Bootstrap from topology ──────────────────────
    bootstrap_from_topology(&state, &stone_endpoint).await;

    // ── Phase 3: Subscribe to tools stream ────────────────────
    let mut backoff_secs = 1u64;
    loop {
        if shutdown.is_cancelled() {
            return;
        }

        // Re-resolve stone endpoint in case tending changed
        let endpoint = state
            .tended_endpoint()
            .await
            .unwrap_or_else(|| stone_endpoint.clone());

        let state_clone = state.clone();
        let result = tools_stream::subscribe_tools_stream(
            &endpoint,
            |fqid| fqid == "offering:mongodb" || fqid.starts_with("offering:mongodb:"),
            move |event| {
                handle_tool_event(&state_clone, event);
            },
        )
        .await;

        match result {
            Ok(()) => {
                tracing::info!("tools stream ended normally, reconnecting...");
                backoff_secs = 1;
            }
            Err(e) => {
                tracing::warn!(error = %e, backoff = backoff_secs, "tools stream error, reconnecting...");
                backoff_secs = (backoff_secs * 2).min(60);
            }
        }

        tokio::select! {
            _ = shutdown.cancelled() => return,
            _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => continue,
        }
    }
}

/// Bootstrap from topology — one-shot scan for existing MongoDB instances.
async fn bootstrap_from_topology(state: &AppState, stone_endpoint: &str) {
    match topology::query_topology_for_offering(stone_endpoint, "mongodb").await {
        Ok(stones) => {
            tracing::info!(
                count = stones.len(),
                "bootstrapped MongoDB instances from topology"
            );
            for s in stones {
                let mongo_endpoint = format!("{}:27017", s.hostname);
                let instance = MongoInstance {
                    stone_id: s.stone_id.clone(),
                    stone_name: s.stone_name.clone(),
                    moss_endpoint: s.moss_endpoint(),
                    mongo_endpoint: mongo_endpoint.clone(),
                    fqn: "mongodb".to_string(), // Default FQN; refined by tools stream
                    health: InstanceHealth::Unknown,
                    role: None,
                    last_seen: Instant::now(),
                };
                state.upsert_instance(instance).await;
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "topology bootstrap failed (will rely on tools stream)");
        }
    }
}

/// Handle a single tool stream event.
fn handle_tool_event(state: &AppState, event: ToolStreamEvent) {
    match event {
        ToolStreamEvent::OfferingDiscovered {
            stone_id,
            stone_name,
            endpoint,
            tool_fqid,
            ready,
        } => {
            // Derive FQN from tool_fqid:
            //   "offering:mongodb"           → "mongodb"
            //   "offering:mongodb:analytics"  → "mongodb:analytics"
            let fqn = tool_fqid
                .strip_prefix("offering:")
                .unwrap_or("mongodb")
                .to_string();

            // Extract host:port from endpoint URL.
            // The tools stream constructs endpoints using the connection protocol
            // (mongodb://, tcp://, http://, etc.) so we strip any scheme.
            let mongo_endpoint = strip_scheme(&endpoint);

            // Spawn a task to update state (we're in a sync callback)
            let state = state.clone();
            tokio::spawn(async move {
                // Suppress registration if a pending removal exists for this endpoint
                if state.has_pending_removal(&mongo_endpoint).await {
                    tracing::trace!(
                        stone = %stone_name,
                        endpoint = %mongo_endpoint,
                        "suppressing discovery (pending removal action)"
                    );
                    return;
                }

                // Determine health based on readiness:
                // ready=true  → Unknown (health monitor will promote to Healthy)
                // ready=false → Stopped (container is down, skip probing)
                let health = if ready {
                    InstanceHealth::Unknown
                } else {
                    InstanceHealth::Stopped
                };

                let host = mongo_endpoint.split(':').next().unwrap_or("127.0.0.1");
                let instance = MongoInstance {
                    stone_id: stone_id.clone(),
                    stone_name: stone_name.clone(),
                    moss_endpoint: format!("http://{}:{}", host, garden_common::constants::MOSS_HTTP),
                    mongo_endpoint: mongo_endpoint.clone(),
                    fqn: fqn.clone(),
                    health,
                    role: None,
                    last_seen: Instant::now(),
                };

                let is_new = state.upsert_instance(instance).await;
                if is_new {
                    tracing::info!(
                        stone = %stone_name,
                        endpoint = %endpoint,
                        fqn = %fqn,
                        ready = ready,
                        "MongoDB instance discovered via tools stream"
                    );
                } else if !ready {
                    // Existing instance transitioned to stopped — update health
                    let mut reg = state.instances.write().await;
                    if let Some(inst) = reg.get_mut(&mongo_endpoint) {
                        if inst.health != InstanceHealth::Stopped {
                            tracing::info!(
                                stone = %stone_name,
                                "MongoDB instance stopped (container down)"
                            );
                            inst.health = InstanceHealth::Stopped;
                        }
                    }
                } else {
                    // Existing instance with ready=true — if it was Stopped, transition to Unknown
                    let mut reg = state.instances.write().await;
                    if let Some(inst) = reg.get_mut(&mongo_endpoint) {
                        if inst.health == InstanceHealth::Stopped {
                            tracing::info!(
                                stone = %stone_name,
                                "MongoDB instance restarted (transitioning from stopped)"
                            );
                            inst.health = InstanceHealth::Unknown;
                        }
                        inst.last_seen = Instant::now();
                    }
                }
            });
        }
        ToolStreamEvent::OfferingRemoved {
            stone_id: _,
            stone_name,
        } => {
            tracing::info!(
                stone = %stone_name,
                "MongoDB instance removed via tools stream"
            );

            // Find and remove the matching instance (offering uninstalled)
            let state = state.clone();
            tokio::spawn(async move {
                let endpoint = {
                    let reg = state.instances.read().await;
                    reg.values()
                        .find(|i| i.stone_name == stone_name)
                        .map(|i| i.mongo_endpoint.clone())
                };
                if let Some(ep) = endpoint {
                    // Also clean up any pending action for this endpoint
                    state.complete_action(&ep).await;
                    state.remove_instance(&ep).await;
                }
            });
        }
        ToolStreamEvent::Heartbeat => {
            tracing::trace!("tools stream heartbeat");
        }
    }
}

/// Strip any URI scheme prefix (e.g. `mongodb://`, `tcp://`, `http://`) and
/// return the bare `host:port` string.
fn strip_scheme(endpoint: &str) -> String {
    match endpoint.find("://") {
        Some(pos) => endpoint[pos + 3..].to_string(),
        None => endpoint.to_string(),
    }
}
