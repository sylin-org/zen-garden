//! Adapter supervisor — manages adapter lifecycle.
//!
//! The supervisor's responsibilities:
//!
//! 1. **Registration** — store factories passed via [`Adapters::register`].
//! 2. **Discovery** — periodically call each factory's `discover` and learn
//!    what adapter instances currently exist.
//! 3. **Spawn** — for each newly-discovered instance (by `info.id`),
//!    construct a per-adapter mpsc filter, spawn the adapter's `run`
//!    inside a `tracing::info_span!("adapter", kind, id)`.
//! 4. **Reap** — when a previously-discovered instance stops appearing,
//!    wait `grace_window` in case the device bounces; if it doesn't come
//!    back, cancel the adapter's shutdown token and await its task.
//! 5. **Subscription filtering** — the per-adapter filter task subscribes
//!    to [`Pulse`] and forwards only events whose kind is in the
//!    adapter's [`AdapterProfile::subscriptions`].
//!
//! The supervisor is not responsible for installing system dependencies
//! at V1 — [`AdapterFactory::required_dependencies`] is defined but not
//! yet wired (see COMPANION-0007 out-of-scope list). It is also not
//! responsible for enforcing [`DeliveryPolicy::LatestEvery`] or
//! [`DeliveryPolicy::Debounced`]; those declare adapter intent but ship
//! as `All` behaviour in Book VI.

use super::adapter::{Adapter, AdapterInfo};
use super::factory::AdapterFactory;
use super::status::AdapterStatus;
use crate::garden::{Event, Pulse};
use crate::moss_client::MossLocalClient;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

/// Default discovery-tick interval.
pub const DEFAULT_DISCOVERY_INTERVAL: Duration = Duration::from_secs(5);

/// Default grace window before reaping an adapter whose device disappeared.
pub const DEFAULT_GRACE_WINDOW: Duration = Duration::from_secs(2);

/// Per-adapter mpsc channel depth. Small enough to apply backpressure on
/// slow adapters without starving the filter task.
const ADAPTER_MPSC_DEPTH: usize = 64;

// ---------------------------------------------------------------------------
// ActiveAdapter — supervisor's bookkeeping per running adapter
// ---------------------------------------------------------------------------

struct ActiveAdapter {
    info: AdapterInfo,
    shutdown: CancellationToken,
    status: Arc<Mutex<AdapterStatus>>,
    last_seen: Instant,
    /// `true` when the adapter was spawned by an external lifecycle
    /// manager (the device bus). The factory-discovery grace-window
    /// reap logic skips these; they are reaped only via explicit
    /// [`Adapters::reap_id`] calls.
    external: bool,
    /// Task running adapter.run(). We await this on reap to ensure clean
    /// shutdown (with timeout).
    run_handle: Option<JoinHandle<()>>,
    /// Task running the filter (subscription → mpsc). Dropped alongside
    /// run_handle on reap — dropping the mpsc sender signals the filter
    /// to exit.
    filter_handle: Option<JoinHandle<()>>,
}

// ---------------------------------------------------------------------------
// Supervisor aggregate
// ---------------------------------------------------------------------------

/// Supervisor aggregate managing adapter lifecycles.
pub struct Adapters {
    factories: RwLock<Vec<Box<dyn AdapterFactory>>>,
    active: Arc<RwLock<HashMap<String, ActiveAdapter>>>,
    moss: Arc<MossLocalClient>,
    pulse: Arc<Pulse>,
    discovery_interval: Duration,
    grace_window: Duration,

    /// Sender side of the adapter-exit event channel. The wrapper task
    /// spawned for each adapter publishes here when the run-future
    /// completes.
    exit_tx: tokio::sync::mpsc::UnboundedSender<crate::adapters::exit::AdapterExited>,
    /// Receiver side, taken once via [`Adapters::subscribe_exits`].
    /// Single-consumer model — the device bus is the canonical owner.
    exit_rx: Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<crate::adapters::exit::AdapterExited>>>,
}

impl Adapters {
    /// Construct an empty supervisor bound to a [`MossLocalClient`] +
    /// [`Pulse`]. The moss client is the canonical read-path adapters
    /// hydrate from at spawn time (COMPANION-0014).
    pub fn new(moss: Arc<MossLocalClient>, pulse: Arc<Pulse>) -> Self {
        let (exit_tx, exit_rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            factories: RwLock::new(Vec::new()),
            active: Arc::new(RwLock::new(HashMap::new())),
            moss,
            pulse,
            discovery_interval: DEFAULT_DISCOVERY_INTERVAL,
            grace_window: DEFAULT_GRACE_WINDOW,
            exit_tx,
            exit_rx: Mutex::new(Some(exit_rx)),
        }
    }

