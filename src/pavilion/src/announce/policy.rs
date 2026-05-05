//! Announcer — coalesces, dedupes, and decides what crosses the
//! toast threshold.
//!
//! Observers feed raw [`GardenEvent`]s into [`Announcer::observe`];
//! the announcer maintains per-key state to coalesce within a window,
//! then applies promotion policy:
//!
//! | Event kind | Window | Toast policy |
//! |------------|--------|--------------|
//! | StoneJoined / StoneLeft | none | always promote |
//! | StorageActivity | 30 s | promote if window total ≥ 5 |
//!
//! Quiet hours, per-source dismissal, and per-suggestion cooldowns
//! from the spec are not yet wired — they require a Settings store
//! that hasn't landed. The promote-/-don't-promote decision is the
//! seam where they will hook in.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use uuid::Uuid;

use super::event::{ActivityEntry, GardenEvent};
use super::store::ActivityStore;
use super::toast;
use crate::settings::SettingsStore;

/// Tauri event name fired when a new activity entry has landed.
/// Frontend subscribers re-fetch via `get_activity` rather than
/// receiving the entry inline — keeps the wire payload empty and
/// avoids races when multiple entries land in quick succession.
pub const EVENT_ACTIVITY_CHANGED: &str = "activity-changed";

/// Coalescing window for `StorageActivity` — ticks within this span
/// for the same `(stone, bank)` collapse into one accepted entry.
const STORAGE_WINDOW: Duration = Duration::from_secs(30);

/// Total-activity threshold above which a coalesced storage event is
/// also fired as a toast. Below it, the entry still lands in Activity
/// but stays quiet.
const STORAGE_TOAST_THRESHOLD: u32 = 5;

/// Cold-start quiet window. Stones that respond to the initial
/// discovery probe within this span land in Activity but never fire
/// toasts — a fresh Pavilion launch shouldn't dump 14 notifications
/// in the user's face just because it discovered the LAN they were
/// already on. After the window closes, normal promotion policy
/// applies and any *new* stone joining fires its toast.
const STARTUP_QUIET_WINDOW: Duration = Duration::from_secs(5);

/// Default per-event cooldown. After a toast for a specific
/// `(kind, key)` fires, the same key can't fire another toast
/// within this window — covers stone-flapping (a flaky USB
/// adapter / WiFi hiccup) and rapid storage-burst tail events
/// without dumping every transition into the user's tray.
///
/// The activity entry still lands either way; cooldown only gates
/// the toast surface.
const DEFAULT_COOLDOWN: Duration = Duration::from_secs(60);

/// In-progress coalesce slot for a `StorageActivity` key.
struct StorageWindow {
    stone_name: String,
    bank_name: String,
    creates: u32,
    modifies: u32,
    deletes: u32,
    /// First tick that opened this window — used to decide when to
    /// flush.
    opened_at: Instant,
}

#[derive(Clone)]
pub struct Announcer {
    inner: Arc<Mutex<Inner>>,
    store: ActivityStore,
    settings: Arc<SettingsStore>,
    app: AppHandle,
    started_at: Instant,
}

struct Inner {
    /// Active storage coalesce windows keyed by `dedupe_key()`.
    storage_windows: HashMap<String, StorageWindow>,
    /// Last toast-fire timestamp per `dedupe_key()`. Drives the
    /// `DEFAULT_COOLDOWN` gate so flapping events don't spam the
    /// tray. Entries stay forever; for a user session that's fine
    /// (the key set is bounded by the live garden's stone+bank
    /// count).
    last_toasted: HashMap<String, Instant>,
}

