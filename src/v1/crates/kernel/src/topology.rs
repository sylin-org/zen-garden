//! Topology: who is in the garden, as events (L18).
//!
//! A handler that claims `STONE_CHIRP` and `STONE_GOODBYE`, keeps the
//! peer map, and publishes changes on a broadcast channel plus a version
//! watch. Readers take snapshots; nobody polls anyone.

use crate::dispatch::Dispatcher;
use crate::ingress::Ingested;
use garden_contract::chirp::ChirpBody;
use garden_contract::consts::announcement;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, watch};
use tokio_util::sync::CancellationToken;

/// What Topology saw. The domain event stream for the garden's membership.
/// `Seen` boxes its payload: this travels a broadcast channel to every
/// subscriber, and the chirp body is the one heavyweight thing we copy.
#[derive(Debug, Clone)]
pub enum TopologyEvent {
    /// A peer chirped (new or updated).
    Seen(Box<StoneView>),
    /// A peer said goodbye — offline immediately, no threshold wait.
    Goodbye { stone_id: String, stone_name: String },
    /// Silence exceeded the offline threshold.
    Expired { stone_id: String, stone_name: String },
}

/// A peer as the garden currently sees it.
#[derive(Debug, Clone)]
pub struct StoneView {
    pub body: ChirpBody,
    pub last_seen: chrono::DateTime<chrono::Utc>,
    pub chirps: u64,
}

#[derive(Default)]
struct Peers {
    map: HashMap<String, StoneView>,
    version: u64,
}

/// Topology state + event wiring. Clone freely.
#[derive(Clone)]
pub struct Topology {
    peers: Arc<parking_lot::Mutex<Peers>>,
    events_tx: broadcast::Sender<TopologyEvent>,
    version_tx: watch::Sender<u64>,
    chirps_total: Arc<AtomicU64>,
}

impl Default for Topology {
    fn default() -> Self {
        Self::new()
    }
}

