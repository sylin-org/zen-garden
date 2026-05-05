//! Continuous topology awareness via UDP chirps + provoked discovery.
//!
//! Mirrors Moss's mechanism (see `src/moss/src/tasks/discovery.rs`):
//! subscribe to `STONE_CHIRP`, keep an in-memory cache keyed by
//! `stone_id`, evict entries that haven't been heard from in 90s.
//!
//! On startup, additionally send one `DISCOVERY_REQUEST` and listen
//! for `DISCOVERY_RESPONSE` so stones reply within milliseconds rather
//! than waiting up to a chirp interval (~30s). This is the same flow
//! Rake uses for its initial scan.
//!
//! Topology changes (add / update / evict) emit a `topology-changed`
//! Tauri event whose payload is the current snapshot — the React app
//! listens via `listen("topology-changed", …)` and re-renders.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use garden_common::infra::communications::{announcement_types, p2p};
use garden_common::{DiscoveryRequest, DiscoveryResponse, TopologyEntry};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::RwLock;

use crate::announce::{Announcer, GardenEvent};

/// Stones whose last sighting is older than this are evicted from the
/// cache. Matches Moss's 90-second topology TTL.
const TTL: Duration = Duration::from_secs(90);

/// Eviction sweep cadence.
const EVICTION_INTERVAL: Duration = Duration::from_secs(15);

/// Tauri event name for topology changes.
pub const EVENT_TOPOLOGY_CHANGED: &str = "topology-changed";

/// Snapshot row for the dashboard. Subset of [`TopologyEntry`] —
/// only what the UI actually renders.
#[derive(Debug, Clone, Serialize)]
pub struct AwareStone {
    pub stone_id: String,
    pub stone_name: String,
    pub endpoint: String,
    /// `unknown` until at least one chirp has arrived (DISCOVERY_RESPONSE
    /// alone doesn't carry health). Otherwise: `thriving`, `degraded`, etc.
    pub health: String,
    pub services_count: usize,
    pub last_seen: DateTime<Utc>,
    /// Seconds since most recent chirp / response (for TTL fading).
    pub age_secs: u64,
    /// Seconds since this stone first appeared in awareness (for
    /// "first-by-response" auto-tend ordering).
    pub seen_first_secs: u64,
}

pub struct Awareness {
    stones: Arc<RwLock<HashMap<String, CacheEntry>>>,
    app: AppHandle,
    announcer: Announcer,
}

#[derive(Clone)]
struct CacheEntry {
    stone_id: String,
    stone_name: String,
    endpoint: String,
    /// `unknown` if only discovery-response received; chirp populates this.
    health: String,
    services_count: usize,
    /// Latest reported wall-clock — chirp's `last_seen` field, or `Utc::now()`
    /// for discovery-response which has no such field.
    last_seen: DateTime<Utc>,
    /// Latest sighting (chirp OR discovery-response). Drives TTL.
    received_at: Instant,
    /// First sighting. Drives "first-by-response" auto-tend ordering.
    first_seen_at: Instant,
}

impl Awareness {
    pub fn new(app: AppHandle, announcer: Announcer) -> Self {
        Self {
            stones: Arc::new(RwLock::new(HashMap::new())),
            app,
            announcer,
        }
    }

    /// Current topology snapshot for the UI, sorted by first-seen
    /// ascending (oldest entry first — i.e. first-to-respond first).
    pub async fn snapshot(&self) -> Vec<AwareStone> {
        let stones = self.stones.read().await;
        let now = Instant::now();
        let mut snap: Vec<(AwareStone, Instant)> = stones
            .values()
            .map(|e| {
                let row = AwareStone {
                    stone_id: e.stone_id.clone(),
                    stone_name: e.stone_name.clone(),
                    endpoint: e.endpoint.clone(),
                    health: e.health.clone(),
                    services_count: e.services_count,
                    last_seen: e.last_seen,
                    age_secs: now.saturating_duration_since(e.received_at).as_secs(),
                    seen_first_secs: now.saturating_duration_since(e.first_seen_at).as_secs(),
                };
                (row, e.first_seen_at)
            })
            .collect();
        snap.sort_by(|a, b| b.1.cmp(&a.1)); // ascending first_seen = descending Instant
        snap.into_iter().map(|(s, _)| s).collect()
    }

