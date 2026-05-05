//! Stone tending — Pavilion's anchor stone for API operations.
//!
//! Mirrors Rake's [RAKE-0010](../../docs/decisions/RAKE-0010-caching.md)
//! tending model. Pavilion picks one stone as its current anchor; that
//! stone is the recipient of all API calls (services list, pond status,
//! storage browse, etc.). Topology awareness ([crate::awareness]) is
//! orthogonal — it tracks who's chirping; tending tracks who we *talk
//! to*.
//!
//! # File format
//!
//! Persisted to `~/.zen-garden/.tending` — the **same file Rake uses**.
//! When Pavilion writes, Rake's next invocation sees the new tending;
//! when Rake writes, Pavilion picks it up on next start. The struct
//! shape mirrors Rake's `TendingState` so the JSON is interchangeable.
//!
//! # Auto-tend logic
//!
//! On startup:
//! 1. Read existing `.tending` file. If present and the named stone
//!    appears in awareness within a short window: keep it.
//! 2. Otherwise, prefer a stone running on localhost (Pavilion is
//!    authorised to check local first).
//! 3. Otherwise, tend the first stone observed via chirp.
//!
//! The user can override at any time via the `set_tended` command.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::sync::{watch, RwLock};

use crate::awareness::{AwareStone, Awareness};
use crate::settings::SettingsStore;

/// Tauri event name for tending changes.
pub const EVENT_TENDING_CHANGED: &str = "tending-changed";

/// On-disk format. Field-compatible with `garden_rake::tending::TendingState`
/// so the file is shared between Rake and Pavilion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TendingState {
    pub stone_name: String,
    pub endpoint: String,
    #[serde(with = "iso8601")]
    pub last_seen: SystemTime,
    /// Hardware capabilities cached at tend-time. Pavilion treats this
    /// as opaque — preserves Rake's data on round-trip without parsing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<serde_json::Value>,
}

/// Slim shape sent to the React frontend (capabilities omitted).
#[derive(Debug, Clone, Serialize)]
pub struct TendedStone {
    pub stone_name: String,
    pub endpoint: String,
}

impl From<&TendingState> for TendedStone {
    fn from(s: &TendingState) -> Self {
        TendedStone {
            stone_name: s.stone_name.clone(),
            endpoint: s.endpoint.clone(),
        }
    }
}

pub struct Tending {
    current: Arc<RwLock<Option<TendingState>>>,
    /// Watch channel mirroring `current` so Rust-side subscribers
    /// (the storage-observer supervisor) can rebind on tending
    /// changes without polling the file or the Tauri event bus.
    tx: watch::Sender<Option<TendedStone>>,
    app: AppHandle,
}

impl Tending {
    pub async fn new(app: AppHandle) -> Self {
        let stored = read_file().await;
        if let Some(s) = &stored {
            tracing::info!(stone = %s.stone_name, "tending: loaded from disk");
        } else {
            tracing::info!("tending: no existing .tending file");
        }
        let initial = stored.as_ref().map(TendedStone::from);
        let (tx, _) = watch::channel(initial);
        Self {
            current: Arc::new(RwLock::new(stored)),
            tx,
            app,
        }
    }

    pub async fn current(&self) -> Option<TendedStone> {
        self.current.read().await.as_ref().map(TendedStone::from)
    }

    /// Subscribe to tending changes — the receiver yields a fresh
    /// snapshot on every `set` call (and one snapshot of the initial
    /// state on first read).
    pub fn subscribe(&self) -> watch::Receiver<Option<TendedStone>> {
        self.tx.subscribe()
    }

    /// Replace the tended stone. Persists to disk and emits the event.
    pub async fn set(&self, stone: &AwareStone) {
        let state = TendingState {
            stone_name: stone.stone_name.clone(),
            endpoint: stone.endpoint.clone(),
            last_seen: SystemTime::now(),
            capabilities: None,
        };

        {
            let mut current = self.current.write().await;
            *current = Some(state.clone());
        }

        if let Err(e) = write_file(&state).await {
            tracing::warn!(error = %e, "tending: failed to persist");
        } else {
            tracing::info!(stone = %state.stone_name, "tending: anchored");
        }

        let payload = TendedStone::from(&state);
        // Notify Rust-side subscribers (observer supervisor) and the
        // frontend in parallel — both must see the new tending.
        let _ = self.tx.send(Some(payload.clone()));
        if let Err(e) = self.app.emit(EVENT_TENDING_CHANGED, &payload) {
            tracing::warn!(error = %e, "tending: failed to emit");
        }
    }