impl Topology {
    pub fn new() -> Self {
        let (events_tx, _) = broadcast::channel(256);
        let (version_tx, _) = watch::channel(0);
        Self {
            peers: Arc::new(parking_lot::Mutex::new(Peers::default())),
            events_tx,
            version_tx,
            chirps_total: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Subscribe to membership events (L18: events, not polls).
    pub fn events(&self) -> broadcast::Receiver<TopologyEvent> {
        self.events_tx.subscribe()
    }

    /// Version watch — bump on every map change. Await it to react to change.
    pub fn version(&self) -> watch::Receiver<u64> {
        self.version_tx.subscribe()
    }

    /// Snapshot of the current garden.
    pub fn snapshot(&self) -> Vec<StoneView> {
        self.peers.lock().map.values().cloned().collect()
    }

    pub fn chirps_total(&self) -> u64 {
        self.chirps_total.load(Ordering::Relaxed)
    }

    /// Claim this Topology's message types on the dispatcher and drive the
    /// handler queue until cancelled. One Topology, one puller.
    pub fn claim(self: &Arc<Self>, dispatcher: &Dispatcher, token: CancellationToken) {
        let mut chirps = dispatcher.claim(announcement::STONE_CHIRP);
        let mut goodbyes = dispatcher.claim(announcement::STONE_GOODBYE);
        let mut responses = dispatcher.claim(announcement::DISCOVERY_RESPONSE);
        let this = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = token.cancelled() => return,
                    msg = chirps.recv() => match msg {
                        Some(m) => this.on_chirp(m).await,
                        None => return,
                    },
                    msg = goodbyes.recv() => match msg {
                        Some(m) => this.on_goodbye(m).await,
                        None => return,
                    },
                    msg = responses.recv() => match msg {
                        Some(m) => this.on_response(m).await,
                        None => return,
                    },
                }
            }
        });
    }

    async fn on_chirp(self: &Arc<Self>, msg: Ingested) {
        let Ok(body) = serde_json::from_value::<ChirpBody>(msg.announcement.data) else {
            tracing::debug!(source = %msg.source, "chirp body undecodable");
            return;
        };
        self.chirps_total.fetch_add(1, Ordering::Relaxed);
        let event = {
            let mut peers = self.peers.lock();
            let event = {
                let entry =
                    peers.map.entry(body.stone_id.clone()).or_insert_with(|| StoneView {
                        body: body.clone(),
                        last_seen: msg.received_at,
                        chirps: 0,
                    });
                entry.chirps += 1;
                entry.last_seen = msg.received_at;
                entry.body = body.clone();
                TopologyEvent::Seen(Box::new(StoneView {
                    body,
                    last_seen: entry.last_seen,
                    chirps: entry.chirps,
                }))
            };
            peers.version += 1;
            event
        };
        self.publish(event);
    }
    /// A discovery response is a hint, not a chirp: we know a stone exists
    /// at an address but nothing of its offerings or health. Record it as
    /// `starting` so observe shows it immediately; the stone's own next
    /// chirp overwrites this entry with the full truth.
    async fn on_response(self: &Arc<Self>, msg: Ingested) {
        use garden_glossary::{health, presence};
        let Ok(resp) = serde_json::from_value::<garden_contract::discovery::DiscoveryResponse>(
            msg.announcement.data,
        ) else {
            tracing::debug!(source = %msg.source, "discovery response undecodable");
            return;
        };
        let now = chrono::Utc::now();
        let event = {
            let mut peers = self.peers.lock();
            let stone_id = resp.stone_id.clone().unwrap_or_else(|| resp.stone_name.clone());
            // A real chirp (proto stamped) already told us more than any
            // hint can; never downgrade it.
            if peers.map.get(&stone_id).is_some_and(|p| p.body.proto.is_some()) {
                return;
            }
            let hint_body = ChirpBody {
                stone_id,
                stone_name: resp.stone_name,
                address: resp.address,
                moss_version: resp.moss_version,
                services: Vec::new(),
                health: health::STARTING.into(),
                status: presence::ONLINE.into(),
                discovered_at: now,
                last_seen: now,
                mac: None,
                proto: None,
                boot_id: None,
                seq: None,
            };
            let event = {
                let entry = peers.map.entry(hint_body.stone_id.clone()).or_insert_with(|| StoneView {
                    body: hint_body.clone(),
                    last_seen: now,
                    chirps: 0,
                });
                entry.last_seen = now;
                entry.body = hint_body;
                TopologyEvent::Seen(Box::new(StoneView {
                    body: entry.body.clone(),
                    last_seen: entry.last_seen,
                    chirps: entry.chirps,
                }))
            };
            peers.version += 1;
            event
        };
        self.publish(event);
    }

    async fn on_goodbye(self: &Arc<Self>, msg: Ingested) {        // The PoC goodbye body is the chirp body; fall back to name-only.
        let Ok(body) = serde_json::from_value::<ChirpBody>(msg.announcement.data) else {
            return;
        };
        let event = {
            let mut peers = self.peers.lock();
            peers.map.remove(&body.stone_id);
            peers.version += 1;
            TopologyEvent::Goodbye { stone_id: body.stone_id, stone_name: body.stone_name }
        };
        self.publish(event);
    }

    /// Protocol-periodic liveness sweep (R2.8 allows: the threshold IS the
    /// protocol). Emits `Expired` for peers silent past the threshold.
    pub async fn run_expiry(self: Arc<Self>, token: CancellationToken, threshold_secs: u64) {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(
            (threshold_secs / 3).max(5),
        ));
        loop {
            tokio::select! {
                _ = token.cancelled() => return,
                _ = ticker.tick() => {
                    let cutoff = chrono::Duration::seconds(threshold_secs as i64);
                    let now = chrono::Utc::now();
                    let expired: Vec<(String, String)> = {
                        let mut peers = self.peers.lock();
                        let before = peers.map.len();
                        let expired: Vec<(String, String)> = peers
                            .map
                            .iter()
                            .filter(|(_, p)| now - p.last_seen > cutoff)
                            .map(|(id, p)| (id.clone(), p.body.stone_name.clone()))
                            .collect();
                        for (id, _) in &expired {
                            peers.map.remove(id);
                        }
                        if !expired.is_empty() {
                            peers.version += 1;
                        }
                        let _ = before;
                        expired
                    };
                    for (stone_id, stone_name) in expired {
                        self.publish(TopologyEvent::Expired { stone_id, stone_name });
                    }
                }
            }
        }
    }

    fn publish(&self, event: TopologyEvent) {
        let _ = self.events_tx.send(event);
        let version = self.peers.lock().version;
        self.version_tx.send_replace(version);
    }
}