    /// Spawn the chirp + discovery-response listeners + TTL eviction
    /// sweep. Sends one `DISCOVERY_REQUEST` so stones respond within
    /// milliseconds (rather than waiting ~30s for the next chirp).
    pub fn spawn_listeners(self: &Arc<Self>) {
        let chirp_handle = self.clone();
        tauri::async_runtime::spawn(async move { chirp_handle.run_chirp_listener().await });

        let response_handle = self.clone();
        tauri::async_runtime::spawn(async move { response_handle.run_response_listener().await });

        let evict_handle = self.clone();
        tauri::async_runtime::spawn(async move { evict_handle.run_ttl_eviction().await });

        let probe_handle = self.clone();
        tauri::async_runtime::spawn(async move { probe_handle.send_discovery_probe().await });
    }

    async fn run_chirp_listener(self: Arc<Self>) {
        let mut rx = match p2p::subscribe_to_announcement(announcement_types::STONE_CHIRP).await {
            Ok(rx) => rx,
            Err(e) => {
                tracing::error!(error = %e, "awareness: failed to subscribe to STONE_CHIRP");
                return;
            }
        };
        tracing::info!("awareness: chirp listener active");

        while let Some((payload, addr)) = rx.recv().await {
            let chirp: TopologyEntry = match serde_json::from_value(payload) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(error = %e, ?addr, "awareness: malformed chirp dropped");
                    continue;
                }
            };
            self.ingest_chirp(chirp, addr.to_string()).await;
        }
        tracing::warn!("awareness: chirp channel closed");
    }

    async fn run_response_listener(self: Arc<Self>) {
        let mut rx =
            match p2p::subscribe_to_announcement(announcement_types::DISCOVERY_RESPONSE).await {
                Ok(rx) => rx,
                Err(e) => {
                    tracing::error!(error = %e, "awareness: failed to subscribe to DISCOVERY_RESPONSE");
                    return;
                }
            };
        tracing::info!("awareness: discovery-response listener active");

        while let Some((payload, addr)) = rx.recv().await {
            let resp: DiscoveryResponse = match serde_json::from_value(payload) {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(error = %e, ?addr, "awareness: malformed discovery response dropped");
                    continue;
                }
            };
            self.ingest_discovery_response(resp, addr.to_string()).await;
        }
        tracing::warn!("awareness: discovery-response channel closed");
    }

    async fn run_ttl_eviction(self: Arc<Self>) {
        loop {
            tokio::time::sleep(EVICTION_INTERVAL).await;
            let evicted_entries: Vec<(String, String)> = {
                let mut stones = self.stones.write().await;
                let mut gone = Vec::new();
                stones.retain(|id, e| {
                    let alive = e.received_at.elapsed() < TTL;
                    if !alive {
                        gone.push((id.clone(), e.stone_name.clone()));
                    }
                    alive
                });
                gone
            };
            if !evicted_entries.is_empty() {
                tracing::info!(evicted = evicted_entries.len(), "awareness: TTL eviction");
                for (stone_id, stone_name) in &evicted_entries {
                    self.announcer
                        .observe(GardenEvent::StoneLeft {
                            stone_id: stone_id.clone(),
                            stone_name: stone_name.clone(),
                        })
                        .await;
                }
                self.emit_topology().await;
            }
        }
    }

    /// Send a single `DISCOVERY_REQUEST` so stones reply with
    /// `DISCOVERY_RESPONSE` within ms instead of us waiting up to a
    /// chirp interval (~30s). After this one shot, awareness sits and
    /// listens — no further provocation.
    async fn send_discovery_probe(self: Arc<Self>) {
        // Brief delay so the response listener is wired before the
        // probe goes out (otherwise we miss replies from very-fast
        // stones).
        tokio::time::sleep(Duration::from_millis(250)).await;

        let request = DiscoveryRequest {
            discover: "moss".into(),
            request_id: uuid::Uuid::now_v7().to_string(),
            requester: "pavilion".into(),
        };
        match p2p::send_announcement(announcement_types::DISCOVERY_REQUEST, &request).await {
            Ok(()) => tracing::info!(request_id = %request.request_id, "awareness: discovery probe sent"),
            Err(e) => tracing::warn!(error = %e, "awareness: discovery probe failed"),
        }
    }

    async fn ingest_chirp(&self, chirp: TopologyEntry, from: String) {
        let stone_id = chirp.stone_id.clone();
        let now = Instant::now();
        let (was_new, name_for_event, endpoint_for_event) = {
            let mut stones = self.stones.write().await;
            let was_new = !stones.contains_key(&stone_id);
            let entry = stones.entry(stone_id.clone()).or_insert_with(|| CacheEntry {
                stone_id: stone_id.clone(),
                stone_name: chirp.stone_name.clone(),
                endpoint: chirp.address.http_base(),
                health: String::from("unknown"),
                services_count: 0,
                last_seen: chirp.last_seen,
                received_at: now,
                first_seen_at: now,
            });
            // Chirp is the source of truth for the rich fields.
            entry.stone_name = chirp.stone_name;
            entry.endpoint = chirp.address.http_base();
            entry.health = chirp.health;
            entry.services_count = chirp.services.len();
            entry.last_seen = chirp.last_seen;
            entry.received_at = now;
            (was_new, entry.stone_name.clone(), entry.endpoint.clone())
        };
        if was_new {
            tracing::info!(stone_id = %stone_id, %from, "awareness: new stone via chirp");
            self.announcer
                .observe(GardenEvent::StoneJoined {
                    stone_id: stone_id.clone(),
                    stone_name: name_for_event,
                    endpoint: endpoint_for_event,
                })
                .await;
        }
        self.emit_topology().await;
    }

    async fn ingest_discovery_response(&self, resp: DiscoveryResponse, from: String) {
        let stone_id = resp
            .stone_id
            .clone()
            .unwrap_or_else(|| resp.stone_name.clone());
        let now = Instant::now();
        let (was_new, name_for_event, endpoint_for_event) = {
            let mut stones = self.stones.write().await;
            let was_new = !stones.contains_key(&stone_id);
            let entry = stones.entry(stone_id.clone()).or_insert_with(|| CacheEntry {
                stone_id: stone_id.clone(),
                stone_name: resp.stone_name.clone(),
                endpoint: resp.address.http_base(),
                health: String::from("unknown"),
                services_count: 0,
                last_seen: Utc::now(),
                received_at: now,
                first_seen_at: now,
            });
            // Discovery-response refreshes identity/endpoint and TTL but
            // does NOT clobber chirp-populated health / services_count.
            entry.stone_name = resp.stone_name;
            entry.endpoint = resp.address.http_base();
            entry.last_seen = Utc::now();
            entry.received_at = now;
            (was_new, entry.stone_name.clone(), entry.endpoint.clone())
        };
        if was_new {
            tracing::info!(stone_id = %stone_id, %from, "awareness: new stone via discovery response");
            self.announcer
                .observe(GardenEvent::StoneJoined {
                    stone_id: stone_id.clone(),
                    stone_name: name_for_event,
                    endpoint: endpoint_for_event,
                })
                .await;
        }
        self.emit_topology().await;
    }

    async fn emit_topology(&self) {
        let snap = self.snapshot().await;
        if let Err(e) = self.app.emit(EVENT_TOPOLOGY_CHANGED, &snap) {
            tracing::warn!(error = %e, "awareness: failed to emit topology-changed");
        }
    }
}
