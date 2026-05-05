//! Engine: combines suggestion sources with dismissal state and
//! emits the current `Option<Suggestion>` to the frontend.
//!
//! The engine is recomputed on every settings change, awareness
//! change, and tending change. Pond-status changes are pulled
//! lazily — when the engine recomputes for any other reason it
//! re-fetches pond status if the tended stone changed.
//!
//! ## Dismissals
//!
//! Two layers, both checked here:
//!
//! 1. **Per-kind suppression** lives in
//!    [`crate::settings::Settings::suppressed_kinds`] under the
//!    `"facilitator:<source>"` namespace, alongside announcer
//!    toast suppressions. Persistent across sessions.
//! 2. **Per-id "Not now"** is session-local — a `HashSet<String>`
//!    held by the engine. Resets on Pavilion restart so a
//!    suggestion that re-enters the user's awareness later still
//!    surfaces.

use std::collections::HashSet;
use std::sync::Arc;

use serde_json::Value;
use tauri::{AppHandle, Emitter};
use tokio::sync::RwLock;
use tracing::{debug, warn};

use super::source::{self, SuggestionContext};
use super::types::Suggestion;
use crate::awareness::Awareness;
use crate::settings::SettingsStore;
use crate::tending::Tending;

/// Tauri event name fired when the active suggestion changes
/// (including transitions to `None`). Frontend re-fetches via
/// [`crate::commands::get_suggestion`].
pub const EVENT_SUGGESTION_CHANGED: &str = "suggestion-changed";

#[derive(Clone)]
pub struct FacilitatorEngine {
    inner: Arc<RwLock<Inner>>,
    awareness: Arc<Awareness>,
    tending: Arc<Tending>,
    settings: Arc<SettingsStore>,
    app: AppHandle,
}

struct Inner {
    /// Currently surfaced suggestion, if any.
    current: Option<Suggestion>,
    /// Per-id "Not now" dismissals — session-local.
    dismissed_ids: HashSet<String>,
    /// Cached pond_initialised for the tended stone. Refreshed
    /// lazily during `recompute`. `None` means "we haven't
    /// fetched yet" or "fetch failed."
    pond_initialised: Option<bool>,
    /// Tended stone name we last fetched pond status for; used to
    /// invalidate the cache when tending changes.
    pond_fetched_for: Option<String>,
}

