//! Skill provisioner (ORCH-0029).
//!
//! Coordinates the three-step dependency pipeline for a single
//! skill on a single ComfyUI instance:
//!
//! 1. **`ensure_cached`** — for each `required_model` of the skill,
//!    resolve through the cache manifest; download missing entries
//!    to the workspace; verify SHA-256; ingest into the cache with
//!    dedup. Returns the canonical cached file paths.
//!
//! 2. **`push_to_instance`** — for each cached model, HEAD the
//!    remote Moss volume; push the file via streaming PUT if
//!    missing.
//!
//! 3. **`check_instance_readiness`** — HEAD every required model on
//!    the instance without downloading or pushing. Used by the
//!    discovery fast path: when a ComfyUI instance comes up, the
//!    adapter calls this first. If `ready: true`, no provisioning
//!    job is submitted — the skill is already usable there.
//!
//! The provisioner is pure logic — the caller owns the HTTP client
//! and the cache paths. The bounded-concurrency queue (§commit 2.4)
//! sits on top.

use std::path::PathBuf;

use anyhow::{Context, Result};
use reqwest::Client;

use super::cache::{self, CachePaths, DependencyManifest, IngestResult};
use super::moss_volume;
use super::types::{ModelRef, SkillDefinition};

/// Readiness verdict for a skill on one instance.
#[derive(Debug, Clone)]
pub struct InstanceReadiness {
    pub ready: bool,
    pub reason: String,
}

/// A model that has been ingested into the cache and is ready to be
/// pushed to an instance.
#[derive(Debug, Clone)]
pub struct CachedModel {
    /// The name the skill asked for.
    pub original_name: String,
    /// The canonical name in the manifest (may differ after dedup).
    pub canonical_name: String,
    /// The model_type directory on the ComfyUI side (`checkpoints`,
    /// `loras`, `upscale_models`, `vae`, …).
    pub model_type: String,
    /// Absolute path to the cached file.
    pub cache_path: PathBuf,
}

/// Check whether every required model for `skill` is already on the
/// given ComfyUI instance, resolving through the cache manifest's
/// alias chain before issuing HEAD requests.
///
/// Fast path: no downloads, no pushes, just HEADs. Returns
/// `ready: true` only if ALL required models are present.
pub async fn check_instance_readiness(
    http: &Client,
    skill: &SkillDefinition,
    manifest: &DependencyManifest,
    moss_endpoint: &str,
    offering_fqn: &str,
    volume: &str,
) -> InstanceReadiness {
    if skill.required_models.is_empty() {
        return InstanceReadiness {
            ready: true,
            reason: "no required models".into(),
        };
    }

    for model in &skill.required_models {
        // Resolve through the alias chain so we HEAD the canonical
        // filename (which is what the provisioner would actually
        // push to the instance).
        let canonical = manifest.resolve(&model.filename);
        let remote_path = format!("{}/{}", model.model_type, canonical);
        let present = moss_volume::file_exists(http, moss_endpoint, offering_fqn, volume, &remote_path).await;
        if !present {
            return InstanceReadiness {
                ready: false,
                reason: format!("missing on instance: {}/{}", model.model_type, canonical),
            };
        }
    }

    InstanceReadiness {
        ready: true,
        reason: "all required models present".into(),
    }
}

