//! The `Directory` aggregate — single source of truth for provider
//! metadata.
//!
//! Follows the ORCH-0020 domain-owned state pattern:
//! - Private mutable state behind a `tokio::sync::Mutex`.
//! - Immutable `DirectorySnapshot` published via `watch::channel`.
//! - Readers acquire snapshots lock-free via [`Directory::snapshot`].
//!
//! Change propagation is pure event-driven:
//! - On `register`, a per-provider forwarder task is spawned that
//!   watches the provider's state channel and bumps the Directory's
//!   dirty counter on every publication.
//! - The `directory_maintenance` task listens to the dirty counter,
//!   debounces a short window to coalesce bursts, and rebuilds.
//! - No timers, no polling, no periodic refresh.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::{watch, Mutex};

use crate::domain::ids::{ModelFqn, ProviderName, RegistrationId};
use crate::domain::moniker::Moniker;
use crate::domain::primitive::Primitive;
use crate::domain::provider::{
    Model, ModelDescriptor, Provider, ProviderHealth, Registration, RegistrationStrategy,
};

// ── Snapshot ──────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct DirectorySnapshot {
    pub version: u64,
    pub updated_at: DateTime<Utc>,
    pub providers: Arc<HashMap<ProviderName, ProviderView>>,
    pub primitives: Arc<HashMap<Primitive, PrimitiveView>>,
    pub skills: Arc<HashMap<SkillKey, SkillView>>,
    pub models: Arc<HashMap<ModelFqn, ModelView>>,
}

impl DirectorySnapshot {
    pub fn providers_count(&self) -> usize {
        self.providers.len()
    }

    pub fn primitives_count(&self) -> usize {
        self.primitives.len()
    }

    pub fn healthy_provider_count(&self) -> usize {
        self.providers
            .values()
            .filter(|v| matches!(v.health, ProviderHealth::Healthy))
            .count()
    }

    pub fn degraded_provider_count(&self) -> usize {
        self.providers
            .values()
            .filter(|v| matches!(v.health, ProviderHealth::Degraded { .. }))
            .count()
    }

    pub fn offline_provider_count(&self) -> usize {
        self.providers
            .values()
            .filter(|v| matches!(v.health, ProviderHealth::Offline { .. }))
            .count()
    }

    pub fn find_registration(
        &self,
        provider: &ProviderName,
        primitive: Primitive,
        skill: Option<&Moniker>,
    ) -> Option<&Registration> {
        let view = self.providers.get(provider)?;
        view.registrations.iter().find(|r| {
            if r.primitive != primitive {
                return false;
            }
            match (&r.strategy, skill) {
                (RegistrationStrategy::Skill { moniker, .. }, Some(m)) => moniker == m,
                (RegistrationStrategy::Skill { .. }, None) => false,
                (_, None) => true,
                (_, Some(_)) => false,
            }
        })
    }

    /// Find the registration for a skill, regardless of provider.
    pub fn find_skill(&self, primitive: Primitive, moniker: &Moniker) -> Option<&SkillView> {
        let key = SkillKey {
            primitive,
            moniker: moniker.clone(),
        };
        self.skills.get(&key)
    }

    /// All providers currently registered for a primitive.
    pub fn providers_for(&self, primitive: Primitive) -> Vec<&ProviderView> {
        self.primitives
            .get(&primitive)
            .into_iter()
            .flat_map(|view| {
                view.providers
                    .iter()
                    .filter_map(|name| self.providers.get(name))
            })
            .collect()
    }

    /// Look up a model by fully-qualified name.
    pub fn model(&self, fqn: &ModelFqn) -> Option<&ModelView> {
        self.models.get(fqn)
    }

