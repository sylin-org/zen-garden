//! Preferences domain — global field defaults + orchestrator
//! settings (ORCH-0030 §8).
//!
//! Preferences are global to the orchestrator instance (no per-caller
//! identity). The store has two namespaces:
//!
//! ## `fields` — payload field defaults
//!
//! Dotted field paths and their preferred values. Used for two
//! things:
//!
//! 1. **Catalog rendering** — when `GET /v1/catalog/{path}` renders
//!    a field list, field preferences are layered *over* the
//!    static default so clients see the operator's preferred
//!    values pre-filled.
//!
//! 2. **Dispatcher contextualization** — when a request reaches
//!    the contextualizer, field preferences are layered *under*
//!    the caller's explicit payload. Fields the caller omitted
//!    are filled from preferences before `recommended:*`
//!    defaulting.
//!
//! The layering order is:
//!
//! ```text
//! caller payload  >  field prefs  >  field static default  >  recommended:* (selectors only)
//! ```
//!
//! ## `settings` — orchestrator-wide flags
//!
//! Tunable behaviors that don't belong in any primitive's payload:
//! feature toggles, routing policies, cadence overrides. Examples:
//!
//! - `orchestrator.strict_fit` — ORCH-0038 VRAM fit filter (bool)
//!
//! Settings are **never** injected into payloads. They're consumed
//! by adapters and services that read them directly at the moment
//! they need the value. Changes publish `preferences.changed` on
//! the event bus just like field updates.
//!
//! ## Persistence
//!
//! Preferences are persisted to `{data_dir}/preferences.json` in
//! the namespaced shape:
//!
//! ```json
//! {
//!   "fields": {"image.width": 1024},
//!   "settings": {"orchestrator.strict_fit": false}
//! }
//! ```
//!
//! The legacy flat shape (`{"image.width": 1024}`) is still loaded
//! for backward compatibility and migrated to the namespaced shape
//! on the next save.

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
    /// Field defaults — dotted payload paths to values.
    /// Example: `{ "image.width": 1024, "text.sampling.temperature": 0.7 }`
    #[serde(default)]
    fields: HashMap<String, Value>,
    /// Orchestrator-wide settings — tunable flags that never
    /// enter a payload. Example: `{ "orchestrator.strict_fit": false }`
    #[serde(default)]
    settings: HashMap<String, Value>,
}