    /// Run the auto-tend strategy in a background task.
    ///
    /// Strategy: localhost preferred, then **first-by-response**. If no
    /// stones are present, sit and wait indefinitely — Pavilion already
    /// listens to chirps and provoked one DISCOVERY_REQUEST at startup,
    /// so any stone that comes online (or is silently present and
    /// pre-existing) will eventually surface.
    ///
    /// Existing-tending grace: if a `~/.zen-garden/.tending` was loaded,
    /// give it a brief grace window (10s) to chirp before reselecting.
    /// If it never re-appears, fall through to fresh selection — the
    /// user can re-tend explicitly later.
    ///
    /// **Onboarding gate**: when `settings.onboarded` is false (a fresh
    /// installation), auto-tend stays asleep until the user has made an
    /// explicit choice via the onboarding view. Otherwise we'd race the
    /// UI — the user would open Pavilion, see "tending stone-X" before
    /// they ever read the onboarding prompt, and lose the deliberate
    /// first-pick the spec calls for.
    pub fn spawn_auto_tend(
        self: Arc<Self>,
        awareness: Arc<Awareness>,
        settings: Arc<SettingsStore>,
    ) {
        tauri::async_runtime::spawn(async move {
            // Wait until onboarding is complete (either by an explicit
            // tend, which sets tending and lets this loop exit on its
            // first iteration, or by Skip, which leaves tending None
            // and lets the fresh-selection branch fire).
            wait_until_onboarded(&settings).await;

            let already = self.current.read().await.clone();

            // Phase 1: existing-tending grace.
            if let Some(existing) = &already {
                let interval = Duration::from_millis(500);
                let total = Duration::from_secs(10);
                let mut waited = Duration::ZERO;
                while waited < total {
                    let snap = awareness.snapshot().await;
                    if snap.iter().any(|s| {
                        s.stone_name == existing.stone_name || s.endpoint == existing.endpoint
                    }) {
                        tracing::info!(
                            stone = %existing.stone_name,
                            "tending: existing stone confirmed in awareness"
                        );
                        return;
                    }
                    tokio::time::sleep(interval).await;
                    waited += interval;
                }
                tracing::info!(
                    stone = %existing.stone_name,
                    "tending: existing stone silent; falling through to first-by-response selection"
                );
            }

            // Phase 2: fresh selection — sit and wait indefinitely.
            // No timeout. The cache is empty until a chirp or
            // discovery-response arrives; once anything appears we pick
            // localhost-first, then first-by-response.
            let interval = Duration::from_millis(500);
            loop {
                let snap = awareness.snapshot().await;
                if let Some(picked) = pick_stone(&snap) {
                    self.set(picked).await;
                    return;
                }
                tokio::time::sleep(interval).await;
            }
        });
    }
}

/// Auto-tend selection: localhost first, then first-by-response.
///
/// `Awareness::snapshot` returns stones sorted by `first_seen_at`
/// ascending (oldest first), so `snap.first()` after the localhost
/// filter gives us "first-by-response."
fn pick_stone(snap: &[AwareStone]) -> Option<&AwareStone> {
    snap.iter()
        .find(|s| is_localhost(&s.endpoint))
        .or_else(|| snap.first())
}

fn is_localhost(endpoint: &str) -> bool {
    endpoint.contains("127.0.0.1")
        || endpoint.contains("//localhost")
        || endpoint.contains("[::1]")
}

// ── Path resolution ──────────────────────────────────────────────────

/// Match Rake's resolution: XDG data dir on Linux, `~/.zen-garden` on
/// other platforms. See `src/rake/src/tending.rs` for the canonical
/// implementation.
fn zen_garden_dir() -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    if let Some(xdg) = dirs::data_dir() {
        let p = xdg.join("zen-garden");
        if std::fs::create_dir_all(&p).is_ok() {
            return Some(p);
        }
    }
    if let Some(home) = dirs::home_dir() {
        let p = home.join(".zen-garden");
        if std::fs::create_dir_all(&p).is_ok() {
            return Some(p);
        }
    }
    None
}

fn tending_path() -> Option<PathBuf> {
    zen_garden_dir().map(|d| d.join(".tending"))
}

async fn read_file() -> Option<TendingState> {
    let path = tending_path()?;
    let content = tokio::fs::read_to_string(&path).await.ok()?;
    match serde_json::from_str::<TendingState>(&content) {
        Ok(s) => Some(s),
        Err(e) => {
            tracing::warn!(error = %e, path = %path.display(), "tending: file unparseable");
            None
        }
    }
}

async fn write_file(state: &TendingState) -> std::io::Result<()> {
    let path = tending_path()
        .ok_or_else(|| std::io::Error::other("could not resolve zen-garden config dir"))?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let content = serde_json::to_string_pretty(state)
        .map_err(std::io::Error::other)?;
    tokio::fs::write(&path, content).await
}

// ── ISO-8601 SystemTime serde — matches Rake's format ──────────────

mod iso8601 {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::{SystemTime, UNIX_EPOCH};

    pub fn serialize<S: Serializer>(time: &SystemTime, s: S) -> Result<S::Ok, S::Error> {
        let secs = time
            .duration_since(UNIX_EPOCH)
            .map_err(serde::ser::Error::custom)?
            .as_secs();
        let iso = chrono::DateTime::from_timestamp(secs as i64, 0)
            .ok_or_else(|| serde::ser::Error::custom("invalid timestamp"))?
            .to_rfc3339();
        iso.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<SystemTime, D::Error> {
        let s = String::deserialize(d)?;
        let dt = chrono::DateTime::parse_from_rfc3339(&s).map_err(serde::de::Error::custom)?;
        Ok(UNIX_EPOCH + std::time::Duration::from_secs(dt.timestamp() as u64))
    }
}

/// Block until `settings.onboarded` is `true`. Cheap snapshot first
/// (covers warm starts), then subscribe to the settings watch
/// channel and await transitions. Yields without burning CPU.
async fn wait_until_onboarded(settings: &SettingsStore) {
    if settings.snapshot().await.onboarded {
        return;
    }
    tracing::info!("tending: auto-tend paused until onboarding completes");
    let mut rx = settings.subscribe();
    loop {
        if rx.changed().await.is_err() {
            tracing::warn!("tending: settings channel closed before onboarding completed");
            return;
        }
        if rx.borrow_and_update().onboarded {
            tracing::info!("tending: onboarding complete, auto-tend resuming");
            return;
        }
    }
}
