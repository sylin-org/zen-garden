//! Identity cache — `device_id` → adapter-registration-name hint.
//!
//! The cache is a **hint, not a bypass** (COMPANION-0012). On attach
//! the bus looks up `device_id`; a cached registration is tried first
//! (predicate still evaluated). A positive match short-circuits the
//! full ordered dance. A negative match invalidates the entry.
//!
//! Persisted to `{state_dir}/device-bus-cache.json` so the binding
//! survives daemon restarts.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
struct CacheSnapshot {
    entries: HashMap<String, String>,
}

/// Cache of `device_id` → `registration_name` bindings.
pub struct DeviceCache {
    path: Option<PathBuf>,
    inner: Mutex<CacheSnapshot>,
}

impl DeviceCache {
    /// In-memory cache with no persistence. Used by tests and by
    /// `DeviceBus` instances that don't configure a state dir.
    pub fn memory() -> Self {
        Self {
            path: None,
            inner: Mutex::new(CacheSnapshot::default()),
        }
    }

    /// Cache persisted to `path`. Loads existing contents if the file
    /// is present and parseable. A corrupt file is logged and ignored
    /// (cache starts empty); consequence is one extra probe-and-dance
    /// per device until the cache rebuilds.
    pub fn load(path: PathBuf) -> Self {
        let inner = match std::fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str::<CacheSnapshot>(&raw) {
                Ok(snap) => snap,
                Err(e) => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "device cache parse failed; starting empty"
                    );
                    CacheSnapshot::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => CacheSnapshot::default(),
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "device cache read failed; starting empty"
                );
                CacheSnapshot::default()
            }
        };
        Self {
            path: Some(path),
            inner: Mutex::new(inner),
        }
    }

    /// Look up the cached registration name for `device_id`. Returns
    /// `None` if unbound.
    pub fn lookup(&self, device_id: &str) -> Option<String> {
        self.inner.lock().unwrap().entries.get(device_id).cloned()
    }

    /// Bind `device_id` to `registration_name`. Persists if a path is
    /// configured; persistence failure is logged and dropped (in-memory
    /// state is always updated).
    pub fn insert(&self, device_id: impl Into<String>, registration_name: impl Into<String>) {
        let device_id = device_id.into();
        let name = registration_name.into();
        let snapshot = {
            let mut inner = self.inner.lock().unwrap();
            inner.entries.insert(device_id, name);
            inner.entries.clone()
        };
        self.persist(CacheSnapshot { entries: snapshot });
    }

    /// Drop the binding for `device_id` (e.g. after a predicate
    /// mismatch when the cached registration no longer claims).
    pub fn invalidate(&self, device_id: &str) {
        let snapshot = {
            let mut inner = self.inner.lock().unwrap();
            inner.entries.remove(device_id);
            inner.entries.clone()
        };
        self.persist(CacheSnapshot { entries: snapshot });
    }

    /// Number of active bindings. For test assertions and telemetry.
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn persist(&self, snapshot: CacheSnapshot) {
        let Some(ref path) = self.path else {
            return;
        };
        let Ok(raw) = serde_json::to_string_pretty(&snapshot) else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = atomic_write(path, raw.as_bytes()) {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "device cache persist failed"
            );
        }
    }
}

/// Write-then-rename to avoid torn files on crash or power loss.
fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut tmp = path.to_path_buf();
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("device-bus-cache.json");
    tmp.set_file_name(format!(".{name}.tmp"));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn memory_cache_roundtrips_bindings() {
        let c = DeviceCache::memory();
        assert!(c.is_empty());
        c.insert("01938abc-de01-7234-89ab-cdef01234567", "firefly.oled-v2");
        assert_eq!(
            c.lookup("01938abc-de01-7234-89ab-cdef01234567"),
            Some("firefly.oled-v2".to_string())
        );
        assert_eq!(c.len(), 1);
        c.invalidate("01938abc-de01-7234-89ab-cdef01234567");
        assert!(c.is_empty());
    }

    #[test]
    fn persisted_cache_survives_reload() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("device-bus-cache.json");
        {
            let c = DeviceCache::load(path.clone());
            c.insert("id-a", "firefly.matrix");
            c.insert("id-b", "firefly.oled-v1");
        }
        let c = DeviceCache::load(path);
        assert_eq!(c.len(), 2);
        assert_eq!(c.lookup("id-a"), Some("firefly.matrix".to_string()));
        assert_eq!(c.lookup("id-b"), Some("firefly.oled-v1".to_string()));
    }

    #[test]
    fn missing_file_starts_empty() {
        let dir = TempDir::new().unwrap();
        let c = DeviceCache::load(dir.path().join("nonexistent.json"));
        assert!(c.is_empty());
    }

    #[test]
    fn corrupt_file_starts_empty_without_panic() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("device-bus-cache.json");
        std::fs::write(&path, "not json").unwrap();
        let c = DeviceCache::load(path);
        assert!(c.is_empty());
    }

    #[test]
    fn invalidate_is_idempotent() {
        let c = DeviceCache::memory();
        c.invalidate("no-such-key");
        c.insert("k", "r");
        c.invalidate("k");
        c.invalidate("k");
        assert!(c.is_empty());
    }
}
