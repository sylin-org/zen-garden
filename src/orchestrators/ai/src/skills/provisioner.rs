//! Skill provisioner — orchestrator-side dependency coordination (ORCH-0022).
//!
//! The orchestrator knows WHAT is needed (from skill.json). The provider knows
//! HOW to get it. This module coordinates:
//!
//! 1. For each skill's required models → download to workspace → checksum → ingest to cache
//! 2. For each instance → push cached models that are missing
//! 3. Update skill readiness based on instance state

use anyhow::{Context, Result};

use crate::domain::skill::{SkillDefinition, SkillReadiness};
use crate::skills::cache::{self, CachePaths, DependencyManifest, IngestResult};

/// Optional event emitter for provisioning progress.
pub type EventEmitter = std::sync::Arc<dyn Fn(&str, &str) + Send + Sync>;

/// Ensure all required models for a skill are in the local cache.
///
/// Downloads missing models to the workspace, computes checksums, and
/// ingests into the cache with dedup. Returns the list of canonical
/// filenames in the cache (after alias resolution).
///
/// The optional `event_tx` emits SSE dashboard events with download progress.
pub async fn ensure_cached(
    http: &reqwest::Client,
    skill: &SkillDefinition,
    cache_paths: &CachePaths,
    event_tx: Option<EventEmitter>,
    secrets: Option<&crate::infra::secrets::SecretsStore>,
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

        // Download URL is required — append auth tokens for known services
        let raw_url = model.url.as_deref().unwrap_or("");
        if raw_url.is_empty() {
            tracing::warn!(
                model = %model.filename,
                skill = %skill.name,
                "model has no download URL — skipping"
            );
            continue;
        }

        // Check if auth is needed but not configured — fail fast with clear message
        if let Some(missing_key) = requires_auth_key(raw_url, secrets).await {
            anyhow::bail!(
                "model '{}' requires a {} API key to download. \
                 Set it in Dashboard → Secrets before provisioning.",
                model.filename, missing_key
            );
        }

        // Append auth token for known services
        let url = inject_auth_token(raw_url, secrets).await;

        // Download to workspace
        let ws_path = workspace.join(&model.filename);
        tracing::info!(
            model = %model.filename,
            skill = %skill.name,
            url = %raw_url,
            "downloading model dependency"
        );

        // Build progress callback for SSE events
        let progress_cb: Option<cache::ProgressFn> = event_tx.as_ref().map(|tx| {
            let tx = tx.clone();
            let skill_name = skill.name.clone();
            let model_name = model.filename.clone();
            Box::new(move |downloaded: u64, total: Option<u64>| {
                let data = serde_json::json!({
                    "skill": skill_name,
                    "model": model_name,
                    "downloaded_bytes": downloaded,
                    "total_bytes": total,
                });
                tx("skill.provisioning", &data.to_string());
            }) as cache::ProgressFn
        });

        let (downloaded_path, checksum) = cache::stream_download(
            http,
            &url,
            &ws_path,
            model.size_bytes,
            progress_cb,
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
            if !checksum.eq_ignore_ascii_case(&expected_full) {
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

/// Append auth tokens to download URLs for known services.
/// Check if a download URL requires authentication that isn't configured.
/// Returns the provider name if auth is needed but missing.
async fn requires_auth_key(
    url: &str,
    secrets: Option<&crate::infra::secrets::SecretsStore>,
) -> Option<&'static str> {
    let secrets = secrets?;

    // CivitAI download URLs always need a token
    if url.contains("civitai.com/api/download") {
        if secrets.get(crate::infra::secrets::KEY_CIVITAI).await.is_none() {
            return Some("CivitAI");
        }
    }

    // HuggingFace gated models need a token
    // (non-gated models work without auth, so we can't check statically —
    // the 401 error handling in cache.rs catches these at download time)

    None
}

async fn inject_auth_token(
    url: &str,
    secrets: Option<&crate::infra::secrets::SecretsStore>,
) -> String {
    let secrets = match secrets {
        Some(s) => s,
        None => return url.to_string(),
    };

    // CivitAI: append ?token={key}
    if url.contains("civitai.com") {
        if let Some(token) = secrets.get(crate::infra::secrets::KEY_CIVITAI).await {
            let separator = if url.contains('?') { "&" } else { "?" };
            return format!("{url}{separator}token={token}");
        }
    }

    // HuggingFace: add Authorization header is better, but for URL-based downloads,
    // HF also supports ?token= for resolve URLs
    if url.contains("huggingface.co") {
        if let Some(token) = secrets.get(crate::infra::secrets::KEY_HUGGINGFACE).await {
            let separator = if url.contains('?') { "&" } else { "?" };
            return format!("{url}{separator}token={token}");
        }
    }

    url.to_string()
}
