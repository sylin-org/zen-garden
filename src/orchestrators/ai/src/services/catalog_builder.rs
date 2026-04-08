//! Catalog builder background task.
//!
//! Subscribes to the [`crate::domain::directory::Directory`] snapshot
//! and pre-renders two JSON documents whenever the version bumps:
//!
//! - The full `/v1/catalog` body.
//! - The abbreviated `/v1/do` action index with examples and hints.
//!
//! HTTP handlers read the pre-rendered `Arc<Value>` from the
//! published watch channel — no work on the hot path.

use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::domain::directory::{Directory, DirectorySnapshot};
use crate::domain::events::EventBus;
use crate::domain::primitive::Primitive;
use crate::domain::provider::ProviderHealth;
use crate::domain::vocabulary::VocabularyRegistry;
use crate::services::skills::registry::{Skills, SkillsSnapshot};

/// Bundle both pre-rendered documents and their Directory version.
#[derive(Clone)]
pub struct CatalogDocuments {
    pub directory_version: u64,
    pub catalog: Arc<Value>,
    pub actions_index: Arc<Value>,
}

impl CatalogDocuments {
    pub fn initial() -> Self {
        Self {
            directory_version: 0,
            catalog: Arc::new(json!({
                "version": 0,
                "primitives": [],
                "skills": [],
                "providers": [],
                "models": [],
            })),
            actions_index: Arc::new(json!({
                "actions": [],
                "status": {
                    "providers_registered": 0,
                    "providers_healthy": 0,
                    "providers_degraded": 0,
                    "providers_offline": 0,
                    "actions_available": 0,
                    "models_discovered": 0,
                },
            })),
        }
    }
}

pub struct CatalogBuilder {
    directory: Arc<Directory>,
    vocabularies: VocabularyRegistry,
    skills: Arc<Skills>,
    events: Arc<EventBus>,
    tx: watch::Sender<Arc<CatalogDocuments>>,
}

impl CatalogBuilder {
    pub fn new(
        directory: Arc<Directory>,
        vocabularies: VocabularyRegistry,
        skills: Arc<Skills>,
        events: Arc<EventBus>,
    ) -> Arc<Self> {
        let (tx, _rx) = watch::channel(Arc::new(CatalogDocuments::initial()));
        Arc::new(Self {
            directory,
            vocabularies,
            skills,
            events,
            tx,
        })
    }

    pub fn subscribe(&self) -> watch::Receiver<Arc<CatalogDocuments>> {
        self.tx.subscribe()
    }

    pub fn snapshot(&self) -> Arc<CatalogDocuments> {
        self.tx.borrow().clone()
    }

    pub async fn run(self: Arc<Self>, shutdown: CancellationToken) {
        let mut rx_dir = self.directory.subscribe();
        let mut rx_skills = self.skills.subscribe();
        // Render immediately so handlers have valid data from tick 0.
        let initial = rx_dir.borrow_and_update().clone();
        let _ = rx_skills.borrow_and_update();
        self.render_and_publish(&initial).await;

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                changed = rx_dir.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    let snap = rx_dir.borrow_and_update().clone();
                    self.render_and_publish(&snap).await;
                }
                changed = rx_skills.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    let _ = rx_skills.borrow_and_update();
                    let snap = rx_dir.borrow().clone();
                    self.render_and_publish(&snap).await;
                }
            }
        }
    }

    async fn render_and_publish(&self, snapshot: &Arc<DirectorySnapshot>) {
        let skills_snapshot = self.skills.snapshot();
        let catalog = Arc::new(render_catalog(snapshot, &self.vocabularies, &skills_snapshot));
        let actions_index = Arc::new(render_actions_index(snapshot, &self.vocabularies));
        let docs = Arc::new(CatalogDocuments {
            directory_version: snapshot.version,
            catalog,
            actions_index,
        });
        // `send_replace` rather than `send` because there may be no
        // active receivers when the catalog HTTP handlers haven't
        // been hit yet — `send` would silently fail and the stored
        // value would never advance past `CatalogDocuments::initial`.
        let _ = self.tx.send_replace(docs);

        // Publish a `catalog.version` event on the unified bus so
        // subscribers can react without keeping a watch handle.
        // ORCH-0030 §1.6: this replaces the `/v1/catalog/events`
        // endpoint that used to expose the watch::channel directly.
        self.events
            .publish(
                "catalog.version",
                &serde_json::json!({
                    "version": snapshot.version,
                }),
            )
            .await;
    }
}