impl FacilitatorEngine {
    pub fn new(
        app: AppHandle,
        awareness: Arc<Awareness>,
        tending: Arc<Tending>,
        settings: Arc<SettingsStore>,
    ) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner {
                current: None,
                dismissed_ids: HashSet::new(),
                pond_initialised: None,
                pond_fetched_for: None,
            })),
            awareness,
            tending,
            settings,
            app,
        }
    }

    /// Current suggestion snapshot for the `get_suggestion` Tauri
    /// command.
    pub async fn current(&self) -> Option<Suggestion> {
        self.inner.read().await.current.clone()
    }

    /// Mark a suggestion id as dismissed for this session. The
    /// next recompute that would emit the same id will skip and
    /// either surface a different suggestion or `None`.
    pub async fn dismiss_for_session(&self, id: &str) {
        let mut inner = self.inner.write().await;
        inner.dismissed_ids.insert(id.to_string());
        // The caller (commands.rs) re-runs recompute after this
        // returns so the change reflects in the UI promptly.
    }

    /// Recompute the active suggestion from current state. Emits
    /// `suggestion-changed` only when the result differs from the
    /// last emitted value, so listeners don't see redundant
    /// events.
    pub async fn recompute(&self) {
        // Pull state outside the lock — these reads can be slow
        // (HTTP for pond status).
        let stones = self.awareness.snapshot().await;
        let tended = self.tending.current().await;

        // Refresh the pond-status cache if tending changed (or
        // we've never fetched). One probe per tending event keeps
        // the engine cheap.
        let pond_initialised = self.refresh_pond_cache(&tended).await;

        let suggestion = source::pick(&SuggestionContext {
            stones,
            tended,
            pond_initialised,
        });

        // Filter against dismissals. Per-kind suppression first
        // (persistent), then per-id (session).
        let suppressed_kinds = self.settings.snapshot().await.suppressed_kinds;
        let dismissed_ids: HashSet<String> = self.inner.read().await.dismissed_ids.clone();

        let allowed = suggestion.filter(|s| {
            if suppressed_kinds.iter().any(|k| k == &s.kind) {
                debug!(kind = %s.kind, "facilitator: kind suppressed");
                return false;
            }
            if dismissed_ids.contains(&s.id) {
                debug!(id = %s.id, "facilitator: id dismissed for session");
                return false;
            }
            true
        });

        let changed = {
            let mut inner = self.inner.write().await;
            let same = inner.current == allowed;
            inner.current = allowed.clone();
            !same
        };

        if changed {
            if let Err(e) = self.app.emit(EVENT_SUGGESTION_CHANGED, &allowed) {
                warn!(error = %e, "facilitator: emit failed");
            }
        }
    }

    /// One-shot pond-status probe keyed on the tended stone name.
    /// Returns `None` for "no tending" / "fetch failed" — both
    /// mean no `enable_pond` suggestion should fire.
    async fn refresh_pond_cache(
        &self,
        tended: &Option<crate::tending::TendedStone>,
    ) -> Option<bool> {
        let Some(t) = tended else {
            let mut inner = self.inner.write().await;
            inner.pond_initialised = None;
            inner.pond_fetched_for = None;
            return None;
        };

        // Skip the network round-trip if we already have a
        // cached answer for this stone.
        {
            let inner = self.inner.read().await;
            if inner.pond_fetched_for.as_deref() == Some(&t.stone_name) {
                return inner.pond_initialised;
            }
        }

        let api = crate::connection::api_for(t);
        let initialised = match api.pond().status().await {
            Ok(value) => Some(extract_initialised(&value)),
            Err(e) if e.is_not_found() => Some(false),
            Err(e) => {
                debug!(error = %e, "facilitator: pond status probe failed");
                None
            }
        };

        let mut inner = self.inner.write().await;
        inner.pond_initialised = initialised;
        inner.pond_fetched_for = Some(t.stone_name.clone());
        initialised
    }
}

/// Best-effort `initialised` extraction from the free-form pond
/// status response. Mirrors the logic in `commands::get_pond_status`
/// but flattened to a single bool — we don't need name / member /
/// cornerstone here.
fn extract_initialised(raw: &Value) -> bool {
    let status = raw
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    raw.get("initialised")
        .or_else(|| raw.get("initialized"))
        .or_else(|| raw.get("active"))
        .and_then(|v| v.as_bool())
        .unwrap_or(status != "uninitialised" && status != "unknown")
}

/// Spawn the recompute supervisor. Triggers on:
///
/// - Awareness `topology-changed` events
/// - Tending changes (via the watch channel)
/// - Settings changes (a Hide-this-kind toggle should immediately
///   hide a banner; an un-hide should immediately bring it back)
/// - A periodic 30 s tick so awareness state ageing eventually
///   reflects without a topology change (the underlying signal
///   `evict()` already emits topology-changed, but the pond
///   cache may need refreshing on its own cadence)
pub fn spawn_supervisor(engine: FacilitatorEngine) {
    use std::time::Duration;
    use tokio::time::interval;

    // Initial compute so the UI has something on first paint.
    let bootstrap = engine.clone();
    tauri::async_runtime::spawn(async move {
        bootstrap.recompute().await;
    });

    // Tending changes — drives the pond cache invalidation.
    let tending_engine = engine.clone();
    let tending = engine.tending.clone();
    tauri::async_runtime::spawn(async move {
        let mut rx = tending.subscribe();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            tending_engine.recompute().await;
        }
    });

    // Settings changes.
    let settings_engine = engine.clone();
    let settings = engine.settings.clone();
    tauri::async_runtime::spawn(async move {
        let mut rx = settings.subscribe();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            settings_engine.recompute().await;
        }
    });

    // Periodic tick — covers awareness ageing without a
    // topology event and lets the pond cache refresh
    // periodically on long-lived sessions.
    let tick_engine = engine;
    tauri::async_runtime::spawn(async move {
        let mut tick = interval(Duration::from_secs(30));
        // First tick fires immediately; we already bootstrap-ed
        // above, so swallow it.
        tick.tick().await;
        loop {
            tick.tick().await;
            tick_engine.recompute().await;
        }
    });
}
