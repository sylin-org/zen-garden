//! Garden — the client-side read-model aggregate.
//!
//! [`Garden`] subscribes to [`Pulse`], projects incoming events onto an
//! internal [`GardenState`], and exposes typed property accessors for
//! consumers (adapters in Book VI, Companion in Book VII). It also
//! synthesizes a [`GardenSnapshot`] event at subscribe time so adapters
//! have a clean hydration point.
//!
//! See [COMPANION-0006] for the book ADR.
//!
//! [COMPANION-0006]: https://github.com/zen-garden/zen-garden/blob/dev/docs/decisions/COMPANION-0006-garden-aggregate.md
//! [`Pulse`]: super::Pulse

use super::core_payloads::{
    KIND_PRESENCE_SNAPSHOT, KIND_SERVICE_STARTED, KIND_SERVICE_STOPPED,
    KIND_STONE_HEALTH_CHANGED, KIND_STONE_LOAD_UPDATED, KIND_STORAGE_CONNECTED,
    KIND_STORAGE_REMOVED, PresenceSnapshotExt, ServiceStartedPayload, ServiceStoppedPayload,
    StoneHealthChangedExt, StoneLoadUpdatedExt, StorageConnectedPayload, StorageRemovedPayload,
};
use super::event::{Event, EventPayload};
use super::pulse::Pulse;
use garden_common::domain::{Health, Load, Pond, SeedBank};
use garden_common::presence::OfferingState;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// GardenState — the read-model
// ---------------------------------------------------------------------------

/// Current state of the garden as observed by this companion.
///
/// All fields have sensible `Default` values so a freshly-constructed
/// [`Garden`] can return a consistent state before any presence events
/// have arrived. Adapters should treat `ready == false` as "waiting for
/// initial snapshot" and render accordingly (e.g. show an "connecting..."
/// placeholder).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GardenState {
    /// Stone name. `None` until the first presence snapshot arrives.
    pub stone_name: Option<String>,

    /// Stone vitality. Defaults to [`Health::Dormant`].
    pub health: Health,

    /// Resource load. Defaults to [`Load::ZERO`].
    pub load: Load,

    /// Managed offerings currently running on the stone. Wire-typed for
    /// Book V; a typed `Offering` promotion may land in a later book.
    pub offerings: Vec<OfferingState>,

    /// Attached seed-bank, if any.
    pub seed_bank: Option<SeedBank>,

    /// Pond membership. Defaults to [`Pond::Solo`].
    pub pond: Pond,

    /// `true` once the first [`core.presence.snapshot`] event has been
    /// projected. Adapters that need initial state should gate rendering
    /// on this.
    pub ready: bool,
}

// ---------------------------------------------------------------------------
// GardenSnapshot event payload
// ---------------------------------------------------------------------------

/// Synthetic event payload carrying the full [`GardenState`] at a point
/// in time. Emitted by [`Garden::subscribe`] as the initial event in
/// every subscription — adapters render from this before entering their
/// event loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GardenSnapshot {
    pub state: GardenState,
}

impl GardenSnapshot {
    pub const KIND: &'static str = "core.garden.snapshot";
}

impl EventPayload for GardenSnapshot {
    const KIND: &'static str = "core.garden.snapshot";
    fn as_any(&self) -> &dyn Any {
        self
    }
}

// ---------------------------------------------------------------------------
// GardenSubscription — subscribe() return
// ---------------------------------------------------------------------------

/// Result of [`Garden::subscribe`]. The `snapshot` field is a freshly-
/// synthesized [`GardenSnapshot`] event reflecting current state; treat
/// it as if it were the first event received. The `receiver` then
/// streams live events from [`Pulse`] for as long as the subscription
/// is held.
pub struct GardenSubscription {
    /// Snapshot event — render this first.
    pub snapshot: Event,

    /// Live event stream — `recv()` in a loop.
    pub receiver: broadcast::Receiver<Event>,
}

