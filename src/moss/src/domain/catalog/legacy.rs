//! Transient free-function layer during the Ch2 → Ch4 Catalog strangler.
//!
//! These three functions (`ensure_offerings_index`, `get_compiled_offering`,
//! `rebuild_offerings_index`) were lifted verbatim from
//! `domain/offerings/catalog.rs` in Ch2 of ARCH-0022 (Book V of
//! ARCH-0017) so that every catalog concern lives under one
//! `domain/catalog/` directory from the start of the book. They
//! continue to coordinate the `AppState::manifest_registry` +
//! `AppState::offerings_index` strangler fields during Ch2 and Ch3
//! while the aggregate skeleton is being built alongside.
//!
//! Ch4 migrates their 25 caller sites to the aggregate's typed
//! commands (`state.catalog.load()` / `state.catalog.rebuild()` /
//! `state.catalog.get_compiled(name)`). Ch5 deletes this file.
//!
//! The function bodies are unchanged from their original location to
//! preserve behavioral equivalence — only the import paths are
//! rewritten to point at the new `super::*` module locations.

use anyhow::Result;

use super::cache::{CatalogCache, FileCatalogCache};
use super::entry::CompiledOffering;
use super::fingerprint::{current_capabilities_hash, manifests_hash, moss_version_string};
use super::index::{OfferingsFingerprint, OfferingsIndex};

use crate::domain::compatibility::compile_compatibility;
use garden_common::manifests::ManifestRegistry;

/// Ensure offerings index is loaded (with caching).
///
/// Loads offerings index from cache or rebuilds if:
/// - Cache doesn't exist
/// - `force_rebuild` is true
/// - Fingerprint doesn't match (version/capabilities/manifests changed)
pub async fn ensure_offerings_index(
    state: &crate::AppState,
    force_rebuild: bool,
    cache: &FileCatalogCache,
) -> Result<()> {
    if !force_rebuild {
        let existing = state.offerings_index.read().await;
        if existing.is_some() {
            return Ok(());
        }
    }

    // Snapshot cached capabilities once
    let cached_caps = state.current.capabilities.read().await.clone();
    let cached_caps_ref = cached_caps.as_ref();

    // Try disk cache first (best-effort)
    if !force_rebuild && let Some(on_disk) = cache.load().await? {
        let current = OfferingsFingerprint {
            moss_version: moss_version_string(),
            capabilities_hash: current_capabilities_hash(cached_caps_ref),
            templates_hash: manifests_hash(&state.manifest_registry)?,
        };

        if on_disk.fingerprint == current {
            *state.offerings_index.write().await = Some(on_disk);
            return Ok(());
        }
    }

    let rebuilt = rebuild_offerings_index(&state.manifest_registry, cached_caps_ref)?;
    cache.save(&rebuilt).await?;
    *state.offerings_index.write().await = Some(rebuilt);
    Ok(())
}

/// Get a compiled offering by name.
///
/// Ensures offerings index is loaded, then queries for specific offering.
pub async fn get_compiled_offering(
    state: &crate::AppState,
    offering: &str,
    cache: &FileCatalogCache,
) -> Result<Option<CompiledOffering>> {
    ensure_offerings_index(state, false, cache).await?;
    let guard = state.offerings_index.read().await;
    Ok(guard
        .as_ref()
        .and_then(|idx| idx.offerings.iter().find(|o| o.name == offering).cloned()))
}

/// Rebuild offerings index from `ManifestRegistry`.
///
/// Evaluates compatibility rules and compiles all offerings.
/// Returns cache-ready index with fingerprint.
pub fn rebuild_offerings_index(
    registry: &ManifestRegistry,
    cached_capabilities: Option<&garden_common::HardwareCapabilities>,
) -> Result<OfferingsIndex> {
    let mut entries: Vec<_> = registry.sw.entries.values().collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));

    let fingerprint = OfferingsFingerprint {
        moss_version: moss_version_string(),
        capabilities_hash: current_capabilities_hash(cached_capabilities),
        templates_hash: manifests_hash(registry)?,
    };

    let mut offerings = Vec::with_capacity(entries.len());
    for entry in entries {
        let mut template = entry.parse_template()?;
        let compatibility = compile_compatibility(&mut template, cached_capabilities);

        offerings.push(CompiledOffering {
            name: entry.name.clone(),
            category: entry.category.clone(),
            description: entry.description(),
            tags: entry.tags(),
            image: template.image,
            command: template.command,
            config_files: template.config_files,
            ports: template.ports,
            environment: template.environment,
            volumes: template.volumes,
            compatibility,
            tasks: template.tasks,
            network: template.network,
            coordination: entry.coordination.clone(),
            device_requests: template.device_requests,
        });
    }

    Ok(OfferingsIndex {
        fingerprint,
        generated_at: chrono::Utc::now().to_rfc3339(),
        offerings,
    })
}