    /// Take the exit-event receiver. Returns `None` after the first
    /// call — the channel is single-consumer because adapter exits
    /// drive port-ownership reclamation, which has exactly one
    /// authoritative owner (the device bus).
    pub fn subscribe_exits(
        &self,
    ) -> Option<tokio::sync::mpsc::UnboundedReceiver<crate::adapters::exit::AdapterExited>> {
        self.exit_rx.lock().expect("Adapters exit_rx lock poisoned").take()
    }

    /// Override the default discovery interval.
    pub fn with_discovery_interval(mut self, d: Duration) -> Self {
        self.discovery_interval = d;
        self
    }

    /// Override the default grace window.
    pub fn with_grace_window(mut self, d: Duration) -> Self {
        self.grace_window = d;
        self
    }

    /// Register a factory.
    pub fn register<F: AdapterFactory>(&self, factory: F) {
        self.factories
            .write()
            .expect("Adapters factories lock poisoned")
            .push(Box::new(factory));
    }

    /// Number of registered factories.
    pub fn factory_count(&self) -> usize {
        self.factories
            .read()
            .expect("Adapters factories lock poisoned")
            .len()
    }

    /// Kinds of all registered factories.
    pub fn factory_kinds(&self) -> Vec<&'static str> {
        self.factories
            .read()
            .expect("Adapters factories lock poisoned")
            .iter()
            .map(|f| f.kind())
            .collect()
    }

    /// Snapshot the status of every currently-active adapter.
    pub fn status(&self) -> Vec<(AdapterInfo, AdapterStatus)> {
        self.active
            .read()
            .expect("Adapters active lock poisoned")
            .values()
            .map(|a| {
                let status = a
                    .status
                    .lock()
                    .expect("AdapterStatus lock poisoned")
                    .clone();
                (a.info.clone(), status)
            })
            .collect()
    }

    /// Number of currently-active adapter instances.
    pub fn active_count(&self) -> usize {
        self.active
            .read()
            .expect("Adapters active lock poisoned")
            .len()
    }

    /// Run the supervisor loop until `shutdown` is cancelled. On exit,
    /// all active adapters are reaped cleanly.
    pub async fn run(&self, shutdown: CancellationToken) {
        tracing::info!(
            factories = self.factory_count(),
            discovery_interval_ms = self.discovery_interval.as_millis() as u64,
            grace_window_ms = self.grace_window.as_millis() as u64,
            "Adapters supervisor starting"
        );

        // First discovery tick immediately so adapters don't wait `discovery_interval`.
        self.tick().await;

        let mut interval = tokio::time::interval(self.discovery_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // First tick fires immediately; consume it since we already ticked.
        interval.tick().await;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.tick().await;
                }
                _ = shutdown.cancelled() => break,
            }
        }

        self.reap_all().await;
        tracing::info!("Adapters supervisor stopped");
    }

    // --- Supervisor internals ---

    /// One discovery tick: collect current candidates from every factory,
    /// spawn new ones, reap those absent past the grace window.
    async fn tick(&self) {
        let now = Instant::now();

        // Collect all candidates first, releasing the factories lock
        // before any await (RwLockReadGuard is not Send).
        let all_candidates: Vec<Box<dyn Adapter>> = {
            let factories = self
                .factories
                .read()
                .expect("Adapters factories lock poisoned");
            let mut out: Vec<Box<dyn Adapter>> = Vec::new();
            for factory in factories.iter() {
                out.extend(factory.discover());
            }
            out
        };

        let mut present_ids: HashSet<String> = HashSet::new();
        for candidate in all_candidates {
            let info = candidate.info();
            present_ids.insert(info.id.clone());

            let needs_spawn = {
                let active = self
                    .active
                    .read()
                    .expect("Adapters active lock poisoned");
                !active.contains_key(&info.id)
            };

            if needs_spawn {
                self.spawn(candidate, now);
            } else {
                // Already running — refresh last_seen so grace-window
                // reaping doesn't kick in while the device is present.
                if let Some(active) = self
                    .active
                    .write()
                    .expect("Adapters active lock poisoned")
                    .get_mut(&info.id)
                {
                    active.last_seen = now;
                }
            }
        }

        // Find factory-spawned adapters whose id is not in present_ids
        // for longer than grace_window. Externally-managed adapters
        // (bus-spawned) are not reaped here — they are reaped via the
        // explicit `reap_id` path triggered by a `Detached` bus event.
        let to_reap: Vec<String> = {
            let active = self
                .active
                .read()
                .expect("Adapters active lock poisoned");
            active
                .iter()
                .filter(|(id, a)| {
                    !a.external
                        && !present_ids.contains(id.as_str())
                        && now.duration_since(a.last_seen) >= self.grace_window
                })
                .map(|(id, _)| id.clone())
                .collect()
        };

        for id in to_reap {
            self.reap(&id).await;
        }
    }

    /// Spawn an adapter supplied by an external lifecycle manager
    /// (the device bus). The returned id is the key the caller uses
    /// with [`Adapters::reap_id`] when the device detaches.
    ///
    /// External adapters are *not* reaped by the factory-discovery
    /// grace-window path — they are only reaped via an explicit
    /// `reap_id` call triggered by a bus `Detached` event.
    pub fn spawn_external(&self, adapter: Box<dyn Adapter>) -> String {
        let id = adapter.info().id.clone();
        self.spawn_inner(adapter, Instant::now(), true);
        id
    }

    /// Reap an adapter by id (factory-spawned or external). Idempotent:
    /// a second call for the same id is a no-op.
    pub async fn reap_id(&self, id: &str) {
        self.reap(id).await;
    }

    /// Spawn a new adapter. Creates the filter task + run task under a
    /// tracing span and stores the bookkeeping entry.
    fn spawn(&self, adapter: Box<dyn Adapter>, now: Instant) {
        self.spawn_inner(adapter, now, false);
    }

    fn spawn_inner(&self, adapter: Box<dyn Adapter>, now: Instant, external: bool) {
        let info = adapter.info();
        let profile = adapter.profile();

        let (tx, rx) = mpsc::channel::<Event>(ADAPTER_MPSC_DEPTH);
        let shutdown = CancellationToken::new();

        let status = Arc::new(Mutex::new(AdapterStatus::Spawning));

        // Filter task: subscribe to pulse, forward matching kinds into mpsc.
        let pulse_rx = self.pulse.subscribe();
        let subscriptions: HashSet<&'static str> =
            profile.subscriptions.iter().copied().collect();
        let filter_status = status.clone();
        let filter_shutdown = shutdown.clone();
        let filter_span = tracing::info_span!(
            "adapter_filter",
            kind = info.kind,
            id = %info.id,
        );

        let filter_handle = tokio::spawn(
            run_filter(
                pulse_rx,
                tx,
                subscriptions,
                filter_status,
                filter_shutdown,
            )
            .instrument(filter_span),
        );

        // Run task: invoke adapter.run inside a tracing span.
        let run_span = tracing::info_span!(
            "adapter",
            kind = info.kind,
            id = %info.id,
        );
        let moss = self.moss.clone();
        let pulse = self.pulse.clone();
        let run_shutdown = shutdown.clone();
        let inner = tokio::spawn(
            adapter
                .run(rx, moss, pulse, run_shutdown)
                .instrument(run_span),
        );

        // Wrapper: observe completion, derive exit reason, publish.
        // The supervisor stores this outer handle so reap_id awaits
        // both the adapter's run and the exit-event delivery.
        let exit_tx = self.exit_tx.clone();
        let exit_id = info.id.clone();
        let exit_shutdown = shutdown.clone();
        let run_handle = tokio::spawn(async move {
            let join = inner.await;
            let reason = if join.is_err() {
                crate::adapters::exit::AdapterExitReason::Panicked
            } else if exit_shutdown.is_cancelled() {
                crate::adapters::exit::AdapterExitReason::Reaped
            } else {
                crate::adapters::exit::AdapterExitReason::SelfExit
            };
            let _ = exit_tx.send(crate::adapters::exit::AdapterExited {
                id: exit_id,
                reason,
            });
        });

        let id = info.id.clone();
        let entry = ActiveAdapter {
            info,
            shutdown,
            status,
            last_seen: now,
            external,
            run_handle: Some(run_handle),
            filter_handle: Some(filter_handle),
        };

        self.active
            .write()
            .expect("Adapters active lock poisoned")
            .insert(id.clone(), entry);

        tracing::info!(id = %id, "adapter spawned");
    }

    /// Cancel and await a single adapter's tasks. Runs the reap procedure
    /// with a bounded timeout so a stuck adapter can't hang the supervisor.
    async fn reap(&self, id: &str) {
        let maybe_entry = self
            .active
            .write()
            .expect("Adapters active lock poisoned")
            .remove(id);

        let Some(mut entry) = maybe_entry else {
            return;
        };

        tracing::info!(id = %entry.info.id, "adapter reap requested");
        entry.shutdown.cancel();

        if let Some(handle) = entry.run_handle.take() {
            let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
        }
        if let Some(handle) = entry.filter_handle.take() {
            handle.abort();
            let _ = handle.await;
        }

        *entry.status.lock().expect("AdapterStatus lock poisoned") = AdapterStatus::Stopped;
        tracing::info!(id = %entry.info.id, "adapter reaped");
    }

    /// Reap every active adapter (called on supervisor shutdown).
    async fn reap_all(&self) {
        let ids: Vec<String> = self
            .active
            .read()
            .expect("Adapters active lock poisoned")
            .keys()
            .cloned()
            .collect();
        for id in ids {
            self.reap(&id).await;
        }
    }
}

