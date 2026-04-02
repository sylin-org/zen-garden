//! Skill provisioner — orchestrator-side dependency coordination (ORCH-0022).
//!
//! The orchestrator knows WHAT is needed (from skill.json). The provider knows
//! HOW to get it. This module coordinates:
//!
//! 1. For each skill's required models → download to workspace → checksum → ingest to cache
//! 2. For each instance → push cached models that are missing
//! 3. Update skill readiness based on instance state

use std::path::Path;

use anyhow::{Context, Result};

use crate::domain::skill::{ModelRef, SkillDefinition, SkillInstanceView, SkillReadiness};
use crate::skills::cache::{self, CachePaths, DependencyManifest, IngestResult};

/// Ensure all required models for a skill are in the local cache.
///
/// Downloads missing models to the workspace, computes checksums, and
/// ingests into the cache with dedup. Returns the list of canonical
/// filenames in the cache (after alias resolution).
pub async fn ensure_cached(
    http: &reqwest::Client,
    skill: &SkillDefinition,
    cache_paths: &CachePaths,
) -> Result<Vec<CachedModel>> {
    cache_paths.ensure_dirs().await?;

    let mut manifest = DependencyManifest::load(&cache_paths.manifest_path).await;
    let skill_moniker = skill.name.rsplit('.').next().unwrap_or(&skill.name);
    let workspace = cache_paths.workspace_for_skill(skill_moniker);
    tokio::fs::create_dir_all(&workspace).await?;

    let mut cached_models = Vec::new();

    for model in &skill.required_models {
        // Check if already cached (by filename or alias)
        let resolved = manifest.resolve(&model.filename);
        if manifest.files.contains_key(&resolved) {
            tracing::debug!(
                model = %model.filename,
                resolved = %resolved,
                "model already in cache"
            );
            cached_models.push(CachedModel {
                original_name: model.filename.clone(),
                canonical_name: resolved,
                model_type: model.model_type.clone(),
                cache_path: cache_paths.provider_dir.join(manifest.resolve(&model.filename)),
            });
            continue;
        }

        // Download URL is required
        let url = model.url.as_deref().unwrap_or("");
        if url.is_empty() {
            tracing::warn!(
                model = %model.filename,
                skill = %skill.name,
                "model has no download URL — skipping"
            );
            continue;
        }

        // Download to workspace
        let ws_path = workspace.join(&model.filename);
        tracing::info!(
            model = %model.filename,
            skill = %skill.name,
            url = %url,
            "downloading model dependency"
        );

        let (downloaded_path, checksum) = cache::stream_download(
            http,
            url,
            &ws_path,
            model.size_bytes,
        )
        .await
        .with_context(|| format!("download model: {}", model.filename))?;

        // Verify checksum if skill.json provides one
        if let Some(expected) = &model.sha256 {
            let expected_full = if expected.starts_with("sha256:") {
                expected.clone()
            } else {
                format!("sha256:{expected}")
            };
            if checksum != expected_full {
                let _ = tokio::fs::remove_file(&downloaded_path).await;
                anyhow::bail!(
                    "checksum mismatch for {}: expected {}, got {}",
                    model.filename, expected_full, checksum
                );
            }
        }

        // Ingest into cache (dedup)
        let result = cache::ingest_to_cache(
            &mut manifest,
            &cache_paths.provider_dir,
            &downloaded_path,
            &model.filename,
            &checksum,
        )
        .await
        .with_context(|| format!("ingest model: {}", model.filename))?;

        let canonical_name = match &result {
            IngestResult::Added { canonical_name } => {
                tracing::info!(model = %model.filename, "new model cached");
                canonical_name.clone()
            }
            IngestResult::AlreadyCached => {
                tracing::debug!(model = %model.filename, "model already cached (race)");
                model.filename.clone()
            }
            IngestResult::Aliased { canonical_name, alias_from } => {
                tracing::info!(
                    model = %alias_from,
                    canonical = %canonical_name,
                    "model is alias of existing cached file"
                );
                canonical_name.clone()
            }
            IngestResult::Renamed { canonical_name, original_name } => {
                tracing::info!(
                    model = %original_name,
                    stored_as = %canonical_name,
                    "name conflict — stored with incremented name"
                );
                canonical_name.clone()
            }
        };

        cached_models.push(CachedModel {
            original_name: model.filename.clone(),
            canonical_name: canonical_name.clone(),
            model_type: model.model_type.clone(),
            cache_path: cache_paths.provider_dir.join(&canonical_name),
        });
    }

    // Save manifest
    manifest.save(&cache_paths.manifest_path).await?;

    // Clean workspace
    let _ = tokio::fs::remove_dir_all(&workspace).await;

    Ok(cached_models)
}

/// Push cached models to a ComfyUI instance via Moss volume API.
///
/// Checks each model via HEAD first — only pushes what's missing.
/// Streams from disk — never buffers in memory.
pub async fn push_to_instance(
    http: &reqwest::Client,
    cached_models: &[CachedModel],
    moss_endpoint: &str,
    offering_fqn: &str,
    volume_name: &str,
) -> Result<()> {
    for model in cached_models {
        let remote_path = format!("{}/{}", model.model_type, model.canonical_name);

        // HEAD check — skip if already on instance
        let exists = crate::skills::prep::model_exists_on_instance(
            http,
            moss_endpoint,
            offering_fqn,
            volume_name,
            &remote_path,
        )
        .await;

        if exists {
            tracing::debug!(
                model = %model.canonical_name,
                "model already on instance"
            );
            continue;
        }

        // Stream push
        tracing::info!(
            model = %model.canonical_name,
            endpoint = %moss_endpoint,
            "pushing model to instance"
        );

        crate::skills::prep::push_model_to_instance(
            http,
            moss_endpoint,
            offering_fqn,
            volume_name,
            &remote_path,
            &model.cache_path,
        )
        .await
        .with_context(|| format!("push {} to instance", model.canonical_name))?;
    }

    Ok(())
}

/// Check readiness of a skill on a specific ComfyUI instance.
///
/// Verifies all required models exist on the instance via HEAD checks.
pub async fn check_instance_readiness(
    http: &reqwest::Client,
    skill: &SkillDefinition,
    manifest: &DependencyManifest,
    moss_endpoint: &str,
    offering_fqn: &str,
    volume_name: &str,
) -> SkillReadiness {
    for model in &skill.required_models {
        let canonical = manifest.resolve(&model.filename);
        let remote_path = format!("{}/{}", model.model_type, canonical);

        let exists = crate::skills::prep::model_exists_on_instance(
            http,
            moss_endpoint,
            offering_fqn,
            volume_name,
            &remote_path,
        )
        .await;

        if !exists {
            return SkillReadiness {
                ready: false,
                reason: format!("missing model: {}", model.filename),
            };
        }
    }

    SkillReadiness {
        ready: true,
        reason: "all models present".into(),
    }
}

/// A model that's been downloaded and ingested into the cache.
pub struct CachedModel {
    /// The name the skill requested (from skill.json).
    pub original_name: String,
    /// The name in the cache (may differ due to dedup/rename).
    pub canonical_name: String,
    /// Model type directory (e.g., "checkpoints", "upscale_models").
    pub model_type: String,
    /// Full path to the cached file.
    pub cache_path: std::path::PathBuf,
}
