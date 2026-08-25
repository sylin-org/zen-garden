//! Adapter lifecycle domain events.
//!
//! [`AdapterExited`] is published by the [`super::Adapters`] supervisor
//! whenever an adapter's run-task completes. The event carries an
//! [`AdapterExitReason`] so subscribers can distinguish externally-
//! requested teardown from a self-initiated exit or a panic.
//!
//! Consumers (today: the device bus) subscribe via
//! [`super::Adapters::subscribe_exits`] to keep their own port-ownership
//! state synchronized with the supervisor's adapter-instance state —
//! without polling.

/// Why an adapter's run-task ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterExitReason {
    /// External teardown requested via [`super::Adapters::reap_id`] — the
    /// shutdown token was cancelled and the adapter cooperatively
    /// returned. This is the expected path during ordered shutdown
    /// and bus-detected device detach.
    Reaped,

    /// The adapter's run future returned without the shutdown token
    /// having been cancelled. The events channel may have closed,
    /// or the adapter chose to exit on its own.
    SelfExit,

    /// The adapter's run future panicked. The supervisor caught the
    /// `JoinError`; the panic did not propagate.
    Panicked,
}

/// Notification that an adapter's run-task has finished.
#[derive(Debug, Clone)]
pub struct AdapterExited {
    /// Adapter id (matches [`super::AdapterInfo::id`] of the spawned
    /// adapter).
    pub id: String,
    /// Why the task ended.
    pub reason: AdapterExitReason,
}
