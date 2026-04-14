//! `Companion` — the top-level runtime that wires together Pulse,
//! Garden, Adapters, and Transports into a runnable companion binary.
//!
//! See [COMPANION-0008] for the book ADR.
//!
//! # Usage
//!
//! ```ignore
//! use garden_companion_sdk::{Companion, SseTransport, CommandTransport};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     Companion::new("firefly")
//!         .with_state_dir("/var/lib/zen-garden/companions/firefly")
//!         .with_transport(SseTransport::new("http://localhost:7185"))
//!         .with_transport(CommandTransport::new(7188))
//!         .with_adapter_factory(RpMatrixFactory)
//!         .run()
//!         .await
//! }
//! ```
//!
//! [COMPANION-0008]: https://github.com/zen-garden/zen-garden/blob/dev/docs/decisions/COMPANION-0008-companion.md

use crate::adapters::{AdapterFactory, Adapters};
use crate::garden::{Garden, Pulse, Transport, kind_namespace};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Default coalesced-event flush cadence per the pattern spec.
pub const DEFAULT_FLUSH_INTERVAL: Duration = Duration::from_millis(50);

/// File name used for the enabled-flag persistence inside `state_dir`.
const ENABLED_FILENAME: &str = "enabled";

/// Top-level companion runtime. Wires together the Garden-context
/// (Pulse, Garden, transports) and the Adapters-context (supervisor,
/// factories) into a single runnable unit.
///
/// Construct via [`Companion::new`], attach transports and factories
/// with the fluent `with_*` methods, and call [`Companion::run`] to
/// start. `run` returns when the shutdown token is cancelled (via
/// OS signal, `CommandTransport`'s `/shutdown`, or programmatic
/// [`Companion::shutdown_token`] cancellation).
pub struct Companion {
    name: String,
    pulse: Arc<Pulse>,
    garden: Arc<Garden>,
    adapters: Arc<Adapters>,
    transports: Vec<Box<dyn Transport>>,
    enabled: Arc<AtomicBool>,
    state_dir: Option<PathBuf>,
    shutdown: CancellationToken,
    flush_interval: Duration,
}

impl Companion {
    /// Construct with a companion name. Name is used in log spans and
    /// health responses; it need not be unique across processes.
    pub fn new(name: impl Into<String>) -> Self {
        let pulse = Arc::new(Pulse::with_defaults());
        let garden = Garden::new(pulse.clone());
        let adapters = Arc::new(Adapters::new(garden.clone(), pulse.clone()));
        Self {
            name: name.into(),
            pulse,
            garden,
            adapters,
            transports: Vec::new(),
            enabled: Arc::new(AtomicBool::new(true)),
            state_dir: None,
            shutdown: CancellationToken::new(),
            flush_interval: DEFAULT_FLUSH_INTERVAL,
        }
    }