fn render_catalog(
    snapshot: &DirectorySnapshot,
    vocabularies: &VocabularyRegistry,
    skills_snapshot: &SkillsSnapshot,
) -> Value {
    let primitives: Vec<Value> = Primitive::ALL
        .iter()
        .filter(|p| !snapshot.providers_for(**p).is_empty())
        .map(|p| {
            let vocab = vocabularies.get(*p);
            let view = vocab.view();
            let providers = snapshot
                .providers_for(*p)
                .into_iter()
                .map(|pv| {
                    let honors: Vec<String> = pv
                        .registrations
                        .iter()
                        .filter(|r| r.primitive == *p)
                        .flat_map(|r| r.honored_fields.iter().map(|h| h.path.as_str().to_string()))
                        .collect();
                    json!({
                        "name": pv.name.as_str(),
                        "honors": honors,
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "action": p.dotted(),
                "modality": p.modality().as_str(),
                "summary": p.summary(),
                "vocabulary": view,
                "providers": providers,
            })
        })
        .collect();

    let skills: Vec<Value> = snapshot
        .skills
        .values()
        .map(|skill| {
            let reg = &skill.registration;
            let (display, description) = match &reg.strategy {
                crate::domain::provider::RegistrationStrategy::Skill {
                    display_name,
                    description,
                    ..
                } => (display_name.clone(), description.clone()),
                _ => (String::new(), None),
            };
            let moniker_str = reg.moniker().map(|m| m.as_str().to_string()).unwrap_or_default();

            // Look up the dynamic state from the Skills aggregate.
            let skills_entry = skills_snapshot.skills.values().find(|e| {
                e.meta.provider == skill.provider
                    && e.meta.moniker.as_str() == moniker_str
            });

            // Render the skill's bindings as a typed list. Each
            // binding overlays the vocabulary's `FieldType` with the
            // skill's `FieldConstraint` (Range / Options / Auto).
            let vocab = vocabularies.get(reg.primitive);
            let fields: Vec<Value> = reg
                .honored_fields
                .iter()
                .map(|hf| {
                    let vocab_spec = vocab.input.required.iter().chain(vocab.input.optional.iter())
                        .find(|s| s.path.as_str() == hf.path.as_str());
                    let vocab_type = vocab_spec
                        .map(|s| serde_json::to_value(&s.field_type).unwrap_or(Value::Null))
                        .unwrap_or(Value::Null);
                    let description = vocab_spec.map(|s| s.description.to_string());
                    let constraint = hf.constraint.as_ref().map(|c| {
                        serde_json::to_value(c).unwrap_or(Value::Null)
                    });
                    json!({
                        "path": hf.path.as_str(),
                        "required": hf.required,
                        "label": hf.label.clone().or(description),
                        "default": hf.default,
                        "type": vocab_type,
                        "constraint": constraint,
                    })
                })
                .collect();

            // Pull skill-meta fields (variants, model_selector, etc.)
            // from the Skills aggregate.
            let variants = skills_entry
                .and_then(|e| e.meta.variants.as_ref())
                .map(|v| serde_json::to_value(v).unwrap_or(Value::Null));
            let model_selector = skills_entry
                .and_then(|e| e.meta.model_selector.as_ref())
                .map(|s| serde_json::to_value(s).unwrap_or(Value::Null));
            let required_models = skills_entry
                .map(|e| serde_json::to_value(&e.meta.required_models).unwrap_or(Value::Null));
            let source = skills_entry
                .and_then(|e| e.meta.source.as_ref())
                .map(|s| serde_json::to_value(s).unwrap_or(Value::Null));
            let preview_url = skills_entry.and_then(|e| e.meta.preview_url.clone());
            let readiness: Vec<Value> = skills_entry
                .map(|e| {
                    e.readiness
                        .values()
                        .map(|r| serde_json::to_value(r).unwrap_or(Value::Null))
                        .collect()
                })
                .unwrap_or_default();

            json!({
                "action": format!("{}.{}", reg.primitive.dotted(), moniker_str),
                "primitive": reg.primitive.dotted(),
                "moniker": moniker_str,
                "display_name": display,
                "description": description,
                "provider": skill.provider.as_str(),
                "fields": fields,
                "media_inputs": reg.media_inputs.iter().map(|m| json!({
                    "field": m.field.as_str(),
                    "delivery": m.delivery,
                    "accepted_types": m.accepted_types,
                    "overlay": m.overlay,
                })).collect::<Vec<_>>(),
                "variants": variants,
                "model_selector": model_selector,
                "required_models": required_models,
                "source": source,
                "preview_url": preview_url,
                "readiness": readiness,
            })
        })
        .collect();

    let providers: Vec<Value> = snapshot
        .providers
        .values()
        .map(|pv| {
            json!({
                "name": pv.name.as_str(),
                "health": match &pv.health {
                    ProviderHealth::Healthy => "healthy",
                    ProviderHealth::Degraded { .. } => "degraded",
                    ProviderHealth::Offline { .. } => "offline",
                },
                "registration_count": pv.registrations.len(),
                "model_count": pv.models.len(),
            })
        })
        .collect();

    let models: Vec<Value> = snapshot
        .models
        .values()
        .map(|m| serde_json::to_value(m).unwrap_or(Value::Null))
        .collect();

    json!({
        "version": snapshot.version,
        "updated_at": snapshot.updated_at,
        "primitives": primitives,
        "skills": skills,
        "providers": providers,
        "models": models,
    })
}

fn render_actions_index(snapshot: &DirectorySnapshot, vocabularies: &VocabularyRegistry) -> Value {
    let mut actions: Vec<Value> = Vec::new();

    // One entry per primitive that has at least one registered provider.
    for primitive in Primitive::ALL {
        let providers_for = snapshot.providers_for(*primitive);
        if providers_for.is_empty() {
            continue;
        }
        let vocab = vocabularies.get(*primitive);
        let provider_names: Vec<String> = providers_for
            .iter()
            .map(|p| p.name.as_str().to_string())
            .collect();
        actions.push(json!({
            "action": primitive.dotted(),
            "url": format!("/v1/{}/{}", primitive.modality().as_str(), primitive.leaf()),
            "summary": primitive.summary(),
            "required": vocab.input.required.iter().map(|s| s.path.as_str()).collect::<Vec<_>>(),
            "providers": provider_names,
            "example": vocab.example_minimal.clone(),
        }));
    }

    // One entry per registered skill.
    for (key, skill) in snapshot.skills.iter() {
        let vocab = vocabularies.get(key.primitive);
        let url = format!(
            "/v1/{}/{}/{}",
            key.primitive.modality().as_str(),
            key.primitive.leaf(),
            key.moniker
        );
        actions.push(json!({
            "action": format!("{}.{}", key.primitive.dotted(), key.moniker),
            "url": url,
            "summary": match &skill.registration.strategy {
                crate::domain::provider::RegistrationStrategy::Skill { description, .. } => {
                    description.clone().unwrap_or_else(|| format!("Skill `{}` for `{}`.", key.moniker, key.primitive.dotted()))
                }
                _ => format!("Skill `{}` for `{}`.", key.moniker, key.primitive.dotted()),
            },
            "required": vocab.input.required.iter().map(|s| s.path.as_str()).collect::<Vec<_>>(),
            "providers": vec![skill.provider.as_str()],
            "example": vocab.example_minimal.clone(),
        }));
    }

    let setup_hints = build_setup_hints(snapshot);

    json!({
        "actions": actions,
        "status": {
            "providers_registered": snapshot.providers_count(),
            "providers_healthy": snapshot.healthy_provider_count(),
            "providers_degraded": snapshot.degraded_provider_count(),
            "providers_offline": snapshot.offline_provider_count(),
            "actions_available": actions.len(),
            "models_discovered": snapshot.models.len(),
        },
        "setup": if setup_hints.is_empty() {
            Value::Null
        } else {
            json!({"hints": setup_hints})
        },
    })
}

fn build_setup_hints(snapshot: &DirectorySnapshot) -> Vec<String> {
    let mut hints = Vec::new();
    for (name, view) in snapshot.providers.iter() {
        match &view.health {
            ProviderHealth::Degraded { reason } => {
                hints.push(format!("Provider `{}` is degraded: {}", name, reason));
            }
            ProviderHealth::Offline { reason } => {
                hints.push(format!("Provider `{}` is offline: {}", name, reason));
            }
            ProviderHealth::Healthy => {}
        }
    }
    for primitive in Primitive::ALL {
        if snapshot.providers_for(*primitive).is_empty() {
            hints.push(format!(
                "No provider is registered for `{}` yet.",
                primitive.dotted()
            ));
        }
    }
    hints
}
