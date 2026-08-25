//! `Subsystems` aggregate — DDD root of the Subsystems bounded context.
//!
//! Book VI of ARCH-0017 (ARCH-0023). Replaces the `SubSystems` struct
//! of `Arc<AtomicBool>` fields with a registration-based aggregate
//! backed by `tokio::sync::watch` channels.
//!
//! ## Design
//!
//! Subsystems are registered by name at bootstrap time. Each
//! registration creates a `watch::Sender<bool>` (held by the
//! aggregate) and a `watch::Receiver<bool>` (returned to the caller
//! or obtained later via `receiver()`). Producers call
//! `mark_ready`/`mark_unready`; consumers poll `is_ready` or await
//! `wait_ready`.
//!
//! ## Pattern deviations
//!
//! - **Ephemeral** — no persistence port. Subsystem readiness is
//!   runtime-only (matches Metrics Book I, Jobs Book IV).
//! - **Infallible mutations** — `mark_ready`/`mark_unready` are
//!   no-ops on unknown subsystem names (warn-level trace). No
//!   `SubsystemsError` type (matches Metrics Book I, Jobs Book IV).
//! - **No internal `RwLock`** — the `HashMap<String, watch::Sender<bool>>`
//!   is populated at registration time (single-threaded bootstrap)
//!   and never structurally modified afterward. `watch::Sender::send()`
//!   is inherently thread-safe. This is a simplification over the
//!   standard `RwLock<State>` pattern.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{broadcast, watch};

use super::event::{ChangeKind, SubsystemsChanged};
use crate::domain::Metrics;

/// Capacity of the internal `SubsystemsChanged` broadcast channel.
const CHANGES_CHANNEL_CAPACITY: usize = 64;

/// Snapshot of a single subsystem's readiness state.
#[derive(Debug, Clone)]
pub struct SubsystemStatus {
    /// Subsystem name (e.g., "network", "docker").
    pub name: String,
    /// Whether the subsystem is currently ready.
    pub ready: bool,
}

/// `Subsystems` bounded context.
///
/// Ephemeral aggregate — no persistence, no store port. Subsystems are
/// registered at bootstrap and their readiness is toggled by monitor
/// tasks. Consumers poll `is_ready()` or await `wait_ready()`.
pub struct Subsystems {
    /// Per-subsystem watch senders. Populated at registration time,
    /// structurally immutable afterward.
    state: HashMap<String, watch::Sender<bool>>,

    /// Metrics aggregate for readiness-transition counters.
    metrics: Arc<Metrics>,

    /// Internal `SubsystemsChanged` broadcast.
    changes: broadcast::Sender<SubsystemsChanged>,
}

impl Subsystems {
    /// Create a new empty `Subsystems` aggregate.
    ///
    /// Call `register()` for each subsystem before handing the
    /// aggregate to concurrent consumers.
    #[tracing::instrument(skip_all, name = "Subsystems::new")]
    pub async fn new(metrics: Arc<Metrics>) -> Self {
        let (changes, _) = broadcast::channel(CHANGES_CHANNEL_CAPACITY);

        metrics
            .register_domain("subsystems", ChangeKind::ALL_NAMES)
            .await;

        Self {
            state: HashMap::new(),
            metrics,
            changes,
        }
    }

    // ── Commands ──────────────────────────────────────────────────

    /// Register a subsystem by name. Must be called during bootstrap
    /// (single-threaded) before the aggregate is shared.
    ///
    /// # Panics
    ///
    /// Panics if a subsystem with the same name is already registered.
    /// This is a programming error, not a runtime condition.
    pub fn register(&mut self, name: &str) {
        if self.state.contains_key(name) {
            panic!("Subsystem '{}' is already registered", name);
        }
        let (tx, _) = watch::channel(false);
        self.state.insert(name.to_owned(), tx);
        tracing::debug!(subsystem = %name, "Subsystem registered");
    }

    /// Mark a subsystem as ready.
    ///
    /// No-op if the subsystem is already ready or if the name is
    /// unknown (warn-level trace). Fires a `SubsystemsChanged::Ready`
    /// event on interesting transitions only.
    #[tracing::instrument(skip(self), fields(subsystem = %name))]
    pub async fn mark_ready(&self, name: &str) {
        let Some(tx) = self.state.get(name) else {
            tracing::warn!(subsystem = %name, "mark_ready: unknown subsystem");
            return;
        };

        // Only fire on interesting transition (false → true)
        if *tx.borrow() {
            return;
        }

        tx.send_modify(|v| *v = true);
        self.emit(ChangeKind::Ready {
            name: name.to_owned(),
        })
        .await;
        tracing::info!(subsystem = %name, "Subsystem ready");
    }

    /// Mark a subsystem as not ready.
    ///
    /// No-op if the subsystem is already not ready or if the name is
    /// unknown. Fires a `SubsystemsChanged::Unready` event on
    /// interesting transitions only.
    #[tracing::instrument(skip(self), fields(subsystem = %name))]
    pub async fn mark_unready(&self, name: &str, reason: &str) {
        let Some(tx) = self.state.get(name) else {
            tracing::warn!(subsystem = %name, "mark_unready: unknown subsystem");
            return;
        };

        // Only fire on interesting transition (true → false)
        if !*tx.borrow() {
            return;
        }

        tx.send_modify(|v| *v = false);
        self.emit(ChangeKind::Unready {
            name: name.to_owned(),
            reason: reason.to_owned(),
        })
        .await;
        tracing::info!(subsystem = %name, reason = %reason, "Subsystem unready");
    }

    // ── Queries ───────────────────────────────────────────────────

    /// Check whether a subsystem is currently ready.
    ///
    /// Returns `false` for unknown subsystem names (warn-level trace).
    /// This is a synchronous, zero-cost poll — no lock, no await.
    pub fn is_ready(&self, name: &str) -> bool {
        match self.state.get(name) {
            Some(tx) => *tx.borrow(),
            None => {
                tracing::warn!(subsystem = %name, "is_ready: unknown subsystem");
                false
            }
        }
    }

    /// Wait until a subsystem becomes ready.
    ///
    /// Returns immediately if already ready. Returns `false` for
    /// unknown subsystem names (no block).
    pub async fn wait_ready(&self, name: &str) -> bool {
        let Some(tx) = self.state.get(name) else {
            tracing::warn!(subsystem = %name, "wait_ready: unknown subsystem");
            return false;
        };

        let mut rx = tx.subscribe();
        // If already ready, return immediately
        if *rx.borrow() {
            return true;
        }
        // Wait for the next change
        loop {
            if rx.changed().await.is_err() {
                // Sender dropped — subsystem will never become ready
                return false;
            }
            if *rx.borrow() {
                return true;
            }
        }
    }

    /// Snapshot of all registered subsystems and their readiness.
    pub fn snapshot(&self) -> Vec<SubsystemStatus> {
        self.state
            .iter()
            .map(|(name, tx)| SubsystemStatus {
                name: name.clone(),
                ready: *tx.borrow(),
            })
            .collect()
    }

    /// Subscribe to the `SubsystemsChanged` event stream.
    pub fn changes(&self) -> broadcast::Receiver<SubsystemsChanged> {
        self.changes.subscribe()
    }

    // ── Internals ─────────────────────────────────────────────────

    /// Emit a domain event and record metrics.
    async fn emit(&self, kind: ChangeKind) {
        self.metrics
            .record_domain_event("subsystems", kind.name())
            .await;
        let _ = self.changes.send(SubsystemsChanged {
            kind,
            timestamp: chrono::Utc::now(),
        });
    }
}
