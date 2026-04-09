//! Preferences domain — global field-path-to-value map (ORCH-0030 §8).
//!
//! Preferences are global to the orchestrator instance (no per-caller
//! identity). They serve two purposes:
//!
//! 1. **Catalog rendering** — when `GET /v1/catalog/{path}` renders a
//!    field list, preferences are layered *over* the field's static
//!    default so clients see the operator's preferred values pre-filled.
//!
//! 2. **Dispatcher contextualization** — when a request reaches the
//!    contextualizer, preferences are layered *under* the caller's
//!    explicit payload. Fields the caller omitted are filled from
//!    preferences before `recommended:*` defaulting.
//!
//! The layering order is:
//!
//! ```text
//! caller payload  >  preferences  >  field static default  >  recommended:* (selectors only)
//! ```
//!
//! Preferences are persisted to `{data_dir}/preferences.json` and
//! loaded on startup. Changes publish `preferences.changed` on the
//! event bus so the catalog builder can rebuild.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::RwLock;

use crate::domain::events::EventBus;

/// The preferences store. Thread-safe, persisted to disk on mutation.
pub struct Preferences {
    state: RwLock<PreferencesState>,
    path: PathBuf,
    events: Arc<EventBus>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PreferencesState {
    /// Flat map of dotted field paths to values.
    /// Example: `{ "image.width": 1024, "text.sampling.temperature": 0.7 }`
    #[serde(flatten)]
    fields: HashMap<String, Value>,
}

impl Preferences {
    /// Load preferences from disk, or create an empty store if the
    /// file doesn't exist.
    pub async fn load(data_dir: &Path, events: Arc<EventBus>) -> Arc<Self> {
        let path = data_dir.join("preferences.json");
        let state = if path.exists() {
            match tokio::fs::read_to_string(&path).await {
                Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                Err(e) => {
                    tracing::warn!(error = %e, "failed to read preferences.json, starting empty");
                    PreferencesState::default()
                }
            }
        } else {
            PreferencesState::default()
        };

        Arc::new(Self {
            state: RwLock::new(state),
            path,
            events,
        })
    }

    /// Get the full preferences map (cloned).
    pub async fn get_all(&self) -> HashMap<String, Value> {
        self.state.read().await.fields.clone()
    }

    /// Get a single preference by dotted field path.
    pub async fn get(&self, key: &str) -> Option<Value> {
        self.state.read().await.fields.get(key).cloned()
    }

    /// Merge new values into the preferences (partial update).
    /// Existing keys are overwritten; keys not in `updates` are
    /// preserved. Persists to disk and publishes `preferences.changed`.
    pub async fn merge(&self, updates: HashMap<String, Value>) {
        {
            let mut state = self.state.write().await;
            for (k, v) in updates {
                state.fields.insert(k, v);
            }
        }
        self.persist().await;
        self.events.publish("preferences.changed", &serde_json::json!({}));
    }

    /// Remove a single key. Returns true if the key existed.
    pub async fn remove(&self, key: &str) -> bool {
        let removed = {
            let mut state = self.state.write().await;
            state.fields.remove(key).is_some()
        };
        if removed {
            self.persist().await;
            self.events.publish("preferences.changed", &serde_json::json!({}));
        }
        removed
    }