    /// Look up models whose short name matches (may return multiple
    /// across providers).
    pub fn models_by_short_name<'a>(
        &'a self,
        short: &'a str,
    ) -> impl Iterator<Item = &'a ModelView> + 'a {
        self.models.values().filter(move |m| m.short_name == short)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProviderView {
    pub name: ProviderName,
    pub health: ProviderHealth,
    pub registrations: Vec<Registration>,
    pub models: Vec<Model>,
}

#[derive(Debug, Clone, Default)]
pub struct PrimitiveView {
    pub primitive: Option<Primitive>,
    pub providers: Vec<ProviderName>,
    pub registration_ids: Vec<RegistrationId>,
    pub skill_monikers: Vec<Moniker>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct SkillKey {
    pub primitive: Primitive,
    pub moniker: Moniker,
}

#[derive(Debug, Clone)]
pub struct SkillView {
    pub registration: Registration,
    pub provider: ProviderName,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelView {
    pub fqn: ModelFqn,
    pub short_name: String,
    pub provider: ProviderName,
    pub registration_id: RegistrationId,
    pub primitives: Vec<String>,
    pub capability_tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_length: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter_count: Option<u64>,
}

// ── Directory aggregate ───────────────────────────────────────

/// The aggregate root. Readers use [`Directory::snapshot`] or
/// [`Directory::subscribe`]; writers go through [`Directory::register`]
/// and [`Directory::unregister`].
pub struct Directory {
    state: Mutex<DirectoryState>,
    snapshot_tx: watch::Sender<Arc<DirectorySnapshot>>,
    dirty_tx: watch::Sender<u64>,
}

struct DirectoryState {
    providers: HashMap<ProviderName, Arc<dyn Provider>>,
    /// Handles to the per-provider forwarder tasks so we can abort
    /// them on unregister.
    forwarders: HashMap<ProviderName, tokio::task::JoinHandle<()>>,
    dirty_count: u64,
}

impl Directory {
    /// Construct an empty directory. Readers see version 0 until the
    /// first provider event triggers a rebuild.
    pub fn new() -> Arc<Self> {
        let initial = Arc::new(DirectorySnapshot {
            version: 0,
            updated_at: Utc::now(),
            ..Default::default()
        });
        let (snapshot_tx, _) = watch::channel(initial);
        let (dirty_tx, _) = watch::channel(0u64);
        Arc::new(Self {
            state: Mutex::new(DirectoryState {
                providers: HashMap::new(),
                forwarders: HashMap::new(),
                dirty_count: 0,
            }),
            snapshot_tx,
            dirty_tx,
        })
    }

    /// Register a provider.
    ///
    /// Spawns a forwarder task subscribed to the provider's state
    /// channel; every publication (including the provider's initial
    /// state) bumps the Directory's dirty counter. The
    /// `directory_maintenance` task picks up the bump, debounces, and
    /// rebuilds. Fails on duplicate name.
    pub async fn register(&self, provider: Arc<dyn Provider>) -> Result<(), DirectoryError> {
        let mut state = self.state.lock().await;
        let name = provider.name();
        if state.providers.contains_key(&name) {
            return Err(DirectoryError::DuplicateProvider(name));
        }

        // Insert first so the rebuild triggered by the forwarder's
        // initial-value fire always sees this provider.
        state.providers.insert(name.clone(), provider.clone());

        let mut rx = provider.subscribe();
        let dirty_tx = self.dirty_tx.clone();
        let forwarder = tokio::spawn(async move {
            // A freshly-subscribed receiver treats the current value as
            // unseen, so the first `changed()` fires immediately with
            // the provider's initial state. That is the first event.
            while rx.changed().await.is_ok() {
                let next = dirty_tx.borrow().wrapping_add(1);
                let _ = dirty_tx.send(next);
            }
        });
        state.forwarders.insert(name, forwarder);
        Ok(())
    }

    /// Unregister a provider and abort its forwarder.
    pub async fn unregister(&self, name: &ProviderName) {
        let mut state = self.state.lock().await;
        state.providers.remove(name);
        if let Some(handle) = state.forwarders.remove(name) {
            handle.abort();
        }
        // No forwarder means no event — bump dirty ourselves so the
        // maintenance task rebuilds.
        state.dirty_count = state.dirty_count.wrapping_add(1);
        let count = state.dirty_count;
        drop(state);
        let _ = self.dirty_tx.send(count);
    }

    /// Look up a provider by name (lock-free via the snapshot's
    /// providers map is not possible because [`DirectorySnapshot`]
    /// stores views, not handles; use this method when you need the
    /// trait object).
    pub async fn provider(&self, name: &ProviderName) -> Option<Arc<dyn Provider>> {
        let state = self.state.lock().await;
        state.providers.get(name).cloned()
    }

    /// Lock-free snapshot read.
    pub fn snapshot(&self) -> Arc<DirectorySnapshot> {
        self.snapshot_tx.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<Arc<DirectorySnapshot>> {
        self.snapshot_tx.subscribe()
    }

    /// Subscribe to the dirty-count pulse used by the maintenance task.
    pub fn subscribe_dirty(&self) -> watch::Receiver<u64> {
        self.dirty_tx.subscribe()
    }

    /// Rebuild the snapshot from the current state. Called by the
    /// `directory_maintenance` task and a few tests.
    pub async fn rebuild_snapshot(&self) {
        let state = self.state.lock().await;
        let current = self.snapshot_tx.borrow().clone();

        let mut providers_map: HashMap<ProviderName, ProviderView> = HashMap::new();
        let mut primitives_map: HashMap<Primitive, PrimitiveView> = HashMap::new();
        let mut skills_map: HashMap<SkillKey, SkillView> = HashMap::new();
        let mut models_map: HashMap<ModelFqn, ModelView> = HashMap::new();

        for (name, provider) in &state.providers {
            let provider_state = provider.state();
            let view = ProviderView {
                name: name.clone(),
                health: provider_state.health.clone(),
                registrations: provider_state.registrations.clone(),
                models: provider_state.models.clone(),
            };

            for reg in &provider_state.registrations {
                let entry = primitives_map
                    .entry(reg.primitive)
                    .or_insert_with(|| PrimitiveView {
                        primitive: Some(reg.primitive),
                        providers: Vec::new(),
                        registration_ids: Vec::new(),
                        skill_monikers: Vec::new(),
                    });
                if !entry.providers.contains(name) {
                    entry.providers.push(name.clone());
                }
                entry.registration_ids.push(reg.id.clone());
                if let RegistrationStrategy::Skill { moniker, .. } = &reg.strategy {
                    entry.skill_monikers.push(moniker.clone());
                    skills_map.insert(
                        SkillKey {
                            primitive: reg.primitive,
                            moniker: moniker.clone(),
                        },
                        SkillView {
                            registration: reg.clone(),
                            provider: name.clone(),
                        },
                    );
                }
                if let RegistrationStrategy::Models { catalog } = &reg.strategy {
                    for model in catalog {
                        let fqn = ModelFqn::new(name, &model.short_name);
                        models_map.insert(
                            fqn.clone(),
                            model_view_from_descriptor(name, &reg.id, fqn, reg.primitive, model),
                        );
                    }
                }
            }

            for model in &provider_state.models {
                models_map
                    .entry(model.fqn.clone())
                    .or_insert_with(|| model_view_from_model(name, model));
            }

            providers_map.insert(name.clone(), view);
        }

        let mut built = DirectorySnapshot {
            version: current.version,
            updated_at: Utc::now(),
            providers: Arc::new(providers_map),
            primitives: Arc::new(primitives_map),
            skills: Arc::new(skills_map),
            models: Arc::new(models_map),
        };
        drop(state);

        // Bump version only on structural change. `send_replace` is
        // used because at startup there may be no subscribers yet; it
        // updates the stored value regardless of receiver presence.
        if snapshots_differ(&current, &built) {
            built.version = current.version.saturating_add(1);
            let _ = self.snapshot_tx.send_replace(Arc::new(built));
        }
    }

}

fn model_view_from_descriptor(
    provider: &ProviderName,
    registration_id: &RegistrationId,
    fqn: ModelFqn,
    primitive: Primitive,
    descriptor: &ModelDescriptor,
) -> ModelView {
    ModelView {
        fqn,
        short_name: descriptor.short_name.clone(),
        provider: provider.clone(),
        registration_id: registration_id.clone(),
        primitives: vec![primitive.dotted().to_string()],
        capability_tags: descriptor.capability_tags.clone(),
        size_bytes: descriptor.size_bytes,
        context_length: descriptor.context_length,
        parameter_count: descriptor.parameter_count,
    }
}

fn model_view_from_model(provider: &ProviderName, model: &Model) -> ModelView {
    ModelView {
        fqn: model.fqn.clone(),
        short_name: model.short_name.clone(),
        provider: provider.clone(),
        registration_id: RegistrationId::from("unknown"),
        primitives: model.primitives.iter().map(|p| p.dotted().to_string()).collect(),
        capability_tags: model.capability_tags.clone(),
        size_bytes: model.size_bytes,
        context_length: model.context_length,
        parameter_count: model.parameter_count,
    }
}

fn snapshots_differ(prev: &DirectorySnapshot, next: &DirectorySnapshot) -> bool {
    if prev.providers.len() != next.providers.len() {
        return true;
    }
    if prev.skills.len() != next.skills.len() {
        return true;
    }
    if prev.models.len() != next.models.len() {
        return true;
    }
    for (name, view) in next.providers.iter() {
        let Some(prev_view) = prev.providers.get(name) else {
            return true;
        };
        if prev_view.health != view.health {
            return true;
        }
        if prev_view.registrations.len() != view.registrations.len() {
            return true;
        }
        if prev_view.models.len() != view.models.len() {
            return true;
        }
    }
    false
}

#[derive(Debug, thiserror::Error)]
pub enum DirectoryError {
    #[error("duplicate provider: {0}")]
    DuplicateProvider(ProviderName),
    #[error("duplicate moniker: {primitive} / {moniker}")]
    DuplicateMoniker {
        primitive: Primitive,
        moniker: Moniker,
    },
}
