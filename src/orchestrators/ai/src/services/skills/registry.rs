//! `Skills` aggregate (ORCH-0029).
//!
//! Holds the dynamic per-skill state — metadata pushed by skill-aware
//! adapters at load time, per-instance readiness updated by the
//! provisioning worker, and AI-naming updates emitted asynchronously
//! by the namer. Per ORCH-0028 §6, mutable state is private behind a
//! `tokio::sync::Mutex`; readers see immutable snapshots via
//! `watch::channel`. Per ORCH-0028 §13, lifecycle events are exposed
//! via a typed `broadcast::Sender<SkillEvent>`.
//!
//! The Directory carries the static schema (Registration entries
//! published by providers); this aggregate carries everything else.
//! Catalog requests join `directory.snapshot()` × `skills.snapshot()`
//! at read time.

use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::{broadcast, watch, Mutex};

use crate::domain::ids::ProviderName;
use crate::domain::moniker::Moniker;
use crate::domain::primitive::Primitive;

use super::types::{ImportSource, ModelRef, ModelSelector, Variant};

/// Composite key for a skill — `(provider, moniker)`. Two providers
/// may register skills with the same moniker; the provider half
/// disambiguates.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize)]
pub struct SkillKey {
    pub provider: ProviderName,
    pub moniker: Moniker,
}

impl SkillKey {
    pub fn new(provider: ProviderName, moniker: Moniker) -> Self {
        Self { provider, moniker }
    }
}

impl std::fmt::Display for SkillKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}::{}", self.provider, self.moniker.as_str())
    }
}

/// Static metadata pushed by the adapter at load time.
///
/// Mirrors what the adapter declares in its Directory `Registration`,
/// plus the skill-meta fields the Registration doesn't carry
/// (variants, model_selector, required_models, source, preview_url).
/// The split exists because the Directory aggregates registrations by
/// `(provider, primitive)` for fast catalog rendering, while the
/// Skills aggregate keys by `(provider, moniker)` and carries
/// per-skill provisioning state.
#[derive(Debug, Clone, Serialize)]
pub struct SkillMeta {
    pub provider: ProviderName,
    pub moniker: Moniker,
    pub primitive: Primitive,
    pub display_name: String,
    pub description: String,
    pub vram_mb: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variants: Option<Vec<Variant>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_selector: Option<ModelSelector>,
    pub required_models: Vec<ModelRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<ImportSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_url: Option<String>,
}

/// Per-instance readiness for one skill on one ComfyUI instance.
#[derive(Debug, Clone, Serialize)]
pub struct InstanceReadiness {
    pub stone_name: String,
    pub endpoint: String,
    pub ready: bool,
    pub reason: String,
    pub vram_mb: u64,
}

/// Composite view: static metadata + dynamic per-instance state.
#[derive(Debug, Clone, Serialize)]
pub struct SkillEntry {
    pub meta: SkillMeta,
    pub readiness: HashMap<String, InstanceReadiness>,
}

/// Immutable snapshot — what readers see.
///
/// `skills` is keyed by `SkillKey` for O(1) lookup but serializes as
/// a flat list because JSON object keys must be strings (and the
/// entry already carries the provider + moniker inside `meta`).
#[derive(Debug, Clone, Default)]
pub struct SkillsSnapshot {
    pub version: u64,
    pub skills: Arc<HashMap<SkillKey, SkillEntry>>,
}

impl Serialize for SkillsSnapshot {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("SkillsSnapshot", 2)?;
        state.serialize_field("version", &self.version)?;
        let entries: Vec<&SkillEntry> = self.skills.values().collect();
        state.serialize_field("skills", &entries)?;
        state.end()
    }
}

/// Lifecycle event emitted on every state change.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SkillEvent {
    Registered {
        key: SkillKey,
    },
    Unregistered {
        key: SkillKey,
    },
    ReadinessChanged {
        key: SkillKey,
        endpoint: String,
        ready: bool,
    },
    Named {
        key: SkillKey,
        display_name: String,
        description: String,
    },
}

/// The Skills aggregate — single source of truth for dynamic per-
/// skill state. ORCH-0028 §6 compliant: private mutable state behind
/// a Mutex, snapshot published via watch.
pub struct Skills {
    state: Mutex<SkillsState>,
    publisher: watch::Sender<Arc<SkillsSnapshot>>,
    events: broadcast::Sender<SkillEvent>,
}