impl Announcer {
    pub fn new(app: AppHandle, store: ActivityStore, settings: Arc<SettingsStore>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                storage_windows: HashMap::new(),
                last_toasted: HashMap::new(),
            })),
            store,
            settings,
            app,
            started_at: Instant::now(),
        }
    }

    /// Whether the cold-start quiet window has elapsed. Until it
    /// has, accepted events skip toast promotion.
    fn past_warmup(&self) -> bool {
        self.started_at.elapsed() >= STARTUP_QUIET_WINDOW
    }

    /// Final promotion gate. Layered checks, fail-closed:
    ///
    /// 1. cold-start warmup
    /// 2. per-kind suppression ("Hide this kind")
    /// 3. quiet hours
    /// 4. per-key cooldown (recently-toasted)
    ///
    /// Each layer can veto; `true` only when every check passes.
    /// The activity entry still lands in the ring buffer either
    /// way — this only controls whether the user sees a toast.
    async fn should_promote_now(&self, event: &GardenEvent) -> bool {
        if !self.past_warmup() {
            return false;
        }
        let snap = self.settings.snapshot().await;
        if snap.is_suppressed(event.kind_str()) {
            return false;
        }
        if snap.is_quiet_now(chrono::Local::now().time()) {
            return false;
        }
        let key = event.dedupe_key();
        let now = Instant::now();
        let inner = self.inner.lock().await;
        if let Some(last) = inner.last_toasted.get(&key)
            && now.duration_since(*last) < DEFAULT_COOLDOWN
        {
            tracing::debug!(
                kind = event.kind_str(),
                key = %key,
                "announcer: toast cooldown active, suppressing"
            );
            return false;
        }
        true
    }

    /// Record that a toast fired for this event so the cooldown
    /// gate can suppress repeats. Called from `accept` only on
    /// the path that actually called `toast::fire`.
    async fn mark_toasted(&self, event: &GardenEvent) {
        let key = event.dedupe_key();
        let mut inner = self.inner.lock().await;
        inner.last_toasted.insert(key, Instant::now());
    }

    /// Borrow of the activity store — useful in tests and future
    /// modules that want to read recent activity inline rather than
    /// going through the Tauri command surface.
    #[allow(dead_code)]
    pub fn store(&self) -> &ActivityStore {
        &self.store
    }

    /// Feed a raw event into the announcer. Returns immediately;
    /// promotion (toast + activity push) happens before the call
    /// returns for non-coalesced kinds, or asynchronously when a
    /// coalesce window flushes.
    pub async fn observe(&self, event: GardenEvent) {
        match event {
            GardenEvent::StoneJoined { .. } | GardenEvent::StoneLeft { .. } => {
                // No coalescing. Whether to toast is decided by the
                // full promotion gate (warmup + suppressions +
                // quiet hours).
                let promote = self.should_promote_now(&event).await;
                self.accept(event, promote).await;
            }
            GardenEvent::StorageActivity { .. } => {
                self.absorb_storage_tick(event).await;
            }
        }
    }

    /// Drive coalesce-window flushes. Call from a periodic timer task
    /// (every few seconds is fine — the window is 30s).
    pub async fn tick(&self) {
        let now = Instant::now();
        let to_flush: Vec<StorageWindow> = {
            let mut inner = self.inner.lock().await;
            let keys: Vec<String> = inner
                .storage_windows
                .iter()
                .filter(|(_, w)| now.duration_since(w.opened_at) >= STORAGE_WINDOW)
                .map(|(k, _)| k.clone())
                .collect();
            keys.into_iter()
                .filter_map(|k| inner.storage_windows.remove(&k))
                .collect()
        };

        for window in to_flush {
            let total = window.creates + window.modifies + window.deletes;
            if total == 0 {
                // Defensive — empty window shouldn't be possible.
                continue;
            }
            let event = GardenEvent::StorageActivity {
                stone_name: window.stone_name,
                bank_name: window.bank_name,
                creates: window.creates,
                modifies: window.modifies,
                deletes: window.deletes,
            };
            // Threshold first (a quiet sync stays in Activity even
            // when no other gate would suppress it), then the full
            // gate (warmup, suppressions, quiet hours).
            let promote =
                total >= STORAGE_TOAST_THRESHOLD && self.should_promote_now(&event).await;
            self.accept(event, promote).await;
        }
    }

    async fn absorb_storage_tick(&self, event: GardenEvent) {
        let GardenEvent::StorageActivity {
            stone_name,
            bank_name,
            creates,
            modifies,
            deletes,
        } = event
        else {
            return;
        };
        let key_event = GardenEvent::StorageActivity {
            stone_name: stone_name.clone(),
            bank_name: bank_name.clone(),
            creates: 0,
            modifies: 0,
            deletes: 0,
        };
        let key = key_event.dedupe_key();
        let mut inner = self.inner.lock().await;
        let window = inner.storage_windows.entry(key).or_insert(StorageWindow {
            stone_name: stone_name.clone(),
            bank_name: bank_name.clone(),
            creates: 0,
            modifies: 0,
            deletes: 0,
            opened_at: Instant::now(),
        });
        window.creates = window.creates.saturating_add(creates);
        window.modifies = window.modifies.saturating_add(modifies);
        window.deletes = window.deletes.saturating_add(deletes);
    }

    async fn accept(&self, event: GardenEvent, promote: bool) {
        let severity = event.severity();
        if promote {
            toast::fire(&self.app, &event);
            self.mark_toasted(&event).await;
        }
        let entry = ActivityEntry {
            id: Uuid::now_v7().to_string(),
            at: Utc::now(),
            event,
            severity,
            promoted: promote,
        };
        self.store.push(entry).await;
        if let Err(e) = self.app.emit(EVENT_ACTIVITY_CHANGED, ()) {
            tracing::warn!(error = %e, "announce: failed to emit activity-changed");
        }
    }
}

/// Spawn the periodic coalesce-window flush. Cheap — runs every
/// `STORAGE_WINDOW / 6` (5s) and only locks the inner state when
/// there are windows present.
pub fn spawn_flush_loop(announcer: Announcer) {
    let interval = STORAGE_WINDOW / 6;
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(interval).await;
            announcer.tick().await;
        }
    });
}