// ---------------------------------------------------------------------------
// Garden aggregate
// ---------------------------------------------------------------------------

/// Client-side read-model of the garden's presence state.
///
/// Constructed as `Arc<Garden>` — the projection task holds a clone
/// internally. Call [`Garden::spawn_projection`] once after construction
/// to start the task.
pub struct Garden {
    state: Arc<RwLock<GardenState>>,
    pulse: Arc<Pulse>,
}

impl Garden {
    /// Construct a new Garden observing the given Pulse. The returned
    /// Arc can be cheaply cloned; the projection task holds its own
    /// clone once spawned.
    pub fn new(pulse: Arc<Pulse>) -> Arc<Self> {
        Arc::new(Self {
            state: Arc::new(RwLock::new(GardenState::default())),
            pulse,
        })
    }

    /// Register the `core.garden` namespace on the underlying Pulse so
    /// GardenSnapshot events can be published if needed. Idempotent.
    pub fn register_namespace(&self) {
        self.pulse.register_namespace("core");
    }

    /// Spawn the projection task. Reads events from Pulse and applies
    /// them to state until `shutdown` is cancelled. Returns the task
    /// handle.
    pub fn spawn_projection(
        self: &Arc<Self>,
        shutdown: CancellationToken,
    ) -> tokio::task::JoinHandle<()> {
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let mut rx = this.pulse.subscribe();
            loop {
                tokio::select! {
                    recv = rx.recv() => match recv {
                        Ok(event) => this.apply(&event),
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(skipped = n, "Garden projection lagged");
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    },
                    _ = shutdown.cancelled() => break,
                }
            }
        })
    }

    /// Apply a single event to state. Public for test use and for
    /// adapters that want to replay recorded events; normally only the
    /// projection task calls this.
    pub fn apply(&self, event: &Event) {
        let mut state = self
            .state
            .write()
            .expect("Garden state lock poisoned");
        project(&mut state, event);
    }

    // --- Typed property accessors ---

    pub fn stone_name(&self) -> Option<String> {
        self.state
            .read()
            .expect("Garden state lock poisoned")
            .stone_name
            .clone()
    }

    pub fn health(&self) -> Health {
        self.state
            .read()
            .expect("Garden state lock poisoned")
            .health
    }

    pub fn load(&self) -> Load {
        self.state
            .read()
            .expect("Garden state lock poisoned")
            .load
    }

    pub fn offerings(&self) -> Vec<OfferingState> {
        self.state
            .read()
            .expect("Garden state lock poisoned")
            .offerings
            .clone()
    }

    pub fn seed_bank(&self) -> Option<SeedBank> {
        self.state
            .read()
            .expect("Garden state lock poisoned")
            .seed_bank
            .clone()
    }

    pub fn pond(&self) -> Pond {
        self.state
            .read()
            .expect("Garden state lock poisoned")
            .pond
    }

    pub fn is_ready(&self) -> bool {
        self.state
            .read()
            .expect("Garden state lock poisoned")
            .ready
    }

    /// Full state clone. Useful for adapter hydration paths that want
    /// a single consistent snapshot rather than many per-property calls.
    pub fn snapshot(&self) -> GardenState {
        self.state
            .read()
            .expect("Garden state lock poisoned")
            .clone()
    }

    /// Subscribe to the event stream. Returns a [`GardenSubscription`]
    /// containing a snapshot event (for immediate hydration) plus a
    /// receiver for live events.
    pub fn subscribe(&self) -> GardenSubscription {
        let state = self.snapshot();
        let snapshot_payload = GardenSnapshot { state };
        let snapshot_event = Event::new(snapshot_payload);
        let receiver = self.pulse.subscribe();
        GardenSubscription {
            snapshot: snapshot_event,
            receiver,
        }
    }
}

// ---------------------------------------------------------------------------
// Projection — the pure state-update function
// ---------------------------------------------------------------------------