    /// Set the state directory for persistent flags. If the `enabled`
    /// file exists at `{dir}/enabled`, its value is loaded. Errors
    /// reading or writing this file are logged at `warn` level and
    /// never propagated.
    pub fn with_state_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        let dir: PathBuf = dir.into();
        if let Some(loaded) = read_enabled(&dir) {
            self.enabled.store(loaded, Ordering::Relaxed);
        }
        self.state_dir = Some(dir);
        self
    }

    /// Override the default coalesced-event flush interval.
    pub fn with_flush_interval(mut self, d: Duration) -> Self {
        self.flush_interval = d;
        self
    }

    /// Attach a transport. It will be spawned on `run()`.
    pub fn with_transport<T: Transport>(mut self, transport: T) -> Self {
        self.transports.push(Box::new(transport));
        self
    }

    /// Register an adapter factory with the supervisor.
    pub fn with_adapter_factory<F: AdapterFactory>(self, factory: F) -> Self {
        self.adapters.register(factory);
        self
    }

    // --- Accessors ---

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn pulse(&self) -> Arc<Pulse> {
        self.pulse.clone()
    }

    pub fn garden(&self) -> Arc<Garden> {
        self.garden.clone()
    }

    pub fn adapters(&self) -> Arc<Adapters> {
        self.adapters.clone()
    }

    /// A clone of the companion's shutdown token. Cancelling this
    /// triggers a clean shutdown of the running companion.
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown.clone()
    }

    /// Current value of the enabled flag. Adapters may observe this
    /// to decide whether to render side-effects or pass through in
    /// idle/paused mode.
    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Update the enabled flag. Persists to `{state_dir}/enabled` if
    /// a state dir was configured.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
        if let Some(dir) = &self.state_dir {
            write_enabled(dir, enabled);
        }
    }

    /// Run until shutdown. Spawns:
    /// - a flush timer task that invokes `pulse.flush_coalesced` periodically
    /// - Garden's projection task
    /// - Adapters' supervisor
    /// - each attached transport
    ///
    /// Returns when cancellation is signalled (Ctrl+C, `/shutdown`, or a
    /// programmatic cancel via [`Companion::shutdown_token`]).
    pub async fn run(self) -> anyhow::Result<()> {
        let Self {
            name,
            pulse,
            garden,
            adapters,
            transports,
            enabled: _,
            state_dir: _,
            shutdown,
            flush_interval,
        } = self;

        tracing::info!(companion = %name, "Companion starting");

        // 1. Auto-register namespaces.
        pulse.register_namespace("core");
        for transport in &transports {
            for kind in transport.emitted_kinds() {
                if let Some(ns) = kind_namespace(kind) {
                    pulse.register_namespace(static_from_kind(kind, ns));
                }
            }
        }

        // 2. Flush timer.
        let flush_handle = {
            let pulse = pulse.clone();
            let shutdown = shutdown.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(flush_interval);
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                interval.tick().await; // consume immediate first tick
                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            pulse.flush_coalesced();
                        }
                        _ = shutdown.cancelled() => break,
                    }
                }
            })
        };

        // 3. Garden projection.
        let projection_handle = garden.spawn_projection(shutdown.clone());

        // 4. Adapters supervisor.
        let supervisor_handle = {
            let adapters = adapters.clone();
            let shutdown = shutdown.clone();
            tokio::spawn(async move {
                adapters.run(shutdown).await;
            })
        };

        // 5. Transports.
        let transport_handles: Vec<_> = transports
            .into_iter()
            .map(|t| {
                let pulse = pulse.clone();
                let shutdown = shutdown.clone();
                tokio::spawn(async move {
                    t.run(pulse, shutdown).await;
                })
            })
            .collect();

        // 6. Wait for shutdown signal (OS signal or programmatic cancel).
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Ctrl+C received");
            }
            _ = shutdown.cancelled() => {
                tracing::info!("Shutdown token cancelled");
            }
        }
        shutdown.cancel();

        // 7. Bounded join.
        let _ = tokio::time::timeout(Duration::from_secs(10), async {
            let _ = flush_handle.await;
            let _ = projection_handle.await;
            let _ = supervisor_handle.await;
            for h in transport_handles {
                let _ = h.await;
            }
        })
        .await;

        tracing::info!(companion = %name, "Companion stopped");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Lifetime-shim for namespace registration
// ---------------------------------------------------------------------------

