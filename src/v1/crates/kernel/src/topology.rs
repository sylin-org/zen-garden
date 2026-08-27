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

/// A rumor about a stone (ADR-0004 §3): knowledge heard through
/// middlemen — an overheard rich answer about a stone we have never met.
/// A candidate is never a member: it does not render in snapshots, and
/// chirp-borne truth always outranks it. Promoted to irrelevance by the
/// stone's first live frame; expired quietly by [`Topology::run_expiry`]
/// after `CANDIDATE_TTL_SECS`.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// What we heard: the answer's card and whatever inventory rode it.
    pub response: garden_contract::discovery::DiscoveryResponse,
    /// Who we heard it from (the relayer, not the stone).
    pub heard_from: std::net::SocketAddr,
    /// When the rumor was first heard.
    pub first_seen: chrono::DateTime<chrono::Utc>,
    /// When the rumor was last refreshed.
    pub last_seen: chrono::DateTime<chrono::Utc>,
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
    candidates: Arc<parking_lot::Mutex<HashMap<String, Candidate>>>,
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
            candidates: Arc::new(parking_lot::Mutex::new(HashMap::new())),
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

    /// The rumor pool (ADR-0004 §3): stones heard about but never met.
    /// Room-level resolution reads peers first, candidates second; a
    /// candidate is a rumor until a chirp confirms it.
    pub fn candidates(&self) -> Vec<Candidate> {
        self.candidates.lock().values().cloned().collect()
    }

    pub fn chirps_total(&self) -> u64 {
        self.chirps_total.load(Ordering::Relaxed)
    }

    /// Claim this Topology's message types on the dispatcher and drive the
    /// handler queue until cancelled. One Topology, one puller.
    pub fn claim(self: &Arc<Self>, dispatcher: &Dispatcher, token: CancellationToken) {
        let mut chirps = dispatcher.claim(announcement::STONE_CHIRP);
        let mut songs = dispatcher.claim(announcement::STONE_SONG);
        let mut goodbyes = dispatcher.claim(announcement::STONE_GOODBYE);
        let mut responses = dispatcher.claim(announcement::DISCOVERY_RESPONSE);
        let this = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = token.cancelled() => return,
                    msg = chirps.recv() => match msg {
                        Some(m) => this.on_frame(m).await,
                        None => return,
                    },
                    msg = songs.recv() => match msg {
                        Some(m) => this.on_frame(m).await,
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

    /// A chirp or a song — the frame shape is ONE (B1); the discriminator
    /// is the mouth, not the meaning. Anchors are per-frame authoritative;
    /// the inventory merges by per-domain rev (ADR-0004 §2), so a LEAN
    /// heartbeat never wipes what a song taught and a stale song never
    /// beats a fresh rev. Candidates and promotion remain S4's work.
    async fn on_frame(self: &Arc<Self>, msg: Ingested) {
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
                let fresh = &mut entry.body;
                fresh.stone = frame.stone.clone();
                fresh.presence = frame.presence.clone();
                fresh.meta = frame.meta.clone();
                fresh.inventory.merge_frame(&frame.inventory);
                fresh.received.last_seen = msg.received_at;
                // The stone spoke for itself: any rumor about it is
                // redundant (ADR-0004 §3 — promotion by first live frame).
                self.candidates.lock().remove(&frame.stone.id);
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

    /// An answer is a rumor, not a meeting (ADR-0004 §3): it lands in the
    /// candidates pool — stamped with who we heard it from and when — and
    /// never in the peers map. Live truth outranks: a stone we already
    /// hear from teaches us nothing through gossip, and its own frame
    /// retires the rumor anyway.
    async fn on_response(self: &Arc<Self>, msg: Ingested) {
        let Ok(resp) = serde_json::from_value::<garden_contract::discovery::DiscoveryResponse>(
            msg.announcement.data,
        ) else {
            tracing::debug!(source = %msg.source, "discovery response undecodable");
            return;
        };
        // A rumor without identity cannot be confirmed or resolved — skip.
        if resp.stone.id.is_empty() {
            return;
        }
        {
            let peers = self.peers.lock();
            if peers.map.contains_key(&resp.stone.id) {
                return;
            }
        }
        let now = chrono::Utc::now();
        let mut candidates = self.candidates.lock();
        match candidates.get_mut(&resp.stone.id) {
            Some(known) => {
                known.response = resp;
                known.heard_from = msg.source;
                known.last_seen = now;
            }
            None => {
                tracing::debug!(stone = %resp.stone.name, from = %msg.source,
                                "candidate heard; awaiting its own frame");
                candidates.insert(
                    resp.stone.id.clone(),
                    Candidate {
                        response: resp,
                        heard_from: msg.source,
                        first_seen: now,
                        last_seen: now,
                    },
                );
            }
        }
        // No version bump: candidates are not membership. The room's map
        // changed only in the gossip ledger.
    }

    /// Drop candidates silent past the TTL (ADR-0004 §3). Returns how many
    /// rumors died — callers log it; tests assert it.
    fn sweep_candidates(&self, now: chrono::DateTime<chrono::Utc>) -> usize {
        let cutoff =
            chrono::Duration::seconds(garden_contract::consts::CANDIDATE_TTL_SECS as i64);
        let mut candidates = self.candidates.lock();
        let before = candidates.len();
        candidates.retain(|_, c| now - c.last_seen <= cutoff);
        before - candidates.len()
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
                    // Rumors age out silently — they were never promised.
                    let rumors = self.sweep_candidates(now);
                    if rumors > 0 {
                        tracing::debug!(expired = rumors, "unconfirmed candidates aged out");
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use garden_contract::chirp::{
        Inventory, Moss, Network, PeerAddress, Presence, Reception, ServiceEntry, ServiceState,
        Stone,
    };
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::time::Duration;

    /// A frame for one stone, with an optional services block.
    fn frame(id: &str, services: Option<Inventory<ServiceEntry>>) -> ChirpFrame {
        let now = chrono::Utc::now();
        ChirpFrame {
            stone: Stone {
                id: id.into(),
                name: format!("stone-{id}"),
                moss: Moss { version: "1.0.0".into() },
                network: Network {
                    address: PeerAddress {
                        ip: IpAddr::from(Ipv4Addr::LOCALHOST),
                        port: 7285,
                        tls_port: None,
                    },
                    mac: None,
                },
            },
            presence: Presence {
                health: garden_glossary::health::THRIVING.into(),
                status: garden_glossary::presence::ONLINE.into(),
            },
            inventory: garden_contract::chirp::InventoryMap {
                services,
                ..Default::default()
            },
            meta: garden_contract::chirp::FrameMeta {
                proto: Some(garden_contract::consts::PROTO_V1.into()),
                boot_id: Some("boot-1".into()),
                ..Default::default()
            },
            received: Reception { discovered_at: now, last_seen: now },
        }
    }

    fn one_item(rev: u64) -> Inventory<ServiceEntry> {
        Inventory {
            rev: Some(rev),
            total: None,
            items: vec![ServiceEntry {
                offering_id: "oid-1".into(),
                name: "redis::default".into(),
                stem: "redis".into(),
                category: "data".into(),
                state: ServiceState { status: "running".into(), role: None },
                ports: Default::default(),
            }],
        }
    }

    /// Speak one frame through the claim path and wait for the cache to
    /// settle (the version watch bumps once per accepted frame).
    async fn speak(
        topology: &Arc<Topology>,
        dispatcher: &Dispatcher,
        kind: &str,
        frame: ChirpFrame,
    ) {
        let mut version = topology.version();
        let before = *version.borrow_and_update();
        dispatcher
            .ingest(Ingested {
                announcement: garden_contract::wire::Announcement::new(
                    kind,
                    serde_json::to_value(&frame).unwrap(),
                ),
                source: SocketAddr::from((Ipv4Addr::LOCALHOST, 50000)),
                received_at: chrono::Utc::now(),
            })
            .await;
        tokio::time::timeout(Duration::from_secs(2), version.changed())
            .await
            .expect("cache must settle")
            .expect("watch alive");
        assert!(*version.borrow() > before, "version advanced");
    }

    /// The two-register promise: a song teaches the set, the lean
    /// heartbeats that follow must NOT wipe it, and a genuinely newer
    /// empty-set rev still defends its truth (ADR-0004 §2).
    #[tokio::test]
    async fn lean_heartbeat_never_wipes_what_a_song_taught() {
        let (dispatcher, handle) = Dispatcher::new(16);
        let topology = Arc::new(Topology::new());
        let token = CancellationToken::new();
        topology.claim(&dispatcher, token.clone());
        tokio::spawn(handle.run(token.clone()));

        speak(
            &topology,
            &dispatcher,
            announcement::STONE_SONG,
            frame("s1", Some(one_item(2))),
        )
        .await;
        speak(
            &topology,
            &dispatcher,
            announcement::STONE_CHIRP,
            frame("s1", Some(Inventory { rev: Some(2), total: None, items: Vec::new() })),
        )
        .await;

        let stored = topology
            .snapshot()
            .into_iter()
            .find(|v| v.body.stone.id == "s1")
            .expect("peer cached");
        let inv = stored.body.inventory.services.expect("block kept");
        assert_eq!(inv.items.len(), 1, "equal-rev heartbeat keeps song items");
        assert_eq!(inv.rev, Some(2));

        // A newer rev with an empty set is a fresh truth — it wins.
        speak(
            &topology,
            &dispatcher,
            announcement::STONE_SONG,
            frame("s1", Some(Inventory { rev: Some(3), total: None, items: Vec::new() })),
        )
        .await;
        let stored = topology
            .snapshot()
            .into_iter()
            .find(|v| v.body.stone.id == "s1")
            .expect("peer cached");
        let inv = stored.body.inventory.services.expect("block kept");
        assert!(inv.items.is_empty(), "newer empty set defends itself");
        assert_eq!(inv.rev, Some(3));
        token.cancel();
    }

    // --- candidates (S4, ADR-0004 §3) ------------------------------------

    use garden_contract::discovery::DiscoveryResponse;

    /// A rich answer from a stone we have never met.
    fn answer(id: &str, name: &str) -> DiscoveryResponse {
        DiscoveryResponse {
            stone: Stone {
                id: id.into(),
                name: name.into(),
                moss: Moss { version: "1.0.0".into() },
                network: Network {
                    address: PeerAddress {
                        ip: IpAddr::from(Ipv4Addr::LOCALHOST),
                        port: 7285,
                        tls_port: None,
                    },
                    mac: None,
                },
            },
            lantern_endpoint: None,
            services: Some(Inventory {
                rev: Some(7),
                total: None,
                items: Vec::new(),
            }),
        }
    }

    /// Deliver an overheard answer through the claim path.
    async fn overhear(
        topology: &Arc<Topology>,
        dispatcher: &Dispatcher,
        resp: &DiscoveryResponse,
    ) {
        dispatcher
            .ingest(Ingested {
                announcement: garden_contract::wire::Announcement::new(
                    announcement::DISCOVERY_RESPONSE,
                    serde_json::to_value(resp).unwrap(),
                ),
                source: SocketAddr::from((Ipv4Addr::LOCALHOST, 50001)),
                received_at: chrono::Utc::now(),
            })
            .await;
        // on_response does not bump the version watch (candidates are not
        // membership) — settle by time.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let _ = topology; // symmetry with speak()
    }

    #[tokio::test]
    async fn overheard_answer_is_a_rumor_not_a_member() {
        let (dispatcher, handle) = Dispatcher::new(16);
        let topology = Arc::new(Topology::new());
        let token = CancellationToken::new();
        topology.claim(&dispatcher, token.clone());
        tokio::spawn(handle.run(token.clone()));

        overhear(&topology, &dispatcher, &answer("r1", "stone-rumor")).await;
        // A rumor without identity is worthless — not even a candidate.
        overhear(&topology, &dispatcher, &answer("", "stone-ghost")).await;

        assert!(topology.snapshot().is_empty(), "a rumor is never a member");
        let cands = topology.candidates();
        assert_eq!(cands.len(), 1, "only the identified rumor lands");
        assert_eq!(cands[0].response.stone.name, "stone-rumor");
        assert_eq!(
            cands[0].response.services.as_ref().and_then(|i| i.rev),
            Some(7),
            "the rich answer's inventory rides the rumor"
        );
        token.cancel();
    }

    #[tokio::test]
    async fn first_live_frame_promotes_and_silences_the_rumor() {
        let (dispatcher, handle) = Dispatcher::new(16);
        let topology = Arc::new(Topology::new());
        let token = CancellationToken::new();
        topology.claim(&dispatcher, token.clone());
        tokio::spawn(handle.run(token.clone()));

        overhear(&topology, &dispatcher, &answer("r1", "stone-rumor")).await;
        speak(
            &topology,
            &dispatcher,
            announcement::STONE_CHIRP,
            frame("r1", Some(one_item(2))),
        )
        .await;

        assert!(topology.candidates().is_empty(), "the rumor is redundant");
        let peer = topology
            .snapshot()
            .into_iter()
            .find(|v| v.body.stone.id == "r1")
            .expect("live truth cached");
        assert_eq!(
            peer.body.inventory.services.and_then(|i| i.rev),
            Some(2),
            "the peer entry speaks the stone's own truth, not the rumor's"
        );
        token.cancel();
    }

    #[tokio::test]
    async fn live_truth_ignores_gossip() {
        let (dispatcher, handle) = Dispatcher::new(16);
        let topology = Arc::new(Topology::new());
        let token = CancellationToken::new();
        topology.claim(&dispatcher, token.clone());
        tokio::spawn(handle.run(token.clone()));

        speak(
            &topology,
            &dispatcher,
            announcement::STONE_CHIRP,
            frame("r1", Some(one_item(2))),
        )
        .await;
        overhear(&topology, &dispatcher, &answer("r1", "stone-rumor")).await;

        assert!(
            topology.candidates().is_empty(),
            "chirp-borne truth outranks gossip; no rumor accumulates"
        );
        token.cancel();
    }

    #[tokio::test]
    async fn unconfirmed_rumors_age_out() {
        let (dispatcher, handle) = Dispatcher::new(16);
        let topology = Arc::new(Topology::new());
        let token = CancellationToken::new();
        topology.claim(&dispatcher, token.clone());
        tokio::spawn(handle.run(token.clone()));

        overhear(&topology, &dispatcher, &answer("r2", "stone-late")).await;
        assert_eq!(topology.candidates().len(), 1);

        // Within the TTL nothing dies.
        assert_eq!(topology.sweep_candidates(chrono::Utc::now()), 0);
        // Past it, the rumor dies quietly.
        let past = chrono::Utc::now()
            + chrono::Duration::seconds(
                garden_contract::consts::CANDIDATE_TTL_SECS as i64 + 1,
            );
        assert_eq!(topology.sweep_candidates(past), 1);
        assert!(topology.candidates().is_empty());
        token.cancel();
    }
}