/// Apply an event to GardenState. Pure function — no I/O, no locking
/// (caller holds the lock). Unknown event kinds are silently ignored.
pub(crate) fn project(state: &mut GardenState, event: &Event) {
    match event.kind {
        KIND_PRESENCE_SNAPSHOT => {
            if let Some(payload) = event.payload::<garden_common::presence::PresenceSnapshot>() {
                apply_snapshot(state, payload);
            }
        }
        KIND_STONE_HEALTH_CHANGED => {
            if let Some(p) =
                event.payload::<garden_common::presence::StoneHealthChangedPayload>()
            {
                state.health = p.health_domain();
            }
        }
        KIND_STONE_LOAD_UPDATED => {
            if let Some(p) = event.payload::<garden_common::presence::StoneLoadUpdatedPayload>()
            {
                state.load = p.load_domain();
            }
        }
        KIND_SERVICE_STARTED => {
            if let Some(p) = event.payload::<ServiceStartedPayload>() {
                add_offering_if_absent(state, &p.service);
            }
        }
        KIND_SERVICE_STOPPED => {
            if let Some(p) = event.payload::<ServiceStoppedPayload>() {
                state.offerings.retain(|o| o.name != p.service);
            }
        }
        KIND_STORAGE_CONNECTED => {
            if let Some(p) = event.payload::<StorageConnectedPayload>() {
                state.seed_bank = Some(SeedBank {
                    name: p.name.clone(),
                    used_gb: 0,
                    total_gb: p.capacity_gb,
                });
            }
        }
        KIND_STORAGE_REMOVED => {
            if let Some(p) = event.payload::<StorageRemovedPayload>()
                && state
                    .seed_bank
                    .as_ref()
                    .is_some_and(|b| b.name == p.name)
            {
                state.seed_bank = None;
            }
        }
        _ => {
            // Unknown event — no projection effect. Coalescing and
            // discrete events Garden doesn't track fall through here.
        }
    }
}

fn apply_snapshot(state: &mut GardenState, snap: &garden_common::presence::PresenceSnapshot) {
    state.stone_name = Some(snap.stone.name.clone());
    state.health = snap.stone_health();
    state.load = snap.stone_load();
    state.offerings = snap.offerings.clone();
    state.seed_bank = snap.seed_bank();
    state.pond = snap.pond();
    state.ready = true;
}