/// Compute a `&'static str` namespace from a `&'static str` kind.
///
/// `kind_namespace` returns `&str` borrowed from its input. When the
/// input is `&'static str` (which ours always is, since kinds are const
/// literals), the output is also effectively `'static`. Rust's borrow
/// checker doesn't always see this; this helper handles the conversion
/// safely for the tiny table of known namespace prefixes.
///
/// In practice the set of companion-epic namespaces is small
/// (`core`, `firefly`, `cricket`, plus any adapter-provided namespace).
/// We match against known prefixes; unknown prefixes fall back to a
/// best-effort leak-via-intern path that, for Book VII, is narrowly
/// scoped to the transport's `emitted_kinds` const slice.
fn static_from_kind(_kind: &'static str, ns: &str) -> &'static str {
    // The input kind is 'static; ns is a sub-slice of it, so ns itself
    // is a 'static &str when kind is. Rust can't always infer that, so
    // we pattern-match known values for clean 'static references.
    match ns {
        "core" => "core",
        "firefly" => "firefly",
        "cricket" => "cricket",
        "observability" => "observability",
        _ => {
            // Fallback for unusual namespaces (e.g. third-party adapter
            // crates). Safe for the companion-epic's scope: kinds are
            // compile-time constants and this function is only called
            // from `run()` at startup, so leaking is bounded by the
            // distinct set of namespaces across registered transports
            // (typically 1-3).
            Box::leak(ns.to_string().into_boxed_str())
        }
    }
}

// ---------------------------------------------------------------------------
// Enabled-flag file I/O
// ---------------------------------------------------------------------------

/// Read the enabled flag from `{dir}/enabled`. Returns `None` if the
/// file is missing, unreadable, or its contents don't parse.
fn read_enabled(dir: &std::path::Path) -> Option<bool> {
    let path = dir.join(ENABLED_FILENAME);
    match std::fs::read_to_string(&path) {
        Ok(content) => match content.trim() {
            "on" => Some(true),
            "off" => Some(false),
            other => {
                tracing::warn!(path = %path.display(), content = %other, "unexpected enabled-flag content; ignoring");
                None
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "failed to read enabled flag");
            None
        }
    }
}

/// Write the enabled flag to `{dir}/enabled`. Logs warnings on
/// persistence failure and otherwise returns silently.
fn write_enabled(dir: &std::path::Path, enabled: bool) {
    let path = dir.join(ENABLED_FILENAME);
    if let Err(e) = std::fs::create_dir_all(dir) {
        tracing::warn!(dir = %dir.display(), error = %e, "failed to ensure state_dir");
        return;
    }
    let content = if enabled { "on" } else { "off" };
    if let Err(e) = std::fs::write(&path, content) {
        tracing::warn!(path = %path.display(), error = %e, "failed to write enabled flag");
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{Adapter, AdapterInfo, AdapterProfile};
    use crate::garden::{
        CommandInvocation, CommandOutcome, CommandResult, CommandTransport, Event, EventPayload,
    };
    use std::any::Any;
    use std::sync::Mutex;
    use tokio::sync::mpsc;

    // --- Fixtures ---

    #[derive(Debug)]
    struct FauxTransport {
        kinds: &'static [&'static str],
        ran: Arc<AtomicBool>,
    }
    impl Transport for FauxTransport {
        fn run(
            self: Box<Self>,
            _pulse: Arc<Pulse>,
            shutdown: CancellationToken,
        ) -> crate::garden::BoxFuture<'static, ()> {
            let ran = self.ran;
            Box::pin(async move {
                ran.store(true, Ordering::Relaxed);
                shutdown.cancelled().await;
            })
        }
        fn emitted_kinds(&self) -> &'static [&'static str] {
            self.kinds
        }
    }

    struct EchoAdapter {
        id: String,
        recorded: Arc<Mutex<Vec<String>>>,
    }

    impl Adapter for EchoAdapter {
        fn info(&self) -> AdapterInfo {
            AdapterInfo {
                kind: "test.echo",
                id: self.id.clone(),
                device: None,
            }
        }
        fn profile(&self) -> AdapterProfile {
            AdapterProfile {
                subscriptions: &["core.command.invocation"],
                ..AdapterProfile::default()
            }
        }
        fn run(
            self: Box<Self>,
            mut events: mpsc::Receiver<Event>,
            _garden: Arc<Garden>,
            pulse: Arc<Pulse>,
            shutdown: CancellationToken,
        ) -> crate::adapters::adapter::BoxFuture<'static, ()> {
            Box::pin(async move {
                loop {
                    tokio::select! {
                        maybe = events.recv() => match maybe {
                            Some(evt) => {
                                if let Some(inv) = evt.payload::<CommandInvocation>() {
                                    self.recorded
                                        .lock()
                                        .unwrap()
                                        .push(inv.raw_args.join(" "));
                                    let _ = pulse.ingest(Event::new(CommandResult {
                                        correlation_id: inv.correlation_id,
                                        outcome: CommandOutcome::Success {
                                            output: Some("echo".into()),
                                        },
                                        from: "test-echo".into(),
                                    }));
                                }
                            }
                            None => break,
                        },
                        _ = shutdown.cancelled() => break,
                    }
                }
            })
        }
    }

    struct EchoFactory {
        recorded: Arc<Mutex<Vec<String>>>,
    }
    impl AdapterFactory for EchoFactory {
        fn kind(&self) -> &'static str {
            "test.echo"
        }
        fn discover(&self) -> Vec<Box<dyn Adapter>> {
            vec![Box::new(EchoAdapter {
                id: "only".into(),
                recorded: self.recorded.clone(),
            })]
        }
    }

    #[derive(Debug, Clone)]
    struct LoadUpdate;
    impl EventPayload for LoadUpdate {
        const KIND: &'static str = "core.stone.load.updated";
        const COALESCING: bool = true;
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    // --- Basic builder ---

    #[test]
    fn new_creates_companion_with_defaults() {
        let c = Companion::new("test-companion");
        assert_eq!(c.name(), "test-companion");
        assert!(c.enabled());
        assert_eq!(c.flush_interval, DEFAULT_FLUSH_INTERVAL);
        assert_eq!(c.transports.len(), 0);
        assert_eq!(c.adapters().factory_count(), 0);
        assert_eq!(c.pulse().receiver_count(), 0);
    }

    #[test]
    fn builder_attaches_transports_and_factories() {
        let ran = Arc::new(AtomicBool::new(false));
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let c = Companion::new("x")
            .with_transport(FauxTransport {
                kinds: &["core.presence.snapshot"],
                ran: ran.clone(),
            })
            .with_adapter_factory(EchoFactory {
                recorded: recorded.clone(),
            });
        assert_eq!(c.transports.len(), 1);
        assert_eq!(c.adapters().factory_count(), 1);
        assert!(!ran.load(Ordering::Relaxed));
    }

    // --- Enabled flag ---

    #[test]
    fn enabled_defaults_to_true() {
        let c = Companion::new("x");
        assert!(c.enabled());
    }

    #[test]
    fn set_enabled_updates_flag() {
        let c = Companion::new("x");
        c.set_enabled(false);
        assert!(!c.enabled());
        c.set_enabled(true);
        assert!(c.enabled());
    }

    #[test]
    fn enabled_persists_to_state_dir() {
        let dir = tempfile::tempdir().unwrap();
        let c = Companion::new("x").with_state_dir(dir.path());
        c.set_enabled(false);

        let file = dir.path().join(ENABLED_FILENAME);
        let content = std::fs::read_to_string(file).unwrap();
        assert_eq!(content.trim(), "off");

        // Second companion loads the persisted state.
        let c2 = Companion::new("x").with_state_dir(dir.path());
        assert!(!c2.enabled());
    }

    #[test]
    fn enabled_load_tolerates_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let c = Companion::new("x").with_state_dir(dir.path());
        assert!(c.enabled()); // default true when no file
    }

    #[test]
    fn enabled_load_tolerates_garbage_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(ENABLED_FILENAME);
        std::fs::write(&path, "garbage").unwrap();
        let c = Companion::new("x").with_state_dir(dir.path());
        assert!(c.enabled()); // falls back to default
    }

    // --- Integration: run() ---

    #[tokio::test]
    async fn run_exits_on_cancellation() {
        let c = Companion::new("x").with_flush_interval(Duration::from_millis(10));
        let shutdown = c.shutdown_token();

        let run_handle = tokio::spawn(async move {
            c.run().await.unwrap();
        });

        tokio::time::sleep(Duration::from_millis(50)).await;
        shutdown.cancel();

        tokio::time::timeout(Duration::from_secs(5), run_handle)
            .await
            .expect("companion did not exit in 5s")
            .expect("companion panicked");
    }

    #[tokio::test]
    async fn run_spawns_transport_and_adapter() {
        let ran = Arc::new(AtomicBool::new(false));
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let c = Companion::new("x")
            .with_flush_interval(Duration::from_millis(10))
            .with_transport(FauxTransport {
                kinds: &["core.presence.snapshot"],
                ran: ran.clone(),
            })
            .with_adapter_factory(EchoFactory {
                recorded: recorded.clone(),
            });
        // Override supervisor cadence through the public handle.
        let shutdown = c.shutdown_token();

        let run_handle = tokio::spawn(async move {
            c.run().await.unwrap();
        });

        // Give the transport time to be spawned and the supervisor to tick.
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(ran.load(Ordering::Relaxed));

        shutdown.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(5), run_handle).await;
    }

    #[tokio::test]
    async fn run_flushes_coalesced_events_on_timer() {
        let c = Companion::new("x").with_flush_interval(Duration::from_millis(10));
        let pulse = c.pulse();
        pulse.register_namespace("core");
        let shutdown = c.shutdown_token();
        let mut rx = pulse.subscribe();

        let run_handle = tokio::spawn(async move {
            c.run().await.unwrap();
        });

        // Publish a coalescing event; it should be buffered in Pulse,
        // then flushed by the timer task.
        tokio::time::sleep(Duration::from_millis(30)).await;
        pulse.ingest(Event::new(LoadUpdate));

        // Wait for flush.
        let flushed = tokio::time::timeout(Duration::from_millis(200), rx.recv()).await;
        assert!(flushed.is_ok(), "coalesced event never flushed");

        shutdown.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(5), run_handle).await;
    }

    #[tokio::test]
    async fn end_to_end_command_dispatches_through_companion() {
        // Real end-to-end: Companion + CommandTransport + EchoAdapter.
        // Send HTTP POST /command; EchoAdapter responds; we get the
        // aggregated CommandResponse back.
        let recorded = Arc::new(Mutex::new(Vec::new()));

        // Pick ephemeral port.
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let c = Companion::new("x")
            .with_flush_interval(Duration::from_millis(10))
            .with_transport(
                CommandTransport::new(port).with_timeout(Duration::from_millis(500)),
            )
            .with_adapter_factory(EchoFactory {
                recorded: recorded.clone(),
            });
        let shutdown = c.shutdown_token();

        let run_handle = tokio::spawn(async move {
            c.run().await.unwrap();
        });

        // Wait for everything to start.
        tokio::time::sleep(Duration::from_millis(200)).await;

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("http://127.0.0.1:{}/command", port))
            .json(&serde_json::json!({ "raw_args": ["hello", "world"] }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: garden_common::command_manifest::CommandResponse = resp.json().await.unwrap();
        assert!(body.is_success());

        let captured = recorded.lock().unwrap().clone();
        assert_eq!(captured, vec!["hello world".to_string()]);

        shutdown.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(5), run_handle).await;
    }
}