/// Ensure every required model for `skill` is in the local cache.
///
/// For each model:
/// - If already in the cache (direct hit or alias resolution), skip.
/// - Otherwise stream the download to the workspace, verify the
///   SHA-256 if the skill declares one, and ingest into the cache
///   with 4-case dedup.
///
/// Models with no `url` are logged and skipped — the provisioner
/// cannot download them, but the skill may still work if the model
/// happens to already be on the instance via some other route.
///
/// Returns the list of cached models ready to be pushed to an
/// instance by [`push_to_instance`].
pub async fn ensure_cached(
    http: &Client,
    skill: &SkillDefinition,
    cache_paths: &CachePaths,
) -> Result<Vec<CachedModel>> {
    cache_paths.ensure_dirs().await?;

    let mut manifest = DependencyManifest::load(&cache_paths.manifest_path).await;
    let workspace = cache_paths.workspace_for_skill(skill.moniker.as_str());
    tokio::fs::create_dir_all(&workspace).await?;

    let mut cached_models = Vec::new();

    for model in &skill.required_models {
        // Already cached? Great, skip the download.
        let resolved = manifest.resolve(&model.filename);
        if manifest.files.contains_key(&resolved) {
            tracing::debug!(
                skill = skill.moniker.as_str(),
                model = %model.filename,
                canonical = %resolved,
                "provisioner: model already cached"
            );
            cached_models.push(CachedModel {
                original_name: model.filename.clone(),
                canonical_name: resolved.clone(),
                model_type: model.model_type.clone(),
                cache_path: cache_paths.file_path(&resolved),
            });
            continue;
        }

        // No URL to download from — emit a warning and skip. The
        // provisioner's job is coordination; the instance might
        // still have the model via some other path.
        let Some(url) = model.url.as_deref().filter(|u| !u.is_empty()) else {
            tracing::warn!(
                skill = skill.moniker.as_str(),
                model = %model.filename,
                "provisioner: model has no download URL, skipping"
            );
            continue;
        };

        // Download to the workspace, computing SHA-256 in-line.
        tracing::info!(
            skill = skill.moniker.as_str(),
            model = %model.filename,
            url = %url.split('?').next().unwrap_or(url),
            "provisioner: downloading missing model"
        );
        let ws_path = workspace.join(&model.filename);
        let (downloaded_path, checksum) = cache::stream_download(
            http,
            url,
            &ws_path,
            model.size_bytes,
            None,
        )
        .await
        .with_context(|| format!("download model {}", model.filename))?;

        // Verify checksum if the skill declared one.
        if let Some(expected) = &model.sha256 {
            let expected_full = if expected.starts_with("sha256:") {
                expected.clone()
            } else {
                format!("sha256:{expected}")
            };
            if !checksum.eq_ignore_ascii_case(&expected_full) {
                // Bad checksum: kill the partial file, bail.
                let _ = tokio::fs::remove_file(&downloaded_path).await;
                anyhow::bail!(
                    "checksum mismatch for {}: expected {}, got {}",
                    model.filename,
                    expected_full,
                    checksum
                );
            }
        }

        // Ingest with dedup.
        let result = cache::ingest_to_cache(
            &mut manifest,
            &cache_paths.provider_dir,
            &downloaded_path,
            &model.filename,
            &checksum,
        )
        .await
        .with_context(|| format!("ingest model {}", model.filename))?;

        let canonical_name = match &result {
            IngestResult::Added { canonical_name } => {
                tracing::info!(model = %model.filename, "provisioner: new model cached");
                canonical_name.clone()
            }
            IngestResult::AlreadyCached => {
                tracing::debug!(model = %model.filename, "provisioner: already cached (race)");
                model.filename.clone()
            }
            IngestResult::Aliased { canonical_name, alias_from } => {
                tracing::info!(
                    alias = %alias_from,
                    canonical = %canonical_name,
                    "provisioner: alias of existing cached file"
                );
                canonical_name.clone()
            }
            IngestResult::Renamed { canonical_name, original_name } => {
                tracing::info!(
                    original = %original_name,
                    stored_as = %canonical_name,
                    "provisioner: name collision, stored with suffix"
                );
                canonical_name.clone()
            }
        };

        cached_models.push(CachedModel {
            original_name: model.filename.clone(),
            canonical_name: canonical_name.clone(),
            model_type: model.model_type.clone(),
            cache_path: cache_paths.file_path(&canonical_name),
        });
    }

    manifest
        .save(&cache_paths.manifest_path)
        .await
        .context("save manifest after ingest")?;

    // Clean the workspace — everything has been moved into the cache.
    let _ = tokio::fs::remove_dir_all(&workspace).await;

    Ok(cached_models)
}

