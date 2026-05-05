//! Settings persistence and live-update bus.
//!
//! Mirrors [`crate::tending`]'s shape: file-backed, in-memory
//! `RwLock` for reads, `watch::Sender` for Rust-side subscribers,
//! Tauri event for the frontend.
//!
//! Path: `~/.zen-garden/.pavilion-settings.json` (XDG data dir on
//! Linux, home dir elsewhere — same resolution as `.tending` so
//! both stores live next to each other).

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter};
use tokio::sync::{watch, RwLock};

use super::types::{Settings, SettingsPatch};

/// Tauri event name emitted to the frontend whenever settings
/// change. Payload is the full new [`Settings`] snapshot.
pub const EVENT_SETTINGS_CHANGED: &str = "settings-changed";

#[derive(Clone)]
pub struct SettingsStore {
    inner: Arc<RwLock<Settings>>,
    tx: watch::Sender<Settings>,
    app: AppHandle,
}

impl SettingsStore {
    /// Construct from disk. The startup read is intentionally
    /// synchronous — settings load before the Announcer, which
    /// wants `Arc<SettingsStore>` at construction time, and
    /// blocking on a small JSON file at process start is cheaper
    /// than threading `block_on` calls through the setup path.
    pub fn new(app: AppHandle) -> Self {
        let (stored, existed) = read_file_sync();
        if existed {
            tracing::info!("settings: loaded from disk");
        } else {
            tracing::info!("settings: no existing file; starting from defaults");
        }
        let (tx, _) = watch::channel(stored.clone());
        Self {
            inner: Arc::new(RwLock::new(stored)),
            tx,
            app,
        }
    }

    /// Cheap clone of the current settings.
    pub async fn snapshot(&self) -> Settings {
        self.inner.read().await.clone()
    }

    /// Apply a partial update. Persists to disk, fans out via the
    /// watch channel, and emits the Tauri event for the frontend.
    pub async fn apply_patch(&self, patch: SettingsPatch) -> Settings {
        let new_settings = {
            let mut s = self.inner.write().await;
            s.apply(patch);
            s.clone()
        };
        if let Err(e) = write_file(&new_settings).await {
            tracing::warn!(error = %e, "settings: failed to persist");
        }
        let _ = self.tx.send(new_settings.clone());
        if let Err(e) = self.app.emit(EVENT_SETTINGS_CHANGED, &new_settings) {
            tracing::warn!(error = %e, "settings: failed to emit");
        }
        tracing::info!("settings: updated");
        new_settings
    }

    /// Subscribe to settings changes from Rust code (e.g. when the
    /// autostart toggle flips, the OS-side autostart plugin needs
    /// to flip too).
    #[allow(dead_code)]
    pub fn subscribe(&self) -> watch::Receiver<Settings> {
        self.tx.subscribe()
    }
}

// ── Path resolution ──────────────────────────────────────────────
// Same shape as `tending::zen_garden_dir` so both stores share a
// home. Duplicated rather than extracted: two callers isn't enough
// to justify a helper module yet.

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

fn settings_path() -> Option<PathBuf> {
    zen_garden_dir().map(|d| d.join(".pavilion-settings.json"))
}

/// Synchronous startup read. Returns the parsed settings (or
/// defaults) plus whether the file was already on disk so the
/// caller can log "loaded" vs "starting fresh".
fn read_file_sync() -> (Settings, bool) {
    let Some(path) = settings_path() else {
        return (Settings::default(), false);
    };
    if !path.exists() {
        return (Settings::default(), false);
    }
    match std::fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<Settings>(&content) {
            Ok(s) => (s, true),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    path = %path.display(),
                    "settings: file unparseable; using defaults"
                );
                (Settings::default(), true)
            }
        },
        Err(e) => {
            tracing::warn!(
                error = %e,
                path = %path.display(),
                "settings: file unreadable; using defaults"
            );
            (Settings::default(), true)
        }
    }
}

async fn write_file(settings: &Settings) -> std::io::Result<()> {
    let path =
        settings_path().ok_or_else(|| std::io::Error::other("could not resolve zen-garden dir"))?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let content = serde_json::to_string_pretty(settings).map_err(std::io::Error::other)?;
    tokio::fs::write(&path, content).await
}
