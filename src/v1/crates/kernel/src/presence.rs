//! Presence: who is in the garden, as events (L18).
//!
//! A handler that claims `STONE_CHIRP` and `STONE_GOODBYE`, keeps the
//! peer map, and publishes changes on a broadcast channel plus a version
//! watch. Readers take snapshots; nobody polls anyone.

use crate::dispatch::Dispatcher;
use crate::ingress::Ingested;
use garden_contract::chirp::ChirpBody;
use garden_contract::chirp::ChirpBody;
use garden_contract::consts::announcement;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, watch};
use tokio_util::sync::CancellationToken;

/// What presence saw. The domain event stream for the garden's membership.
#[derive(Debug, Clone)]
pub enum PresenceEvent {
    /// A peer chirped (new or updated).
    Seen(PeerView),
    /// A peer said goodbye — offline immediately, no threshold wait.
    Goodbye { stone_id: String, stone_name: String },
    /// Silence exceeded the offline threshold.
    Expired { stone_id: String, stone_name: String },
}

/// A peer as the garden currently sees it.
#[derive(Debug, Clone)]
pub struct PeerView {
    pub body: ChirpBody,
    pub last_seen: chrono::DateTime<chrono::Utc>,
    pub chirps: u64,
}

#[derive(Default)]
struct Peers {
    map: HashMap<String, PeerView>,
    version: u64,
}

/// Presence state + event wiring. Clone freely.
#[derive(Clone)]
pub struct Presence {
    peers: Arc<Mutex<Peers>>,
    events_tx: broadcast::Sender<PresenceEvent>,
    version_tx: watch::Sender<u64>,
    chirps_total: Arc<AtomicU64>,
}

impl Presence {
    pub fn new() -> Self {
        let (events_tx, _) = broadcast::channel(256);
        let (version_tx, _) = watch::channel(0);
        Self {
            peers: Arc::new(Mutex::new(Peers::default())),
            events_tx,
            version_tx,
            chirps_total: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Subscribe to membership events (L18: events, not polls).
    pub fn events(&self) -> broadcast::Receiver<PresenceEvent> {
        self.events_tx.subscribe()
    }

    /// Version watch — bump on every map change. Await it to react to change.
    pub fn version(&self) -> watch::Receiver<u64> {
        self.version_tx.subscribe()
    }

    /// Snapshot of the current garden.
    pub fn snapshot(&self) -> Vec<PeerView> {
        self.peers.lock().expect("peers lock").map.values().cloned().collect()
    }

    pub fn chirps_total(&self) -> u64 {
        self.chirps_total.load(Ordering::Relaxed)
    }

    /// Claim this presence's message types on the dispatcher and drive the
    /// handler queue until cancelled. One presence, one puller.
    pub fn claim(self: &Arc<Self>, dispatcher: &Dispatcher, token: CancellationToken) {
        let chirps = dispatcher.claim(announcement::STONE_CHIRP);
        let goodbyes = dispatcher.claim(announcement::STONE_GOODBYE);
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
            let mut peers = self.peers.lock().expect("peers lock");
            let entry = peers.map.entry(body.stone_id.clone()).or_insert_with(|| PeerView {
                body: body.clone(),
                last_seen: msg.received_at,
                chirps: 0,
            });
            entry.chirps += 1;
            entry.last_seen = msg.received_at;
            entry.body = body.clone();
            peers.version += 1;
            PresenceEvent::Seen(PeerView {
                body,
                last_seen: entry.last_seen,
                chirps: entry.chirps,
            })
        };
        self.publish(event);
    }

    async fn on_goodbye(self: &Arc<Self>, msg: Ingested) {
        // The PoC goodbye body is the chirp body; fall back to name-only.
        let Ok(body) = serde_json::from_value::<ChirpBody>(msg.announcement.data) else {
            return;
        };
        let event = {
            let mut peers = self.peers.lock().expect("peers lock");
            peers.map.remove(&body.stone_id);
            peers.version += 1;
            PresenceEvent::Goodbye { stone_id: body.stone_id, stone_name: body.stone_name }
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
                        let mut peers = self.peers.lock().expect("peers lock");
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
                        self.publish(PresenceEvent::Expired { stone_id, stone_name });
                    }
                }
            }
        }
    }

    fn publish(&self, event: PresenceEvent) {
        let _ = self.events_tx.send(event);
        let version = self.peers.lock().expect("peers lock").version;
        self.version_tx.send_replace(version);
    }
}