/// Push a set of cached models to a remote ComfyUI instance.
///
/// HEAD-first: only files that don't already exist on the instance
/// are uploaded. Streaming PUT with `Content-Length` set from local
/// file metadata; no buffering in memory.
pub async fn push_to_instance(
    http: &Client,
    cached_models: &[CachedModel],
    moss_endpoint: &str,
    offering_fqn: &str,
    volume: &str,
) -> Result<()> {
    for model in cached_models {
        let remote_path = format!("{}/{}", model.model_type, model.canonical_name);
        if moss_volume::file_exists(http, moss_endpoint, offering_fqn, volume, &remote_path).await {
            tracing::debug!(
                model = %model.canonical_name,
                "provisioner: model already on instance"
            );
            continue;
        }
        tracing::info!(
            model = %model.canonical_name,
            endpoint = %moss_endpoint,
            "provisioner: pushing model to instance"
        );
        moss_volume::push_file_streaming(
            http,
            moss_endpoint,
            offering_fqn,
            volume,
            &remote_path,
            &model.cache_path,
        )
        .await
        .with_context(|| format!("push {} to instance", model.canonical_name))?;
    }
    Ok(())
}

/// Turn a list of `ModelRef`s + cache manifest into the `CachedModel`
/// list needed by `push_to_instance` — without downloading anything.
///
/// Used by the fast path in discovery: when a new instance comes up
/// and the cache already has every required model, we can push them
/// to the instance without ever calling `ensure_cached` (which
/// would touch the manifest file unnecessarily).
pub fn resolve_cached(
    models: &[ModelRef],
    manifest: &DependencyManifest,
    cache_paths: &CachePaths,
) -> Vec<CachedModel> {
    let mut out = Vec::new();
    for model in models {
        let canonical = manifest.resolve(&model.filename);
        if !manifest.files.contains_key(&canonical) {
            continue;
        }
        out.push(CachedModel {
            original_name: model.filename.clone(),
            canonical_name: canonical.clone(),
            model_type: model.model_type.clone(),
            cache_path: cache_paths.file_path(&canonical),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn sample_models() -> Vec<ModelRef> {
        vec![
            ModelRef {
                filename: "RealESRGAN_x4plus.pth".into(),
                model_type: "upscale_models".into(),
                url: Some("https://example.com/RealESRGAN_x4plus.pth".into()),
                size_bytes: None,
                sha256: None,
                license: None,
                description: None,
            },
            ModelRef {
                filename: "missing.pth".into(),
                model_type: "upscale_models".into(),
                url: None,
                size_bytes: None,
                sha256: None,
                license: None,
                description: None,
            },
        ]
    }

    #[test]
    fn resolve_cached_filters_missing_and_follows_aliases() {
        let dir = tempfile::tempdir().unwrap();
        let paths = CachePaths::new(dir.path(), "comfyui");

        let mut manifest = DependencyManifest::default();
        manifest
            .files
            .insert("RealESRGAN_x4plus.pth".into(), "sha256:abc".into());
        // Add an alias to make sure `resolve_cached` follows it.
        manifest
            .aliases
            .insert("old-name.pth".into(), "RealESRGAN_x4plus.pth".into());

        let models = sample_models();
        let cached = resolve_cached(&models, &manifest, &paths);
        // Only the first model is cached; the second has no URL and
        // isn't in the manifest — it's silently dropped.
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].original_name, "RealESRGAN_x4plus.pth");
        assert_eq!(cached[0].canonical_name, "RealESRGAN_x4plus.pth");
        assert_eq!(cached[0].model_type, "upscale_models");
        assert!(cached[0].cache_path.ends_with("RealESRGAN_x4plus.pth"));

        // Resolve the alias directly.
        let aliased = vec![ModelRef {
            filename: "old-name.pth".into(),
            model_type: "upscale_models".into(),
            url: None,
            size_bytes: None,
            sha256: None,
            license: None,
            description: None,
        }];
        let cached = resolve_cached(&aliased, &manifest, &paths);
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].original_name, "old-name.pth");
        assert_eq!(cached[0].canonical_name, "RealESRGAN_x4plus.pth");

        // Silence the unused import warning in the test scope.
        let _: HashMap<String, String> = HashMap::new();
    }
}