#[derive(Default)]
struct SkillsState {
    version: u64,
    skills: HashMap<SkillKey, SkillEntry>,
}

impl Skills {
    pub fn new() -> Arc<Self> {
        let initial = Arc::new(SkillsSnapshot::default());
        let (publisher, _) = watch::channel(initial);
        let (events, _) = broadcast::channel(64);
        Arc::new(Self {
            state: Mutex::new(SkillsState::default()),
            publisher,
            events,
        })
    }

    /// Read-only snapshot for catalog rendering.
    pub fn snapshot(&self) -> Arc<SkillsSnapshot> {
        self.publisher.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<Arc<SkillsSnapshot>> {
        self.publisher.subscribe()
    }

    /// Subscribe to lifecycle events.
    pub fn skill_stream(&self) -> broadcast::Receiver<SkillEvent> {
        self.events.subscribe()
    }

    /// Adapter pushes a skill (or replaces existing metadata).
    /// Idempotent — calling twice with the same key replaces the
    /// metadata while preserving any per-instance readiness.
    pub async fn register(&self, meta: SkillMeta) {
        let key = SkillKey {
            provider: meta.provider.clone(),
            moniker: meta.moniker.clone(),
        };
        let mut state = self.state.lock().await;
        let entry = state
            .skills
            .entry(key.clone())
            .and_modify(|existing| existing.meta = meta.clone())
            .or_insert_with(|| SkillEntry {
                meta: meta.clone(),
                readiness: HashMap::new(),
            });
        // Whether new or updated, this is a publish-worthy change.
        let _ = entry;
        state.version += 1;
        self.publish(&state);
        drop(state);

        let _ = self.events.send(SkillEvent::Registered { key });
    }

    /// Adapter removes a skill (file deleted on disk, hot-reload
    /// detected the absence). Drops both the metadata and any
    /// per-instance readiness.
    pub async fn unregister(&self, key: &SkillKey) {
        let mut state = self.state.lock().await;
        if state.skills.remove(key).is_some() {
            state.version += 1;
            self.publish(&state);
            drop(state);
            let _ = self.events.send(SkillEvent::Unregistered { key: key.clone() });
        }
    }

    /// Provisioning worker (or discovery probe) updates per-instance
    /// readiness for a skill on a specific instance. Skills not yet
    /// registered are ignored — registration must come first.
    pub async fn set_readiness(&self, key: &SkillKey, readiness: InstanceReadiness) {
        let mut state = self.state.lock().await;
        let Some(entry) = state.skills.get_mut(key) else {
            return;
        };
        let endpoint = readiness.endpoint.clone();
        let ready = readiness.ready;
        entry.readiness.insert(endpoint.clone(), readiness);
        state.version += 1;
        self.publish(&state);
        drop(state);
        let _ = self.events.send(SkillEvent::ReadinessChanged {
            key: key.clone(),
            endpoint,
            ready,
        });
    }

    /// AI namer pushes an updated display name and description.
    pub async fn rename(&self, key: &SkillKey, display_name: String, description: String) {
        let mut state = self.state.lock().await;
        let Some(entry) = state.skills.get_mut(key) else {
            return;
        };
        entry.meta.display_name = display_name.clone();
        entry.meta.description = description.clone();
        state.version += 1;
        self.publish(&state);
        drop(state);
        let _ = self.events.send(SkillEvent::Named {
            key: key.clone(),
            display_name,
            description,
        });
    }

    /// Re-publish the snapshot from the locked state.
    fn publish(&self, state: &SkillsState) {
        let snapshot = Arc::new(SkillsSnapshot {
            version: state.version,
            skills: Arc::new(state.skills.clone()),
        });
        // `send_replace` always updates the stored value, even when
        // there are no receivers — the prior `send` had a known
        // silent-no-op race when called before subscribers existed.
        self.publisher.send_replace(snapshot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::primitive::Primitive;

    fn meta(provider: &str, moniker: &str) -> SkillMeta {
        SkillMeta {
            provider: ProviderName::new(provider),
            moniker: Moniker::new(moniker).unwrap(),
            primitive: Primitive::ImageGenerate,
            display_name: "Test Skill".into(),
            description: "test".into(),
            vram_mb: 1024,
            variants: None,
            model_selector: None,
            required_models: Vec::new(),
            source: None,
            preview_url: None,
        }
    }

    fn readiness(endpoint: &str, ready: bool) -> InstanceReadiness {
        InstanceReadiness {
            stone_name: "stone-test".into(),
            endpoint: endpoint.into(),
            ready,
            reason: if ready { "all models present".into() } else { "missing models".into() },
            vram_mb: 8192,
        }
    }

    #[tokio::test]
    async fn register_then_snapshot_observes_skill() {
        let skills = Skills::new();
        skills.register(meta("comfyui", "test-skill")).await;
        let snap = skills.snapshot();
        assert_eq!(snap.version, 1);
        assert_eq!(snap.skills.len(), 1);
        let key = SkillKey {
            provider: ProviderName::new("comfyui"),
            moniker: Moniker::new("test-skill").unwrap(),
        };
        let entry = snap.skills.get(&key).expect("entry present");
        assert_eq!(entry.meta.display_name, "Test Skill");
        assert!(entry.readiness.is_empty());
    }

    #[tokio::test]
    async fn unregister_drops_skill_and_bumps_version() {
        let skills = Skills::new();
        skills.register(meta("comfyui", "test-skill")).await;
        let key = SkillKey {
            provider: ProviderName::new("comfyui"),
            moniker: Moniker::new("test-skill").unwrap(),
        };
        skills.unregister(&key).await;
        let snap = skills.snapshot();
        assert_eq!(snap.version, 2);
        assert!(snap.skills.is_empty());
    }

    #[tokio::test]
    async fn set_readiness_only_after_register() {
        let skills = Skills::new();
        let key = SkillKey {
            provider: ProviderName::new("comfyui"),
            moniker: Moniker::new("test-skill").unwrap(),
        };
        // Readiness before registration is dropped.
        skills
            .set_readiness(&key, readiness("http://stone:8188", true))
            .await;
        assert!(skills.snapshot().skills.is_empty());

        // Register, then readiness sticks.
        skills.register(meta("comfyui", "test-skill")).await;
        skills
            .set_readiness(&key, readiness("http://stone:8188", true))
            .await;
        let snap = skills.snapshot();
        let entry = snap.skills.get(&key).expect("entry");
        assert_eq!(entry.readiness.len(), 1);
        assert!(entry.readiness["http://stone:8188"].ready);
    }

    #[tokio::test]
    async fn register_replacing_preserves_readiness() {
        let skills = Skills::new();
        let key = SkillKey {
            provider: ProviderName::new("comfyui"),
            moniker: Moniker::new("test-skill").unwrap(),
        };
        skills.register(meta("comfyui", "test-skill")).await;
        skills
            .set_readiness(&key, readiness("http://stone:8188", true))
            .await;

        // Re-register with updated metadata (e.g. AI namer or hot-reload).
        let mut updated = meta("comfyui", "test-skill");
        updated.display_name = "Renamed".into();
        skills.register(updated).await;

        let snap = skills.snapshot();
        let entry = snap.skills.get(&key).expect("entry");
        assert_eq!(entry.meta.display_name, "Renamed");
        assert_eq!(entry.readiness.len(), 1, "readiness preserved across re-register");
    }

    #[tokio::test]
    async fn rename_updates_display_name_and_emits_event() {
        let skills = Skills::new();
        let key = SkillKey {
            provider: ProviderName::new("comfyui"),
            moniker: Moniker::new("test-skill").unwrap(),
        };
        let mut events = skills.skill_stream();
        skills.register(meta("comfyui", "test-skill")).await;
        skills
            .rename(&key, "Renamed Skill".into(), "New description".into())
            .await;

        let snap = skills.snapshot();
        let entry = snap.skills.get(&key).expect("entry");
        assert_eq!(entry.meta.display_name, "Renamed Skill");
        assert_eq!(entry.meta.description, "New description");

        // Drain the event stream — should see Registered then Named.
        let mut saw_named = false;
        while let Ok(event) = events.try_recv() {
            if let SkillEvent::Named { display_name, .. } = event {
                if display_name == "Renamed Skill" {
                    saw_named = true;
                }
            }
        }
        assert!(saw_named, "Named event was not emitted");
    }
}
