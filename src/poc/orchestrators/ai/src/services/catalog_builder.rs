//! Catalog builder background task.
//!
//! Subscribes to the unified [`crate::domain::events::EventBus`] and
//! pre-renders two JSON documents whenever any provider's capability
//! announcement changes:
//!
//! - The full `/v1/catalog` body.
//! - The abbreviated `/v1/do` action index with examples and hints.
//!
//! HTTP handlers read the pre-rendered `Arc<Value>` from the
//! published watch channel — no work on the hot path.
//!
//! # ORCH-0030 R2 M3
//!
//! The trigger source switched from "`Directory::on_snapshot()` watch
//! channel" to "EventBus subscription on
//! `directory.provider.*.updated`". The catalog walks
//! [`crate::services::directory_subscriber::CapabilityDirectory`]
//! directly — there is no separate `Skills` aggregate to consult any
//! more; ComfyUI publishes its skills as part of its
//! `CapabilityAnnouncement` and they appear via
//! `CapabilityDirectory::all_skills`.

use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::{broadcast, watch};
use tokio_util::sync::CancellationToken;

use crate::domain::capability_announcement::{Capability, SkillDeclaration};
use crate::domain::events::EventBus;
use crate::domain::ids::ProviderName;
use crate::domain::primitive::Primitive;
use crate::domain::vocabulary::VocabularyRegistry;
use crate::services::directory_subscriber::{CapabilityDirectory, ProviderCapabilities};

/// Bundle both pre-rendered documents and their directory version.
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
            })),
            actions_index: Arc::new(json!({
                "actions": [],
                "status": {
                    "providers_registered": 0,
                    "providers_enabled": 0,
                    "actions_available": 0,
                },
            })),
        }
    }
}

pub struct CatalogBuilder {
    capability_directory: Arc<CapabilityDirectory>,
    vocabularies: VocabularyRegistry,
    events: Arc<EventBus>,
    tx: watch::Sender<Arc<CatalogDocuments>>,
}

