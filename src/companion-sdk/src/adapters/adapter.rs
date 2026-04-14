//! Adapter trait and profile types.
//!
//! An [`Adapter`] is a long-running task that consumes filtered events
//! from [`Pulse`] and produces local effects (device I/O, audio output,
//! metric emission, external API calls, ...). Adapters are created by
//! [`AdapterFactory`] implementations and managed by the [`Adapters`]
//! supervisor.
//!
//! See [COMPANION-0007] for the book ADR.
//!
//! [`Pulse`]: crate::garden::Pulse
//! [`AdapterFactory`]: super::AdapterFactory
//! [`Adapters`]: super::Adapters
//! [COMPANION-0007]: https://github.com/zen-garden/zen-garden/blob/dev/docs/decisions/COMPANION-0007-adapters.md

use crate::garden::{Event, Garden, Pulse};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Pinned boxed future type for `Adapter::run`.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

// ---------------------------------------------------------------------------
// The trait
// ---------------------------------------------------------------------------

/// An event-consuming extension.
///
/// # Lifecycle
///
/// 1. [`AdapterFactory::discover`] constructs instances when the supervisor
///    detects a candidate (device plugged in, endpoint appeared, ...).
/// 2. The supervisor calls [`Adapter::profile`] to learn what events to
///    forward and what delivery policy to apply.
/// 3. The supervisor spawns `run(events, garden, pulse, shutdown)` under a
///    `tracing::Span` carrying the adapter's `kind` and `id`.
/// 4. When `shutdown` is cancelled, `run` returns. The `Adapter` value is
///    dropped — its `Drop` impl performs any device cleanup.
///
/// # Implementor contract
///
/// - `info()` must return stable identity (same for the same device across
///   calls). The `id` must uniquely identify an instance; the supervisor
///   uses it for deduplication in the discovery loop.
/// - `profile()` should be cheap and deterministic (typically returns a
///   `const` or composed-from-constants value).
/// - `run()` must return when `shutdown` is cancelled. It may also return
///   on unrecoverable errors (device lost, protocol failure). Returning
///   before cancellation makes the supervisor respawn on the next
///   discovery tick that reports the device.
/// - Events arrive via the `mpsc::Receiver` in the same order the
///   supervisor's filter task forwarded them. When the sender drops
///   (supervisor shutdown), `recv()` yields `None`.
pub trait Adapter: Send + 'static {
    fn info(&self) -> AdapterInfo;
    fn profile(&self) -> AdapterProfile;

    fn run(
        self: Box<Self>,
        events: mpsc::Receiver<Event>,
        garden: Arc<Garden>,
        pulse: Arc<Pulse>,
        shutdown: CancellationToken,
    ) -> BoxFuture<'static, ()>;
}

// ---------------------------------------------------------------------------
// AdapterInfo
// ---------------------------------------------------------------------------

/// Stable identity for an adapter instance.
///
/// `kind` groups adapters by type (one per factory), `id` identifies the
/// specific instance (serial port, device serial number, "default", ...).
/// `device` is a human-readable label for status displays and logs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AdapterInfo {
    /// Stable kind tag — typically matches the factory's `kind()`.
    /// Namespace convention: `<companion>.<kind>` (e.g. `"firefly.matrix"`).
    pub kind: &'static str,

    /// Unique-within-kind instance id. Used by the supervisor to dedupe
    /// discovery output and to key persisted state.
    pub id: String,

    /// Optional human-readable device label.
    pub device: Option<String>,
}

// ---------------------------------------------------------------------------
// AdapterProfile
// ---------------------------------------------------------------------------

/// Declarative metadata describing how the supervisor should feed this
/// adapter.
///
/// Declared once at spawn time; values are baked into the supervisor's
/// dispatch path.
#[derive(Debug, Clone)]
pub struct AdapterProfile {
    /// Event kinds this adapter consumes. The supervisor's filter task
    /// only forwards matching events into the adapter's mpsc.
    ///
    /// An empty slice means "subscribe to nothing" — rarely useful but
    /// explicit. For "all events", enumerate the kinds you want.
    pub subscriptions: &'static [&'static str],

    /// How the supervisor paces event delivery to this adapter.
    pub delivery: DeliveryPolicy,

    /// `true` if this adapter opts into typed state persistence (the
    /// supervisor will manage `{state_dir}/adapters/{kind}/{id}.json`
    /// once that subsystem lands — tracked as follow-up work in
    /// COMPANION-0007).
    ///
    /// Book VI stubs this field — the supervisor observes but does not
    /// yet persist. Declaring `true` today is forward-compatible.
    pub persisted_state: bool,
}