    /// Apply preferences to a payload: for every field in preferences
    /// that is NOT already present in the payload, inject the
    /// preference value. This is the "preferences layer under caller
    /// payload" step.
    pub async fn apply_to_payload(&self, payload: &mut Value) {
        let state = self.state.read().await;
        if state.fields.is_empty() {
            return;
        }

        // Collect which keys need injection (check with immutable borrow).
        let to_inject: Vec<(String, Value)> = state
            .fields
            .iter()
            .filter(|(dotted_key, _)| {
                let pointer = format!("/{}", dotted_key.replace('.', "/"));
                payload.pointer(&pointer).is_none()
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        // Now mutate (separate borrow scope).
        if let Some(obj) = payload.as_object_mut() {
            for (dotted_key, pref_value) in to_inject {
                set_nested(obj, &dotted_key, pref_value);
            }
        }
    }

    async fn persist(&self) {
        let state = self.state.read().await;
        let json = match serde_json::to_string_pretty(&*state) {
            Ok(j) => j,
            Err(e) => {
                tracing::error!(error = %e, "failed to serialize preferences");
                return;
            }
        };
        if let Some(parent) = self.path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        if let Err(e) = tokio::fs::write(&self.path, json).await {
            tracing::error!(error = %e, path = %self.path.display(), "failed to persist preferences");
        }
    }
}

/// Set a dotted key in a JSON object, creating intermediate objects
/// as needed. `"image.width"` with value `1024` produces
/// `{"image": {"width": 1024}}`.
fn set_nested(obj: &mut serde_json::Map<String, Value>, dotted: &str, value: Value) {
    let parts: Vec<&str> = dotted.split('.').collect();
    if parts.is_empty() {
        return;
    }
    if parts.len() == 1 {
        obj.entry(parts[0]).or_insert(value);
        return;
    }

    let first = parts[0];
    let rest = parts[1..].join(".");
    let child = obj
        .entry(first)
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if let Some(child_obj) = child.as_object_mut() {
        set_nested(child_obj, &rest, value);
    }
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_nested_single_level() {
        let mut obj = serde_json::Map::new();
        set_nested(&mut obj, "width", serde_json::json!(1024));
        assert_eq!(obj["width"], 1024);
    }

    #[test]
    fn set_nested_multi_level() {
        let mut obj = serde_json::Map::new();
        set_nested(&mut obj, "image.width", serde_json::json!(1024));
        assert_eq!(obj["image"]["width"], 1024);
    }

    #[test]
    fn set_nested_deep() {
        let mut obj = serde_json::Map::new();
        set_nested(&mut obj, "text.sampling.temperature", serde_json::json!(0.7));
        assert_eq!(obj["text"]["sampling"]["temperature"], 0.7);
    }

    #[test]
    fn set_nested_does_not_overwrite_existing() {
        let mut obj = serde_json::Map::new();
        obj.insert("width".to_string(), serde_json::json!(512));
        set_nested(&mut obj, "width", serde_json::json!(1024));
        // or_insert should NOT overwrite
        assert_eq!(obj["width"], 512);
    }

    #[tokio::test]
    async fn preferences_roundtrip() {
        let events = EventBus::new();
        let dir = tempfile::tempdir().unwrap();
        let prefs = Preferences::load(dir.path(), events).await;

        assert!(prefs.get_all().await.is_empty());

        let mut updates = HashMap::new();
        updates.insert("image.width".into(), serde_json::json!(1024));
        updates.insert("text.sampling.temperature".into(), serde_json::json!(0.7));
        prefs.merge(updates).await;

        let all = prefs.get_all().await;
        assert_eq!(all.len(), 2);
        assert_eq!(all["image.width"], 1024);
        assert_eq!(all["text.sampling.temperature"], 0.7);

        // Persisted to disk
        let content = tokio::fs::read_to_string(dir.path().join("preferences.json"))
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["image.width"], 1024);
    }

    #[tokio::test]
    async fn preferences_apply_to_payload() {
        let events = EventBus::new();
        let dir = tempfile::tempdir().unwrap();
        let prefs = Preferences::load(dir.path(), events).await;

        let mut updates = HashMap::new();
        updates.insert("image.width".into(), serde_json::json!(1024));
        updates.insert("text.sampling.temperature".into(), serde_json::json!(0.7));
        prefs.merge(updates).await;

        // Caller provides temperature but not width
        let mut payload = serde_json::json!({
            "text": { "sampling": { "temperature": 1.2 } }
        });
        prefs.apply_to_payload(&mut payload).await;

        // Temperature was set by caller → preserved
        assert_eq!(payload["text"]["sampling"]["temperature"], 1.2);
        // Width was not set → injected from preferences
        assert_eq!(payload["image"]["width"], 1024);
    }

    #[tokio::test]
    async fn preferences_remove() {
        let events = EventBus::new();
        let dir = tempfile::tempdir().unwrap();
        let prefs = Preferences::load(dir.path(), events).await;

        let mut updates = HashMap::new();
        updates.insert("key".into(), serde_json::json!("value"));
        prefs.merge(updates).await;

        assert!(prefs.remove("key").await);
        assert!(prefs.get("key").await.is_none());
        assert!(!prefs.remove("nonexistent").await);
    }
}