impl CatalogBuilder {
    pub fn new(
        capability_directory: Arc<CapabilityDirectory>,
        vocabularies: VocabularyRegistry,
        events: Arc<EventBus>,
    ) -> Arc<Self> {
        let (tx, _rx) = watch::channel(Arc::new(CatalogDocuments::initial()));
        Arc::new(Self {
            capability_directory,
            vocabularies,
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
        let mut bus_rx = self.events.raw_subscribe();
        // Render once at startup so handlers have valid data from
        // tick 0, even before any adapter has published.
        self.render_and_publish().await;

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => return,
                result = bus_rx.recv() => {
                    match result {
                        Ok(event) => {
                            // React to any directory.provider.*.updated
                            // event. The subscriber emits exactly one
                            // of these per accepted announcement,
                            // regardless of diff content.
                            if event.topic.starts_with("directory.provider.")
                                && event.topic.ends_with(".updated")
                            {
                                self.render_and_publish().await;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(
                                skipped = n,
                                "catalog_builder lagged on the bus; some announcements may have been missed",
                            );
                        }
                        Err(broadcast::error::RecvError::Closed) => return,
                    }
                }
            }
        }
    }

    async fn render_and_publish(&self) {
        let providers_map = self.capability_directory.providers().await;
        let directory_version = self.capability_directory.version();

        let catalog = Arc::new(render_catalog(&providers_map, &self.vocabularies));
        let actions_index = Arc::new(render_actions_index(&providers_map, &self.vocabularies));
        let docs = Arc::new(CatalogDocuments {
            directory_version,
            catalog,
            actions_index,
        });
        // `send_replace` rather than `send` because there may be no
        // active receivers when the catalog HTTP handlers haven't
        // been hit yet.
        let _ = self.tx.send_replace(docs);

        // Publish a `catalog.version` event on the unified bus so
        // subscribers can react without keeping a watch handle.
        // ORCH-0030 §1.6: this replaces the `/v1/catalog/events`
        // endpoint that used to expose the watch::channel directly.
        self.events
            .publish(
                "catalog.version",
                &serde_json::json!({
                    "version": directory_version,
                }),
            )
            .await;
    }
}

fn render_catalog(
    providers_map: &std::collections::HashMap<ProviderName, ProviderCapabilities>,
    vocabularies: &VocabularyRegistry,
) -> Value {
    // Compute, per primitive, the list of (provider, capability)
    // pairs that serve it. Used both for the per-primitive section
    // and the per-skill section.
    let primitives: Vec<Value> = Primitive::ALL
        .iter()
        .filter_map(|p| {
            let entries: Vec<(&ProviderName, &Capability)> = providers_map
                .values()
                .filter(|pc| pc.enabled)
                .filter_map(|pc| {
                    pc.announcement
                        .capabilities
                        .iter()
                        .find(|c| c.primitive == *p)
                        .map(|c| (&pc.provider, c))
                })
                .collect();
            if entries.is_empty() {
                return None;
            }
            let vocab = vocabularies.get(*p);
            let view = vocab.view();
            let providers_json = entries
                .iter()
                .map(|(name, cap)| {
                    let media_inputs: Vec<Value> = cap
                        .media_inputs
                        .iter()
                        .map(|m| {
                            json!({
                                "field": m.field,
                                "delivery": m.delivery,
                                "accepted_types": m.accepted_types,
                                "overlay": m.overlay,
                            })
                        })
                        .collect();
                    json!({
                        "name": name.as_str(),
                        "media_inputs": media_inputs,
                    })
                })
                .collect::<Vec<_>>();
            Some(json!({
                "action": p.dotted(),
                "modality": p.modality().as_str(),
                "summary": p.summary(),
                "vocabulary": view,
                "providers": providers_json,
            }))
        })
        .collect();

    // Skills: walk every enabled provider's published skill
    // declarations.
    let mut skills: Vec<Value> = Vec::new();
    for pc in providers_map.values().filter(|pc| pc.enabled) {
        for skill in &pc.announcement.skills {
            skills.push(render_skill_entry(&pc.provider, skill, vocabularies));
        }
    }

    // Provider summary section.
    let providers: Vec<Value> = providers_map
        .values()
        .map(|pc| {
            json!({
                "name": pc.provider.as_str(),
                "enabled": pc.enabled,
                "version": pc.version,
                "capability_count": pc.announcement.capabilities.len(),
                "skill_count": pc.announcement.skills.len(),
            })
        })
        .collect();

    // Modalities section — keyed list of active modalities with
    // display metadata (ORCH-0031: backend-owned icons).
    let active_modalities: std::collections::HashSet<&str> = primitives
        .iter()
        .filter_map(|p| p.get("modality").and_then(|v| v.as_str()))
        .collect();

    let modalities: Vec<Value> = crate::domain::primitive::Modality::ALL
        .iter()
        .filter(|m| active_modalities.contains(m.as_str()))
        .map(|m| {
            json!({
                "id": m.as_str(),
                "label": m.label(),
                "icon": m.icon(),
            })
        })
        .collect();

    json!({
        "modalities": modalities,
        "primitives": primitives,
        "skills": skills,
        "providers": providers,
    })
}

fn render_skill_entry(
    provider: &ProviderName,
    skill: &SkillDeclaration,
    vocabularies: &VocabularyRegistry,
) -> Value {
    let vocab = vocabularies.get(skill.primitive);
    let parameters: Vec<Value> = skill
        .parameters
        .iter()
        .map(|p| {
            let vocab_spec = vocab
                .input
                .required
                .iter()
                .chain(vocab.input.optional.iter())
                .find(|s| s.path.as_str() == p.field);
            let vocab_type = vocab_spec
                .map(|s| serde_json::to_value(&s.field_type).unwrap_or(Value::Null))
                .unwrap_or(Value::Null);
            let vocab_description = vocab_spec.map(|s| s.description.to_string());
            json!({
                "field": p.field,
                "required": p.required,
                "pinnable": p.pinnable,
                "label": p.description.clone().or(vocab_description),
                "default": p.default,
                "auto": p.auto,
                "type": vocab_type,
            })
        })
        .collect();
    json!({
        "action": format!("{}.{}", skill.primitive.dotted(), skill.id),
        "primitive": skill.primitive.dotted(),
        "id": skill.id,
        "display": {
            "name": skill.display.name,
            "description": skill.display.description,
            "tags": skill.display.tags,
            "preview_image": skill.display.preview_image,
        },
        "provider": provider.as_str(),
        "parameters": parameters,
    })
}

fn render_actions_index(
    providers_map: &std::collections::HashMap<ProviderName, ProviderCapabilities>,
    vocabularies: &VocabularyRegistry,
) -> Value {
    let mut actions: Vec<Value> = Vec::new();

    // One entry per primitive that has at least one enabled provider.
    for primitive in Primitive::ALL {
        let provider_names: Vec<String> = providers_map
            .values()
            .filter(|pc| pc.enabled)
            .filter(|pc| pc.announcement.has_capability(*primitive))
            .map(|pc| pc.provider.as_str().to_string())
            .collect();
        if provider_names.is_empty() {
            continue;
        }
        let vocab = vocabularies.get(*primitive);
        actions.push(json!({
            "action": primitive.dotted(),
            "url": format!("/v1/{}/{}", primitive.modality().as_str(), primitive.leaf()),
            "summary": primitive.summary(),
            "required": vocab.input.required.iter().map(|s| s.path.as_str()).collect::<Vec<_>>(),
            "providers": provider_names,
            "example": vocab.example_minimal.clone(),
        }));
    }

    // One entry per published skill across all enabled providers.
    for pc in providers_map.values().filter(|pc| pc.enabled) {
        for skill in &pc.announcement.skills {
            let vocab = vocabularies.get(skill.primitive);
            let url = format!(
                "/v1/{}/{}/{}",
                skill.primitive.modality().as_str(),
                skill.primitive.leaf(),
                skill.id
            );
            actions.push(json!({
                "action": format!("{}.{}", skill.primitive.dotted(), skill.id),
                "url": url,
                "summary": skill.display.description.clone()
                    .unwrap_or_else(|| format!("Skill `{}` for `{}`.", skill.id, skill.primitive.dotted())),
                "required": vocab.input.required.iter().map(|s| s.path.as_str()).collect::<Vec<_>>(),
                "providers": vec![pc.provider.as_str().to_string()],
                "example": vocab.example_minimal.clone(),
            }));
        }
    }

    let setup_hints = build_setup_hints(providers_map);

    let providers_registered = providers_map.len();
    let providers_enabled = providers_map.values().filter(|p| p.enabled).count();

    json!({
        "actions": actions,
        "status": {
            "providers_registered": providers_registered,
            "providers_enabled": providers_enabled,
            "actions_available": actions.len(),
        },
        "setup": if setup_hints.is_empty() {
            Value::Null
        } else {
            json!({"hints": setup_hints})
        },
    })
}

fn build_setup_hints(
    providers_map: &std::collections::HashMap<ProviderName, ProviderCapabilities>,
) -> Vec<String> {
    let mut hints = Vec::new();
    for pc in providers_map.values() {
        if !pc.enabled {
            hints.push(format!(
                "Provider `{}` is disabled (no healthy instances).",
                pc.provider
            ));
        }
    }
    for primitive in Primitive::ALL {
        let any = providers_map
            .values()
            .filter(|pc| pc.enabled)
            .any(|pc| pc.announcement.has_capability(*primitive));
        if !any {
            hints.push(format!(
                "No provider is registered for `{}` yet.",
                primitive.dotted()
            ));
        }
    }
    hints
}