fn add_offering_if_absent(state: &mut GardenState, name: &str) {
    if !state.offerings.iter().any(|o| o.name == name) {
        state.offerings.push(OfferingState {
            name: name.to_string(),
            status: "running".to_string(),
            health: "healthy".to_string(),
        });
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::garden::{IngestResult, PulseConfig};
    use garden_common::presence::{
        PresenceSnapshot, StoneHealthChangedPayload, StoneLoadUpdatedPayload, StoneState,
        StoragePresence,
    };

    fn core_pulse() -> Arc<Pulse> {
        let pulse = Arc::new(Pulse::new(PulseConfig {
            dedup_capacity: 16,
            broadcast_capacity: 64,
        }));
        pulse.register_namespace("core");
        pulse
    }

    fn fresh_snapshot_payload() -> PresenceSnapshot {
        PresenceSnapshot {
            stone: StoneState {
                name: "test-stone".into(),
                health: "thriving".into(),
                cpu_percent: 25.0,
                memory_percent: 40.0,
                disk_percent: 50.0,
                uptime_seconds: 3600,
                pond_active: true,
                io_percent: 10.0,
                gpu_percent: 0.0,
                net_rx_bytes_per_sec: 1024,
                net_tx_bytes_per_sec: 2048,
                has_gpu: false,
                gpu_active: false,
                is_lantern: false,
                has_cricket: false,
                hour: 12.5,
                seed_bank: Some(StoragePresence {
                    name: "primary".into(),
                    used_gb: 200,
                    total_gb: 1000,
                }),
            },
            offerings: vec![
                OfferingState {
                    name: "mongodb".into(),
                    status: "running".into(),
                    health: "healthy".into(),
                },
                OfferingState {
                    name: "redis".into(),
                    status: "running".into(),
                    health: "healthy".into(),
                },
            ],
            timestamp: chrono::Utc::now(),
        }
    }

    // --- Default / initial state ---

    #[test]
    fn default_garden_state_is_dormant_and_not_ready() {
        let garden = Garden::new(core_pulse());
        assert!(garden.stone_name().is_none());
        assert_eq!(garden.health(), Health::Dormant);
        assert_eq!(garden.load(), Load::ZERO);
        assert!(garden.offerings().is_empty());
        assert!(garden.seed_bank().is_none());
        assert_eq!(garden.pond(), Pond::Solo);
        assert!(!garden.is_ready());
    }

    // --- Apply projection (direct, without task) ---

    #[test]
    fn apply_snapshot_replaces_state_and_marks_ready() {
        let garden = Garden::new(core_pulse());
        let payload = fresh_snapshot_payload();
        let event = Event::new(payload);

        garden.apply(&event);

        assert!(garden.is_ready());
        assert_eq!(garden.stone_name().as_deref(), Some("test-stone"));
        assert_eq!(garden.health(), Health::Thriving);
        assert_eq!(garden.load().cpu.value(), 25.0);
        assert_eq!(garden.offerings().len(), 2);
        assert_eq!(garden.seed_bank().unwrap().name, "primary");
        assert_eq!(garden.pond(), Pond::Member);
    }

    #[test]
    fn apply_health_changed_updates_health_field() {
        let garden = Garden::new(core_pulse());
        let event = Event::new(StoneHealthChangedPayload {
            health: "wilting".into(),
            cpu_percent: 0.0,
            memory_percent: 0.0,
        });
        garden.apply(&event);
        assert_eq!(garden.health(), Health::Wilting);
    }

    #[test]
    fn apply_load_updated_updates_load_field() {
        let garden = Garden::new(core_pulse());
        let event = Event::new(StoneLoadUpdatedPayload {
            cpu_percent: 80.0,
            memory_percent: 70.0,
            disk_percent: 50.0,
            io_percent: 20.0,
            gpu_percent: 60.0,
            gpu_active: true,
            net_rx_bytes_per_sec: 500,
            net_tx_bytes_per_sec: 500,
        });
        garden.apply(&event);
        let load = garden.load();
        assert_eq!(load.cpu.value(), 80.0);
        assert_eq!(load.memory.value(), 70.0);
        assert!(load.gpu_active);
        assert_eq!(load.net_total_bytes_per_sec(), 1000);
    }

    #[test]
    fn apply_service_started_adds_to_offerings_once() {
        let garden = Garden::new(core_pulse());

        let event = Event::new(ServiceStartedPayload {
            service: "mongodb".into(),
        });
        garden.apply(&event);
        assert_eq!(garden.offerings().len(), 1);

        // Re-applying the same service does not duplicate.
        let event2 = Event::new(ServiceStartedPayload {
            service: "mongodb".into(),
        });
        garden.apply(&event2);
        assert_eq!(garden.offerings().len(), 1);
    }

    #[test]
    fn apply_service_stopped_removes_from_offerings() {
        let garden = Garden::new(core_pulse());
        // Seed with a snapshot that includes an offering.
        garden.apply(&Event::new(fresh_snapshot_payload()));
        assert_eq!(garden.offerings().len(), 2);

        garden.apply(&Event::new(ServiceStoppedPayload {
            service: "mongodb".into(),
        }));
        assert_eq!(garden.offerings().len(), 1);
        assert_eq!(garden.offerings()[0].name, "redis");
    }

    #[test]
    fn apply_storage_connected_and_removed_toggles_seed_bank() {
        let garden = Garden::new(core_pulse());

        garden.apply(&Event::new(StorageConnectedPayload {
            name: "backup".into(),
            capacity_gb: 500,
            device: None,
            mount_path: None,
        }));
        let bank = garden.seed_bank().expect("seed bank should be present");
        assert_eq!(bank.name, "backup");
        assert_eq!(bank.total_gb, 500);

        garden.apply(&Event::new(StorageRemovedPayload {
            name: "backup".into(),
        }));
        assert!(garden.seed_bank().is_none());
    }

    #[test]
    fn apply_storage_removed_keeps_state_when_names_differ() {
        let garden = Garden::new(core_pulse());
        garden.apply(&Event::new(StorageConnectedPayload {
            name: "primary".into(),
            capacity_gb: 1000,
            device: None,
            mount_path: None,
        }));

        // Remove with a different name — primary should stay.
        garden.apply(&Event::new(StorageRemovedPayload {
            name: "other".into(),
        }));
        assert_eq!(garden.seed_bank().unwrap().name, "primary");
    }

    #[test]
    fn apply_stone_tended_does_not_mutate_state() {
        let garden = Garden::new(core_pulse());
        garden.apply(&Event::new(fresh_snapshot_payload()));
        let before = garden.snapshot();

        let tended = Event::new(crate::garden::StoneTendedPayload {
            by: "rake".into(),
            from: "local".into(),
            message: Some("hi".into()),
        });
        garden.apply(&tended);

        let after = garden.snapshot();
        // The tend event is a transient notification — no state change.
        assert_eq!(before.stone_name, after.stone_name);
        assert_eq!(before.health, after.health);
        assert_eq!(before.offerings.len(), after.offerings.len());
    }

    // --- Subscribe / snapshot event ---

    #[test]
    fn subscribe_returns_snapshot_plus_live_receiver() {
        let garden = Garden::new(core_pulse());
        garden.apply(&Event::new(fresh_snapshot_payload()));

        let sub = garden.subscribe();
        let snapshot = sub.snapshot.payload::<GardenSnapshot>().expect("snapshot");
        assert!(snapshot.state.ready);
        assert_eq!(snapshot.state.stone_name.as_deref(), Some("test-stone"));
        assert_eq!(snapshot.state.health, Health::Thriving);
    }

    #[test]
    fn snapshot_event_has_correct_kind() {
        let garden = Garden::new(core_pulse());
        let sub = garden.subscribe();
        assert_eq!(sub.snapshot.kind, GardenSnapshot::KIND);
        assert_eq!(sub.snapshot.kind, "core.garden.snapshot");
    }

    // --- Projection task ---

    #[tokio::test]
    async fn projection_task_consumes_events_from_pulse() {
        let pulse = core_pulse();
        let garden = Garden::new(pulse.clone());
        let token = CancellationToken::new();
        let handle = garden.spawn_projection(token.clone());

        // Give the task a tick to subscribe.
        tokio::task::yield_now().await;

        let outcome = pulse.ingest(Event::new(fresh_snapshot_payload()));
        assert!(matches!(outcome, IngestResult::Accepted { .. }));

        // Wait for projection to apply. Up to 500ms with short polls.
        for _ in 0..50 {
            if garden.is_ready() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        assert!(garden.is_ready());
        assert_eq!(garden.stone_name().as_deref(), Some("test-stone"));
        assert_eq!(garden.health(), Health::Thriving);

        token.cancel();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(1), handle).await;
    }

    #[tokio::test]
    async fn projection_task_exits_on_shutdown() {
        let garden = Garden::new(core_pulse());
        let token = CancellationToken::new();
        let handle = garden.spawn_projection(token.clone());

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        token.cancel();

        tokio::time::timeout(std::time::Duration::from_secs(1), handle)
            .await
            .expect("projection task did not exit within 1s after shutdown")
            .expect("projection task panicked");
    }

    #[test]
    fn garden_snapshot_payload_is_not_coalescing() {
        // Synthetic events must fire every time (they carry per-subscriber state).
        assert!(!GardenSnapshot::COALESCING);
    }
}
