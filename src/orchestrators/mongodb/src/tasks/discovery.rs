//! Stone discovery task for the MongoDB orchestrator.
//!
//! Uses the shared `resilient_stream` runner from orchestrator-common for:
//! - Initial stone resolution (local → registry → Koi mDNS)
//! - Tools API stream subscription with exponential backoff
//! - Automatic failover with endpoint blacklisting
//!
//! MongoDB-specific logic lives here:
//! - Topology bootstrap (register discovered instances + catalog identities)
//! - Tool stream event handling (offering discovered/removed → state updates)

use crate::app_state::AppState;
use crate::domain::types::{InstanceHealth, MongoInstance, PendingAction};
use garden_common::offerings::OfferingFqn;
use orchestrator_common::resilient_stream::{self, StreamConfig, StreamContext};
use orchestrator_common::stone_catalog::{ServiceKey, StoneIdentity};
use orchestrator_common::tools_stream::ToolStreamEvent;
use orchestrator_common::topology;
use std::collections::HashMap;
use std::time::Instant;
use tokio_util::sync::CancellationToken;

/// Run the discovery task.
///
/// 1. Resolve a stone (local → registry → Koi mDNS, with failover)
/// 2. Bootstrap from topology
/// 3. Subscribe to tools stream for real-time changes
/// 4. On persistent failure → blacklist + failover to another stone
pub async fn run(state: AppState, shutdown: CancellationToken) {
    // Bootstrap from topology when the initial stone is resolved
    // (and again on each failover — see on_stone_switched)
    let bootstrap_state = state.clone();

    let ctx = StreamContext {
        koi_endpoint: state.koi_endpoint.clone(),

        local_endpoint: Some(format!(
            "http://localhost:{}",
            garden_common::constants::MOSS_HTTP,
        )),

        explicit_stone: state.explicit_stone.clone(),

        fqid_filter: |fqid: &str| {
            fqid == "offering:mongodb" || fqid.starts_with("offering:mongodb::")
        },

        on_event: {
            let state = state.clone();
            move |event| handle_tool_event(&state, event)
        },

        candidate_endpoints: {
            let state = state.clone();
            Box::new(move || {
                let state = state.clone();
                Box::pin(async move {
                    let reg = state.instances.read().await;
                    reg.values()
                        .map(|i| i.moss_endpoint.clone())
                        .collect()
                })
            })
        },

        on_stone_selected: {
            let state = state.clone();
            Box::new(move |tended| {
                let state = state.clone();
                Box::pin(async move {
                    state.tend_to(tended).await;
                })
            })
        },

        on_stone_switched: {
            let state = bootstrap_state;
            Box::new(move |endpoint: String| {
                let state = state.clone();
                Box::pin(async move {
                    bootstrap_from_topology(&state, &endpoint).await;
                })
            })
        },

        config: StreamConfig::default(),
    };

    // The resilient stream handles everything:
    // - Initial stone resolution (local → registry → Koi mDNS)
    // - Topology bootstrap via on_stone_switched (called on both initial and failover)
    // - Tools stream subscription with automatic reconnection
    // - Endpoint blacklisting and failover after consecutive failures
    resilient_stream::run_resilient_stream(ctx, shutdown).await;
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
                // Use IP for endpoint — .local mDNS unreliable in Docker on Windows
                let mongo_endpoint = format!("{}:27017", s.ip);
                let moss_endpoint = s.moss_endpoint();

                // Register stone identity in the catalog
                let mut services = HashMap::new();
                services.insert(ServiceKey::Mongo, mongo_endpoint.clone());
                services.insert(ServiceKey::Moss, moss_endpoint.clone());

                state
                    .upsert_catalog(StoneIdentity {
                        stone_name: s.stone_name.clone(),
                        stone_id: Some(s.stone_id.clone()),
                        hostname: s.hostname.clone(),
                        ip: Some(s.ip.clone()),
                        services,
                    })
                    .await;

                let instance = MongoInstance {
                    stone_id: s.stone_id.clone(),
                    stone_name: s.stone_name.clone(),
                    moss_endpoint,
                    mongo_endpoint,
                    fqn: s.fqn.clone(),
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
            //   "offering:mongodb"             → mongodb (default)
            //   "offering:mongodb::analytics"  → mongodb::analytics
            let fqn = tool_fqid
                .strip_prefix("offering:")
                .and_then(|s| OfferingFqn::parse(s).ok())
                .unwrap_or_else(|| OfferingFqn::new("mongodb").unwrap());

            // Extract host:port from endpoint URL.
            // The tools stream constructs endpoints using the connection protocol
            // (mongodb://, tcp://, http://, etc.) so we strip any scheme.
            let mongo_endpoint_raw = strip_scheme(&endpoint);

            // Spawn a task to update state (we're in a sync callback)
            let state = state.clone();
            tokio::spawn(async move {
                // Resolve .local hostnames to IP via Koi (mDNS unreliable in Docker on Windows)
                let mongo_endpoint = orchestrator_common::discovery::resolve_endpoint(
                    &state.koi_endpoint, &mongo_endpoint_raw,
                ).await;

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

                // host is an IP (tools stream now prefers IP over hostname)
                let host = mongo_endpoint.split(':').next().unwrap_or("127.0.0.1");
                let moss_endpoint =
                    format!("http://{}:{}", host, garden_common::constants::MOSS_HTTP);

                // Register stone identity in the catalog
                let mut services = HashMap::new();
                services.insert(ServiceKey::Mongo, mongo_endpoint.clone());
                services.insert(ServiceKey::Moss, moss_endpoint.clone());

                let hostname = if stone_name.contains('.') {
                    stone_name.clone()
                } else {
                    format!("{}.local", &stone_name)
                };

                state
                    .upsert_catalog(StoneIdentity {
                        stone_name: stone_name.clone(),
                        stone_id: Some(stone_id.clone()),
                        hostname,
                        ip: Some(host.to_string()),
                        services,
                    })
                    .await;

                let instance = MongoInstance {
                    stone_id: stone_id.clone(),
                    stone_name: stone_name.clone(),
                    moss_endpoint,
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
                        fqn = %fqn.to_string(),
                        ready = ready,
                        "MongoDB instance discovered via tools stream"
                    );
                } else if !ready {
                    // Existing instance transitioned to stopped — update health
                    let mut reg = state.instances.write().await;
                    if let Some(inst) = reg.get_mut(&stone_name) {
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
                    if let Some(inst) = reg.get_mut(&stone_name) {
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

            // Queue rs.remove() before removing the instance from the registry
            // (we need the FQN and endpoint from the registry while it still exists).
            let state = state.clone();
            tokio::spawn(async move {
                let instance_data = {
                    let reg = state.instances.read().await;
                    reg.get(&stone_name)
                        .map(|i| (i.mongo_endpoint.clone(), i.fqn.clone()))
                };
                if let Some((mongo_ep, fqn)) = instance_data {
                    // Queue rs.remove() so the bootstrap task will evict the
                    // member from the replica set on its next cycle.
                    if !state.has_pending_removal(&mongo_ep).await {
                        state
                            .queue_action(PendingAction::RemoveMember {
                                mongo_endpoint: mongo_ep.clone(),
                                fqn,
                                requested_at: chrono::Utc::now(),
                            })
                            .await;
                        tracing::info!(
                            stone = %stone_name,
                            endpoint = %mongo_ep,
                            "queued rs.remove() for uprooted instance"
                        );
                    }
                }
                state.remove_instance(&stone_name).await;
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