impl Preferences {
    /// Load preferences from disk, or create an empty store if the
    /// file doesn't exist. Handles both the new namespaced shape
    /// and the legacy flat shape (migrating the latter to `fields`
    /// on the next save).
    pub async fn load(data_dir: &Path, events: Arc<EventBus>) -> Arc<Self> {
        let path = data_dir.join("preferences.json");
        let state = if path.exists() {
            match tokio::fs::read_to_string(&path).await {
                Ok(content) => parse_preferences_file(&content),
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

    // ── Field defaults (payload injection) ────────────────────

    /// Get the full field-defaults map (cloned). Does **not**
    /// include settings.
    pub async fn get_all(&self) -> HashMap<String, Value> {
        self.state.read().await.fields.clone()
    }

    /// Get a single field default by dotted path.
    pub async fn get(&self, key: &str) -> Option<Value> {
        self.state.read().await.fields.get(key).cloned()
    }

    /// Merge new field defaults (partial update). Existing keys are
    /// overwritten; keys not in `updates` are preserved. Persists
    /// and publishes `preferences.changed`.
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

    /// Remove a single field default. Returns true if the key
    /// existed.
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

    // ── Settings (orchestrator-wide flags) ────────────────────

    /// Get the full settings map (cloned). Does **not** include
    /// field defaults.
    pub async fn get_all_settings(&self) -> HashMap<String, Value> {
        self.state.read().await.settings.clone()
    }

    /// Get a single setting by dotted key. Returns `None` if
    /// unset.
    pub async fn get_setting(&self, key: &str) -> Option<Value> {
        self.state.read().await.settings.get(key).cloned()
    }

    /// Get a single setting as a bool, falling back to `default`
    /// if unset or not a bool. Convenience for feature-flag
    /// consumers that want a clean no-branch call site.
    pub async fn get_setting_bool(&self, key: &str, default: bool) -> bool {
        self.state
            .read()
            .await
            .settings
            .get(key)
            .and_then(|v| v.as_bool())
            .unwrap_or(default)
    }

    /// Merge new settings (partial update).
    pub async fn merge_settings(&self, updates: HashMap<String, Value>) {
        {
            let mut state = self.state.write().await;
            for (k, v) in updates {
                state.settings.insert(k, v);
            }
        }
        self.persist().await;
        self.events.publish("preferences.changed", &serde_json::json!({}));
    }

    /// Remove a single setting. Returns true if the key existed.
    pub async fn remove_setting(&self, key: &str) -> bool {
        let removed = {
            let mut state = self.state.write().await;
            state.settings.remove(key).is_some()
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

/// Parse a preferences file, handling both the new namespaced
/// shape and the legacy flat shape.
///
/// - **New shape**: `{"fields": {...}, "settings": {...}}` — either
///   top-level key is present. Parsed directly into
///   [`PreferencesState`]. Missing namespaces default to empty.
/// - **Legacy shape**: a flat map of dotted paths to values. Every
///   entry is promoted to `fields`; `settings` starts empty. The
///   next `persist` writes the new shape.
///
/// Unparseable content logs and returns an empty state; better a
/// clean start than a panic on a malformed file.
fn parse_preferences_file(content: &str) -> PreferencesState {
    let parsed: Value = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "failed to parse preferences.json, starting empty");
            return PreferencesState::default();
        }
    };

    let Value::Object(map) = &parsed else {
        tracing::warn!("preferences.json root is not an object, starting empty");
        return PreferencesState::default();
    };

    // Discriminate shape by the presence of either namespace key
    // at the top level. A file with both "fields" and "settings"
    // absent is either empty or legacy.
    let is_new_shape = map.contains_key("fields") || map.contains_key("settings");
    if is_new_shape {
        match serde_json::from_value::<PreferencesState>(parsed.clone()) {
            Ok(state) => state,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "preferences.json has namespaced shape but failed to parse, starting empty"
                );
                PreferencesState::default()
            }
        }
    } else if map.is_empty() {
        PreferencesState::default()
    } else {
        // Legacy flat shape — promote entries to field defaults.
        tracing::info!(
            entries = map.len(),
            "preferences.json is in legacy flat shape; migrating to namespaced {{fields, settings}} on next save"
        );
        let fields: HashMap<String, Value> = map
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        PreferencesState {
            fields,
            settings: HashMap::new(),
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

        // Persisted to disk in the new namespaced shape.
        let content = tokio::fs::read_to_string(dir.path().join("preferences.json"))
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["fields"]["image.width"], 1024);
        assert_eq!(parsed["fields"]["text.sampling.temperature"], 0.7);
        assert!(parsed.get("settings").is_some());
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

    // ── Settings namespace (M3) ───────────────────────────────

    #[tokio::test]
    async fn settings_roundtrip_independent_of_fields() {
        let events = EventBus::new();
        let dir = tempfile::tempdir().unwrap();
        let prefs = Preferences::load(dir.path(), events).await;

        // Set a field default and a setting with similar-looking
        // keys to prove they don't collide.
        prefs
            .merge(HashMap::from([(
                "image.width".to_string(),
                serde_json::json!(1024),
            )]))
            .await;
        prefs
            .merge_settings(HashMap::from([(
                "orchestrator.strict_fit".to_string(),
                serde_json::json!(false),
            )]))
            .await;

        assert_eq!(prefs.get("image.width").await.unwrap(), 1024);
        assert_eq!(
            prefs.get_setting("orchestrator.strict_fit").await.unwrap(),
            false
        );
        assert!(prefs.get("orchestrator.strict_fit").await.is_none());
        assert!(prefs.get_setting("image.width").await.is_none());
    }

    #[tokio::test]
    async fn settings_do_not_leak_into_payloads() {
        let events = EventBus::new();
        let dir = tempfile::tempdir().unwrap();
        let prefs = Preferences::load(dir.path(), events).await;

        prefs
            .merge_settings(HashMap::from([(
                "orchestrator.strict_fit".to_string(),
                serde_json::json!(false),
            )]))
            .await;

        let mut payload = serde_json::json!({});
        prefs.apply_to_payload(&mut payload).await;

        // apply_to_payload only touches the fields namespace, so
        // settings never enter the payload. This is the whole
        // point of the namespace split.
        assert!(payload.as_object().unwrap().is_empty());
    }

    #[tokio::test]
    async fn get_setting_bool_convenience() {
        let events = EventBus::new();
        let dir = tempfile::tempdir().unwrap();
        let prefs = Preferences::load(dir.path(), events).await;

        // Unset → default wins
        assert!(prefs.get_setting_bool("orchestrator.strict_fit", true).await);
        assert!(!prefs.get_setting_bool("orchestrator.strict_fit", false).await);

        // Explicit true
        prefs
            .merge_settings(HashMap::from([(
                "orchestrator.strict_fit".to_string(),
                serde_json::json!(true),
            )]))
            .await;
        assert!(prefs.get_setting_bool("orchestrator.strict_fit", false).await);

        // Explicit false
        prefs
            .merge_settings(HashMap::from([(
                "orchestrator.strict_fit".to_string(),
                serde_json::json!(false),
            )]))
            .await;
        assert!(!prefs.get_setting_bool("orchestrator.strict_fit", true).await);

        // Non-bool value → fallback to default
        prefs
            .merge_settings(HashMap::from([(
                "orchestrator.weird".to_string(),
                serde_json::json!("not-a-bool"),
            )]))
            .await;
        assert!(prefs.get_setting_bool("orchestrator.weird", true).await);
    }

    #[tokio::test]
    async fn settings_remove() {
        let events = EventBus::new();
        let dir = tempfile::tempdir().unwrap();
        let prefs = Preferences::load(dir.path(), events).await;

        prefs
            .merge_settings(HashMap::from([(
                "key".to_string(),
                serde_json::json!("value"),
            )]))
            .await;
        assert!(prefs.remove_setting("key").await);
        assert!(prefs.get_setting("key").await.is_none());
        assert!(!prefs.remove_setting("nonexistent").await);
    }

    // ── Legacy-shape migration ────────────────────────────────

    #[tokio::test]
    async fn load_legacy_flat_shape_migrates_to_fields() {
        let events = EventBus::new();
        let dir = tempfile::tempdir().unwrap();

        // Write a legacy flat file manually.
        let legacy = serde_json::json!({
            "image.width": 1024,
            "text.sampling.temperature": 0.7
        });
        tokio::fs::write(
            dir.path().join("preferences.json"),
            serde_json::to_string_pretty(&legacy).unwrap(),
        )
        .await
        .unwrap();

        let prefs = Preferences::load(dir.path(), events).await;

        // Legacy keys land in fields.
        assert_eq!(prefs.get("image.width").await.unwrap(), 1024);
        assert_eq!(prefs.get("text.sampling.temperature").await.unwrap(), 0.7);
        // Settings start empty.
        assert!(prefs.get_all_settings().await.is_empty());

        // Triggering a save (via merge) rewrites in the new shape.
        prefs
            .merge_settings(HashMap::from([(
                "orchestrator.strict_fit".to_string(),
                serde_json::json!(false),
            )]))
            .await;
        let content = tokio::fs::read_to_string(dir.path().join("preferences.json"))
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["fields"]["image.width"], 1024);
        assert_eq!(parsed["settings"]["orchestrator.strict_fit"], false);
    }

    #[tokio::test]
    async fn load_new_shape_round_trips() {
        let events = EventBus::new();
        let dir = tempfile::tempdir().unwrap();

        let new_shape = serde_json::json!({
            "fields": {"image.width": 2048},
            "settings": {"orchestrator.strict_fit": true}
        });
        tokio::fs::write(
            dir.path().join("preferences.json"),
            serde_json::to_string_pretty(&new_shape).unwrap(),
        )
        .await
        .unwrap();

        let prefs = Preferences::load(dir.path(), events).await;
        assert_eq!(prefs.get("image.width").await.unwrap(), 2048);
        assert_eq!(
            prefs.get_setting("orchestrator.strict_fit").await.unwrap(),
            true
        );
    }

    #[tokio::test]
    async fn load_empty_file_starts_empty() {
        let events = EventBus::new();
        let dir = tempfile::tempdir().unwrap();

        tokio::fs::write(dir.path().join("preferences.json"), "{}")
            .await
            .unwrap();
        let prefs = Preferences::load(dir.path(), events).await;
        assert!(prefs.get_all().await.is_empty());
        assert!(prefs.get_all_settings().await.is_empty());
    }
}