impl Default for AdapterProfile {
    /// Default profile: no subscriptions, deliver everything that *is*
    /// subscribed as-is, no persisted state. Useful as a builder base.
    fn default() -> Self {
        Self {
            subscriptions: &[],
            delivery: DeliveryPolicy::All,
            persisted_state: false,
        }
    }
}

// ---------------------------------------------------------------------------
// DeliveryPolicy
// ---------------------------------------------------------------------------

/// How frequently the supervisor should forward subscribed events to an
/// adapter.
///
/// Only [`DeliveryPolicy::All`] is fully enforced in Book VI. The timer-
/// driven variants are declared so adapters can express their intent from
/// day one; their enforcement is tracked as follow-up work in the
/// COMPANION-0007 ADR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryPolicy {
    /// Every subscribed event delivered immediately. Default.
    All,

    /// Coalesce to the latest event per kind; emit at the given interval.
    /// Use for high-frequency state-delta events (e.g. `LoadUpdated` at
    /// matrix render rate).
    ///
    /// **Book VI behaviour**: supervisor treats this as `All`. Timer-
    /// driven coalescing lands in Book VIII or a follow-up ADR.
    LatestEvery(Duration),

    /// Quiet window after each delivery; intermediate events collapse.
    /// Use for debounced audio triggers and similar.
    ///
    /// **Book VI behaviour**: supervisor treats this as `All`. Enforcement
    /// lands in Book VIII or a follow-up ADR.
    Debounced(Duration),
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    struct NoopAdapter(&'static str, String);
    impl Adapter for NoopAdapter {
        fn info(&self) -> AdapterInfo {
            AdapterInfo {
                kind: self.0,
                id: self.1.clone(),
                device: None,
            }
        }
        fn profile(&self) -> AdapterProfile {
            AdapterProfile::default()
        }
        fn run(
            self: Box<Self>,
            _events: mpsc::Receiver<Event>,
            _garden: Arc<Garden>,
            _pulse: Arc<Pulse>,
            shutdown: CancellationToken,
        ) -> BoxFuture<'static, ()> {
            Box::pin(async move {
                shutdown.cancelled().await;
            })
        }
    }

    #[test]
    fn adapter_is_object_safe() {
        let adapters: Vec<Box<dyn Adapter>> = vec![
            Box::new(NoopAdapter("test.one", "a".into())),
            Box::new(NoopAdapter("test.two", "b".into())),
        ];
        assert_eq!(adapters.len(), 2);
        assert_eq!(adapters[0].info().kind, "test.one");
        assert_eq!(adapters[1].info().id, "b");
    }

    #[test]
    fn adapter_info_equality_by_kind_and_id() {
        let a = AdapterInfo {
            kind: "firefly.matrix",
            id: "COM5".into(),
            device: Some("RP2040-Matrix".into()),
        };
        let b = AdapterInfo {
            kind: "firefly.matrix",
            id: "COM5".into(),
            device: Some("different label".into()),
        };
        // device is part of equality per derive(PartialEq), so they differ.
        // This is intentional: equality of info is "same instance" —
        // change in device label indicates a change worth noticing.
        assert_ne!(a, b);
    }

    #[test]
    fn adapter_profile_default_is_empty_all_no_persist() {
        let p = AdapterProfile::default();
        assert!(p.subscriptions.is_empty());
        assert_eq!(p.delivery, DeliveryPolicy::All);
        assert!(!p.persisted_state);
    }

    #[test]
    fn delivery_policy_values_can_be_constructed() {
        let _ = DeliveryPolicy::All;
        let _ = DeliveryPolicy::LatestEvery(Duration::from_millis(33));
        let _ = DeliveryPolicy::Debounced(Duration::from_secs(1));
    }
}
