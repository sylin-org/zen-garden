//! Topology: who is in the garden, as events (L18).
//!
//! A handler that claims `STONE_CHIRP` and `STONE_GOODBYE`, keeps the
//! peer map, and publishes changes on a broadcast channel plus a version
//! watch. Readers take snapshots; nobody polls anyone.
//!
//! The cache stores the CANONICAL frame (one shape: wire == cache == HTTP,
//! charter bet B1). Its `received` section is the listener's own reception
//! record — senders emit placeholders; here they are overwritten, first
//! sight keeps `discovered_at`, every frame refreshes `last_seen`.

use crate::dispatch::Dispatcher;
use crate::ingress::Ingested;
use garden_contract::chirp::ChirpFrame;
use garden_contract::consts::announcement;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, watch};
use tokio_util::sync::CancellationToken;

/// What Topology saw. The domain event stream for the garden's membership.
/// `Seen` boxes its payload: this travels a broadcast channel to every
/// subscriber, and the frame is the one heavyweight thing we copy.
#[derive(Debug, Clone)]
pub enum TopologyEvent {
    /// A peer chirped (new or updated).
    Seen(Box<StoneView>),
    /// A peer said goodbye — offline immediately, no threshold wait.
    Goodbye { stone_id: String, stone_name: String },
    /// Silence exceeded the offline threshold.
    Expired { stone_id: String, stone_name: String },
}

/// A peer as the garden currently sees it: its announced frame plus OUR
/// reception facts about the relationship.
#[derive(Debug, Clone)]
pub struct StoneView {
    /// The stone's own announced truth (identity, presence, inventory).
    pub body: ChirpFrame,
    /// When we last heard from it.
    pub last_seen: chrono::DateTime<chrono::Utc>,
    /// How many frames we've accepted.
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
        let Ok(mut frame) = serde_json::from_value::<ChirpFrame>(msg.announcement.data) else {
            tracing::debug!(source = %msg.source, "chirp frame undecodable");
            return;
        };
        // Reception facts are OURS: overwrite the placeholders the sender
        // emitted (first sight keeps discovered_at via or_insert below).
        frame.received.last_seen = msg.received_at;
        self.chirps_total.fetch_add(1, Ordering::Relaxed);
        let event = {
            let mut peers = self.peers.lock();
            let event = {
                let entry =
                    peers.map.entry(frame.stone.id.clone()).or_insert_with(|| StoneView {
                        body: {
                            let mut f = frame.clone();
                            f.received.discovered_at = msg.received_at;
                            f
                        },
                        last_seen: msg.received_at,
                        chirps: 0,
                    });
                entry.chirps += 1;
                entry.last_seen = msg.received_at;
                entry.body = frame.clone();
                TopologyEvent::Seen(Box::new(StoneView {
                    body: frame,
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
    /// at an address but nothing of its offerings beyond what a rich answer
    /// carried. Record it as `starting` so observe shows it immediately; the
    /// stone's own next chirp overwrites this entry with the full truth.
    async fn on_response(self: &Arc<Self>, msg: Ingested) {
        let Ok(resp) = serde_json::from_value::<garden_contract::discovery::DiscoveryResponse>(
            msg.announcement.data,
        ) else {
            tracing::debug!(source = %msg.source, "discovery response undecodable");
            return;
        };
        let now = chrono::Utc::now();
        let event = {
            let mut peers = self.peers.lock();
            let stone_id = if resp.stone.id.is_empty() {
                resp.stone.name.clone()
            } else {
                resp.stone.id.clone()
            };
            // A real frame (proto stamped) already told us more than any
            // hint can; never downgrade it.
            if peers
                .map
                .get(&stone_id)
                .is_some_and(|p| p.body.meta.proto.is_some())
            {
                return;
            }
            let mut hint = ChirpFrame::answered(
                resp.stone.name.clone(),
                resp.stone.network.address.clone(),
                resp.stone.moss.version.clone(),
            );
            hint.stone.id = stone_id.clone();
            if let Some(inv) = resp.services {
                hint.services = inv;
            }
            hint.received.discovered_at = now;
            let event = {
                let entry = peers.map.entry(stone_id).or_insert_with(|| StoneView {
                    body: hint.clone(),
                    last_seen: now,
                    chirps: 0,
                });
                entry.last_seen = now;
                entry.body = hint.clone();
                TopologyEvent::Seen(Box::new(StoneView {
                    body: hint,
                    last_seen: entry.last_seen,
                    chirps: entry.chirps,
                }))
            };
            peers.version += 1;
            event
        };
        self.publish(event);
    }

    async fn on_goodbye(self: &Arc<Self>, msg: Ingested) {
        // The goodbye payload is the frame; fall back to name-keyed removal.
        let Ok(frame) = serde_json::from_value::<ChirpFrame>(msg.announcement.data) else {
            return;
        };
        let event = {
            let mut peers = self.peers.lock();
            peers.map.remove(&frame.stone.id);
            peers.version += 1;
            TopologyEvent::Goodbye {
                stone_id: frame.stone.id,
                stone_name: frame.stone.name,
            }
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
                        let expired: Vec<(String, String)> = peers
                            .map
                            .iter()
                            .filter(|(_, p)| now - p.last_seen > cutoff)
                            .map(|(id, p)| (id.clone(), p.body.stone.name.clone()))
                            .collect();
                        for (id, _) in &expired {
                            peers.map.remove(id);
                        }
                        if !expired.is_empty() {
                            peers.version += 1;
                        }
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