// ---------------------------------------------------------------------------
// Filter task
// ---------------------------------------------------------------------------

/// Subscription filter. Reads events from Pulse's broadcast receiver,
/// forwards matching kinds into the adapter's mpsc. Exits when the mpsc
/// closes (adapter ended) or shutdown fires.
async fn run_filter(
    mut pulse_rx: tokio::sync::broadcast::Receiver<Event>,
    tx: mpsc::Sender<Event>,
    subscriptions: HashSet<&'static str>,
    status: Arc<Mutex<AdapterStatus>>,
    shutdown: CancellationToken,
) {
    loop {
        tokio::select! {
            recv = pulse_rx.recv() => match recv {
                Ok(event) => {
                    if !subscriptions.contains(event.kind) {
                        continue;
                    }
                    // Update status to Running on first forward.
                    {
                        let mut s = status.lock().expect("status lock poisoned");
                        match &mut *s {
                            AdapterStatus::Running { events_handled, last_event_at } => {
                                *events_handled += 1;
                                *last_event_at = Instant::now();
                            }
                            _ => {
                                *s = AdapterStatus::Running {
                                    events_handled: 1,
                                    last_event_at: Instant::now(),
                                };
                            }
                        }
                    }
                    if tx.send(event).await.is_err() {
                        // Adapter dropped its receiver — done.
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "adapter filter lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            },
            _ = shutdown.cancelled() => break,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{Adapter, AdapterInfo, AdapterProfile, DeliveryPolicy};
    use crate::garden::{Event, EventPayload, PulseConfig};
    use std::any::Any;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // --- Fixtures ---

    #[derive(Debug)]
    struct TestEventA;
    impl EventPayload for TestEventA {
        const KIND: &'static str = "core.test.a";
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    #[derive(Debug)]
    struct TestEventB;
    impl EventPayload for TestEventB {
        const KIND: &'static str = "core.test.b";
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    fn core_pulse() -> Arc<Pulse> {
        let pulse = Arc::new(Pulse::new(PulseConfig {
            dedup_capacity: 64,
            broadcast_capacity: 128,
        }));
        pulse.register_namespace("core");
        pulse
    }

    fn fixture() -> (Adapters, Arc<Pulse>) {
        let pulse = core_pulse();
        // Tests don't actually hit moss; a placeholder URL is fine —
        // adapters constructed in tests use `_moss` (no read calls).
        let moss = Arc::new(MossLocalClient::new("http://127.0.0.1:0"));
        let supervisor = Adapters::new(moss, pulse.clone())
            .with_discovery_interval(Duration::from_millis(50))
            .with_grace_window(Duration::from_millis(100));
        (supervisor, pulse)
    }

    /// Adapter that records every event it received into a shared vec.
    struct RecordingAdapter {
        kind: &'static str,
        id: String,
        subscriptions: &'static [&'static str],
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl Adapter for RecordingAdapter {
        fn info(&self) -> AdapterInfo {
            AdapterInfo {
                kind: self.kind,
                id: self.id.clone(),
                device: None,
            }
        }
        fn profile(&self) -> AdapterProfile {
            AdapterProfile {
                subscriptions: self.subscriptions,
                delivery: DeliveryPolicy::All,
                persisted_state: false,
            }
        }
        fn run(
            self: Box<Self>,
            mut events: mpsc::Receiver<Event>,
            _moss: Arc<MossLocalClient>,
            _pulse: Arc<Pulse>,
            shutdown: CancellationToken,
        ) -> super::super::adapter::BoxFuture<'static, ()> {
            let seen = self.seen.clone();
            Box::pin(async move {
                loop {
                    tokio::select! {
                        maybe = events.recv() => match maybe {
                            Some(e) => {
                                seen.lock().unwrap().push(e.kind.to_string());
                            }
                            None => break,
                        },
                        _ = shutdown.cancelled() => break,
                    }
                }
            })
        }
    }

    /// Factory with a toggleable "present" flag so tests can control
    /// discovery visibility.
    struct ToggleableFactory {
        kind: &'static str,
        id: String,
        subscriptions: &'static [&'static str],
        seen: Arc<Mutex<Vec<String>>>,
        present: Arc<AtomicUsize>, // 1=visible, 0=hidden
    }

    impl AdapterFactory for ToggleableFactory {
        fn kind(&self) -> &'static str {
            self.kind
        }
        fn discover(&self) -> Vec<Box<dyn Adapter>> {
            if self.present.load(Ordering::Relaxed) == 1 {
                vec![Box::new(RecordingAdapter {
                    kind: self.kind,
                    id: self.id.clone(),
                    subscriptions: self.subscriptions,
                    seen: self.seen.clone(),
                })]
            } else {
                vec![]
            }
        }
    }

    // --- Tests ---

    #[tokio::test]
    async fn supervisor_spawns_adapter_on_first_tick() {
        let (supervisor, _pulse) = fixture();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let present = Arc::new(AtomicUsize::new(1));

        supervisor.register(ToggleableFactory {
            kind: "test.record",
            id: "r1".into(),
            subscriptions: &["core.test.a"],
            seen: seen.clone(),
            present: present.clone(),
        });

        let shutdown = CancellationToken::new();
        let sup_handle = {
            let shutdown = shutdown.clone();
            let supervisor = Arc::new(supervisor);
            let s = supervisor.clone();
            tokio::spawn(async move {
                s.run(shutdown).await;
            })
            .boxed_into(Some(supervisor))
        };

        // Give supervisor a moment for first tick.
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(sup_handle.supervisor.active_count(), 1);

        let status = sup_handle.supervisor.status();
        assert_eq!(status.len(), 1);
        assert_eq!(status[0].0.kind, "test.record");
        assert_eq!(status[0].0.id, "r1");

        shutdown.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(2), sup_handle.join).await;
    }

    #[tokio::test]
    async fn supervisor_reaps_adapter_after_grace_window_elapses() {
        let (supervisor, _pulse) = fixture();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let present = Arc::new(AtomicUsize::new(1));

        supervisor.register(ToggleableFactory {
            kind: "test.record",
            id: "r1".into(),
            subscriptions: &[],
            seen,
            present: present.clone(),
        });

        let shutdown = CancellationToken::new();
        let supervisor = Arc::new(supervisor);
        let s = supervisor.clone();
        let sd = shutdown.clone();
        let sup_handle = tokio::spawn(async move { s.run(sd).await });

        // Wait for spawn.
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(supervisor.active_count(), 1);

        // Hide device.
        present.store(0, Ordering::Relaxed);

        // Wait for one tick + grace window.
        tokio::time::sleep(Duration::from_millis(250)).await;
        assert_eq!(supervisor.active_count(), 0);

        shutdown.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(2), sup_handle).await;
    }

    #[tokio::test]
    async fn supervisor_keeps_adapter_when_device_reappears_within_grace() {
        let (supervisor, _pulse) = fixture();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let present = Arc::new(AtomicUsize::new(1));

        supervisor.register(ToggleableFactory {
            kind: "test.record",
            id: "r1".into(),
            subscriptions: &[],
            seen,
            present: present.clone(),
        });

        let shutdown = CancellationToken::new();
        let supervisor = Arc::new(supervisor);
        let s = supervisor.clone();
        let sd = shutdown.clone();
        let sup_handle = tokio::spawn(async move { s.run(sd).await });

        tokio::time::sleep(Duration::from_millis(100)).await;
        let status_before = supervisor.status();
        let id_before = status_before[0].0.id.clone();

        // Bounce: device disappears briefly then comes back within grace.
        present.store(0, Ordering::Relaxed);
        tokio::time::sleep(Duration::from_millis(60)).await;
        present.store(1, Ordering::Relaxed);
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Still the same instance.
        assert_eq!(supervisor.active_count(), 1);
        let status_after = supervisor.status();
        assert_eq!(status_after[0].0.id, id_before);

        shutdown.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(2), sup_handle).await;
    }

    #[tokio::test]
    async fn subscription_filter_delivers_only_matching_kinds() {
        let (supervisor, pulse) = fixture();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let present = Arc::new(AtomicUsize::new(1));

        supervisor.register(ToggleableFactory {
            kind: "test.record",
            id: "filter".into(),
            subscriptions: &["core.test.a"],
            seen: seen.clone(),
            present,
        });

        let shutdown = CancellationToken::new();
        let supervisor = Arc::new(supervisor);
        let s = supervisor.clone();
        let sd = shutdown.clone();
        let sup_handle = tokio::spawn(async move { s.run(sd).await });

        // Wait for spawn.
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Publish both kinds; only core.test.a should be delivered.
        pulse.ingest(Event::new(TestEventA));
        pulse.ingest(Event::new(TestEventB));
        pulse.ingest(Event::new(TestEventA));

        tokio::time::sleep(Duration::from_millis(100)).await;

        let seen_vec = seen.lock().unwrap().clone();
        assert_eq!(
            seen_vec,
            vec!["core.test.a".to_string(), "core.test.a".to_string()]
        );

        shutdown.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(2), sup_handle).await;
    }

    #[tokio::test]
    async fn supervisor_exits_cleanly_on_cancellation() {
        let (supervisor, _pulse) = fixture();
        let shutdown = CancellationToken::new();
        let supervisor = Arc::new(supervisor);
        let s = supervisor.clone();
        let sd = shutdown.clone();
        let sup_handle = tokio::spawn(async move { s.run(sd).await });

        tokio::time::sleep(Duration::from_millis(20)).await;
        shutdown.cancel();

        tokio::time::timeout(Duration::from_secs(1), sup_handle)
            .await
            .expect("supervisor did not exit in 1s")
            .expect("supervisor panicked");
    }

    #[tokio::test]
    async fn supervisor_reaps_all_on_shutdown() {
        let (supervisor, _pulse) = fixture();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let present = Arc::new(AtomicUsize::new(1));

        supervisor.register(ToggleableFactory {
            kind: "test.record",
            id: "a".into(),
            subscriptions: &[],
            seen: seen.clone(),
            present: present.clone(),
        });
        supervisor.register(ToggleableFactory {
            kind: "test.record",
            id: "b".into(),
            subscriptions: &[],
            seen,
            present,
        });

        let shutdown = CancellationToken::new();
        let supervisor = Arc::new(supervisor);
        let s = supervisor.clone();
        let sd = shutdown.clone();
        let sup_handle = tokio::spawn(async move { s.run(sd).await });

        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(supervisor.active_count(), 2);

        shutdown.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(2), sup_handle).await;
        assert_eq!(supervisor.active_count(), 0);
    }

    // --- Skeleton tests from Chapter 2 still apply ---

    #[test]
    fn empty_supervisor_has_no_factories() {
        let (s, _) = fixture();
        assert_eq!(s.factory_count(), 0);
        assert!(s.factory_kinds().is_empty());
    }

    // --- Test helper: boxed supervisor handle ---
    //
    // We keep the supervisor Arc alive alongside the spawn handle for
    // assertion access across the test boundary.

    trait BoxedIntoExt<T> {
        fn boxed_into(self, supervisor: Option<Arc<T>>) -> SupHandle<T>;
    }
    impl BoxedIntoExt<Adapters> for JoinHandle<()> {
        fn boxed_into(self, supervisor: Option<Arc<Adapters>>) -> SupHandle<Adapters> {
            SupHandle {
                join: self,
                supervisor: supervisor.expect("supervisor required"),
            }
        }
    }
    struct SupHandle<T> {
        join: JoinHandle<()>,
        supervisor: Arc<T>,
    }

    // ---------------------------------------------------------------
    // AdapterExited event tests (COMPANION-0012 follow-up)
    // ---------------------------------------------------------------

    use super::super::exit::{AdapterExitReason, AdapterExited};

    /// Adapter that returns immediately — used to exercise SelfExit.
    struct ImmediateExitAdapter {
        id: String,
    }
    impl Adapter for ImmediateExitAdapter {
        fn info(&self) -> AdapterInfo {
            AdapterInfo {
                kind: "test.immediate-exit",
                id: self.id.clone(),
                device: None,
            }
        }
        fn profile(&self) -> AdapterProfile {
            AdapterProfile::default()
        }
        fn run(
            self: Box<Self>,
            _events: mpsc::Receiver<Event>,
            _moss: Arc<MossLocalClient>,
            _pulse: Arc<Pulse>,
            _shutdown: CancellationToken,
        ) -> super::super::adapter::BoxFuture<'static, ()> {
            Box::pin(async {})
        }
    }

    /// Adapter that runs forever until shutdown. Exercises Reaped.
    struct LongRunAdapter {
        id: String,
    }
    impl Adapter for LongRunAdapter {
        fn info(&self) -> AdapterInfo {
            AdapterInfo {
                kind: "test.long-run",
                id: self.id.clone(),
                device: None,
            }
        }
        fn profile(&self) -> AdapterProfile {
            AdapterProfile::default()
        }
        fn run(
            self: Box<Self>,
            _events: mpsc::Receiver<Event>,
            _moss: Arc<MossLocalClient>,
            _pulse: Arc<Pulse>,
            shutdown: CancellationToken,
        ) -> super::super::adapter::BoxFuture<'static, ()> {
            Box::pin(async move {
                shutdown.cancelled().await;
            })
        }
    }

    /// Adapter that panics. Exercises Panicked.
    struct PanicAdapter {
        id: String,
    }
    impl Adapter for PanicAdapter {
        fn info(&self) -> AdapterInfo {
            AdapterInfo {
                kind: "test.panic",
                id: self.id.clone(),
                device: None,
            }
        }
        fn profile(&self) -> AdapterProfile {
            AdapterProfile::default()
        }
        fn run(
            self: Box<Self>,
            _events: mpsc::Receiver<Event>,
            _moss: Arc<MossLocalClient>,
            _pulse: Arc<Pulse>,
            _shutdown: CancellationToken,
        ) -> super::super::adapter::BoxFuture<'static, ()> {
            Box::pin(async {
                panic!("boom");
            })
        }
    }

    async fn next_exit(
        rx: &mut tokio::sync::mpsc::UnboundedReceiver<AdapterExited>,
    ) -> AdapterExited {
        tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("no AdapterExited within 1s")
            .expect("exit channel closed")
    }

    #[tokio::test]
    async fn self_exit_reports_self_exit() {
        let (s, _) = fixture();
        let mut rx = s.subscribe_exits().expect("first subscribe should succeed");
        s.spawn_external(Box::new(ImmediateExitAdapter {
            id: "se-1".into(),
        }));
        let exit = next_exit(&mut rx).await;
        assert_eq!(exit.id, "se-1");
        assert_eq!(exit.reason, AdapterExitReason::SelfExit);
    }

    #[tokio::test]
    async fn reap_id_reports_reaped() {
        let (s, _) = fixture();
        let mut rx = s.subscribe_exits().unwrap();
        s.spawn_external(Box::new(LongRunAdapter { id: "lr-1".into() }));
        // Give the adapter a moment to actually start awaiting shutdown.
        tokio::time::sleep(Duration::from_millis(20)).await;
        s.reap_id("lr-1").await;
        let exit = next_exit(&mut rx).await;
        assert_eq!(exit.id, "lr-1");
        assert_eq!(exit.reason, AdapterExitReason::Reaped);
    }

    #[tokio::test]
    async fn panic_reports_panicked() {
        let (s, _) = fixture();
        let mut rx = s.subscribe_exits().unwrap();
        s.spawn_external(Box::new(PanicAdapter { id: "p-1".into() }));
        let exit = next_exit(&mut rx).await;
        assert_eq!(exit.id, "p-1");
        assert_eq!(exit.reason, AdapterExitReason::Panicked);
    }

    #[tokio::test]
    async fn subscribe_exits_is_single_consumer() {
        let (s, _) = fixture();
        assert!(s.subscribe_exits().is_some());
        assert!(s.subscribe_exits().is_none());
    }
}
