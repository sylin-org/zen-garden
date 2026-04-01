//! Skill preparation — download models, push to instances, mark live.
//!
//! The provisioning state machine:
//! 1. INITIALIZING: Download required models to orchestrator's local cache
//! 2. PROVISIONING: Push cached models to ComfyUI instances via Moss volume API
//! 3. LIVE: At least one instance has all models — skill is available
//!
//! This module is called by the discovery task after skills are enumerated.

use anyhow::{Context, Result};
use reqwest::Client;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::domain::skill::SkillDefinition;

// ── Skill State ────────────────────────────────────────────────

/// Provisioning state for a skill across all instances.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillState {
    /// Skill discovered but models not cached locally yet.
    Discovered,
    /// Models are being downloaded to orchestrator cache.
    Initializing,
    /// Models cached locally, pushing to instances.
    Provisioning,
    /// At least one instance is fully provisioned.
    Live,
    /// Some instances provisioned, others pending.
    Degraded,
    /// Download or push failed.
    Failed(String),
}

/// Per-instance provisioning status.
#[derive(Debug, Clone)]
pub struct InstanceReadiness {
    pub endpoint: String,
    pub moss_endpoint: String,
    pub fqn: String,
    pub ready: bool,
    pub missing_models: Vec<String>,
}

/// Tracks provisioning across all skills and instances.
#[derive(Debug, Default)]
pub struct ProvisioningTracker {
    pub skill_states: HashMap<String, SkillState>,
    pub instance_readiness: HashMap<String, HashMap<String, InstanceReadiness>>,
}

impl ProvisioningTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_live(&self, skill_name: &str) -> bool {
        matches!(
            self.skill_states.get(skill_name),
            Some(SkillState::Live) | Some(SkillState::Degraded)
        )
    }
}

// ── Cache Management ───────────────────────────────────────────

/// Ensure a model file is present in the local cache.
///
/// Downloads from the upstream URL if not already cached.
/// Returns the local cache path.
pub async fn ensure_cached(
    http: &Client,
    cache_dir: &Path,
    model_type: &str,
    filename: &str,
    url: &str,
) -> Result<PathBuf> {
    let dir = cache_dir.join(model_type);
    let path = dir.join(filename);

    if path.exists() {
        tracing::debug!(filename, "model already cached");
        return Ok(path);
    }

    tokio::fs::create_dir_all(&dir)
        .await
        .with_context(|| format!("create cache dir: {}", dir.display()))?;

    tracing::info!(filename, url, "downloading model to cache");

    // No global timeout — stream to disk as bytes arrive.
    // Only the initial connection has a timeout (from the Client config).
    let resp = http
        .get(url)
        .send()
        .await
        .with_context(|| format!("download model: {url}"))?;

    if !resp.status().is_success() {
        anyhow::bail!("download failed HTTP {}: {}", resp.status(), url);
    }

    let total_bytes = resp.content_length();

    // Stream to temp file — no full-file buffering in RAM
    let tmp_path = path.with_extension("tmp");
    let mut file = tokio::fs::File::create(&tmp_path)
        .await
        .with_context(|| format!("create temp file: {}", tmp_path.display()))?;

    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut last_log = std::time::Instant::now();

    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.with_context(|| format!("read chunk from: {url}"))?;
        file.write_all(&chunk).await.with_context(|| "write chunk to cache")?;
        downloaded += chunk.len() as u64;

        // Log progress every 5 seconds
        if last_log.elapsed() > std::time::Duration::from_secs(5) {
            if let Some(total) = total_bytes {
                let pct = (downloaded as f64 / total as f64 * 100.0) as u32;
                tracing::info!(filename, downloaded, total, pct, "download progress");
            } else {
                tracing::info!(filename, downloaded, "download progress");
            }
            last_log = std::time::Instant::now();
        }
    }

    file.flush().await?;
    drop(file);

    // Atomic rename
    tokio::fs::rename(&tmp_path, &path)
        .await
        .with_context(|| format!("rename cache file: {}", path.display()))?;

    tracing::info!(filename, bytes = downloaded, "model cached");

    Ok(path)
}

// ── Instance Provisioning ──────────────────────────────────────

/// Check if a model exists on a remote instance via Moss HEAD.
pub async fn model_exists_on_instance(
    http: &Client,
    moss_endpoint: &str,
    fqn: &str,
    volume: &str,
    model_path: &str,
) -> bool {
    let url = format!(
        "{moss_endpoint}/api/v1/stone/offerings/{fqn}/volumes/{volume}/{model_path}"
    );

    match http
        .head(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

/// Push a cached model file to a remote instance via Moss PUT.
///
/// Streams from disk — never loads the full file into memory.
pub async fn push_model_to_instance(
    http: &Client,
    moss_endpoint: &str,
    fqn: &str,
    volume: &str,
    model_path: &str,
    local_path: &Path,
) -> Result<()> {
    let file_size = tokio::fs::metadata(local_path)
        .await
        .with_context(|| format!("stat cached model: {}", local_path.display()))?
        .len();

    let url = format!(
        "{moss_endpoint}/api/v1/stone/offerings/{fqn}/volumes/{volume}/{model_path}"
    );

    tracing::info!(
        url = %url,
        bytes = file_size,
        "streaming model to instance"
    );

    let file = tokio::fs::File::open(local_path)
        .await
        .with_context(|| format!("open cached model: {}", local_path.display()))?;

    let stream = tokio_util::io::ReaderStream::new(file);
    let body = reqwest::Body::wrap_stream(stream);

    // No global timeout — large files need sustained throughput, not a wall clock.
    // The underlying TCP will detect dead connections via keepalive.
    let resp = http
        .put(&url)
        .header(reqwest::header::CONTENT_LENGTH, file_size)
        .body(body)
        .send()
        .await
        .with_context(|| format!("PUT model to: {url}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("push model failed HTTP {status}: {text}");
    }

    tracing::info!(
        model_path = %model_path,
        bytes = file_size,
        "model pushed to instance"
    );

    Ok(())
}

// ── Full Provisioning Cycle ────────────────────────────────────

/// Run the full provisioning cycle for a skill.
///
/// 1. Download missing models to local cache
/// 2. For each ComfyUI instance, check readiness and push missing models
/// 3. Return whether at least one instance is fully provisioned
/// Instance info for provisioning: (comfyui_endpoint, moss_endpoint, fqn, vram_mb)
pub type InstanceTarget = (String, String, String, u64);

pub async fn provision_skill(
    http: &Client,
    cache_dir: &Path,
    skill: &SkillDefinition,
    instances: &[InstanceTarget],
) -> Result<ProvisionResult> {
    // Aggregate all recommended models across all skill types
    let mut recommended: Vec<super::builtin::RecommendedModel> = Vec::new();
    recommended.extend(super::builtin::recommended_upscale_models());
    recommended.extend(super::builtin::recommended_checkpoint_models());

    // 1. Ensure all required models are cached locally
    let mut cached_models: HashMap<String, PathBuf> = HashMap::new();

    for model_ref in &skill.required_models {
        // Find download URL from recommended list
        let rec = recommended
            .iter()
            .find(|r| r.filename == model_ref.filename);

        if let Some(rec) = rec {
            match ensure_cached(http, cache_dir, &rec.model_type, &rec.filename, &rec.url).await {
                Ok(path) => {
                    cached_models.insert(rec.filename.clone(), path);
                }
                Err(e) => {
                    tracing::warn!(
                        model = %rec.filename,
                        error = %e,
                        "failed to cache model"
                    );
                }
            }
        } else {
            tracing::debug!(
                filename = %model_ref.filename,
                "model not in recommended list — skipping download"
            );
        }
    }

    if cached_models.is_empty() && !skill.required_models.is_empty() {
        return Ok(ProvisionResult {
            live_instances: 0,
            total_instances: instances.len(),
            state: SkillState::Failed("no models could be cached".into()),
        });
    }

    // 2. Push to each instance (skip those with insufficient VRAM)
    let mut live_count = 0;
    let mut skipped_vram = 0;

    for (comfyui_endpoint, moss_endpoint, fqn, instance_vram_mb) in instances {
        if *instance_vram_mb > 0 && *instance_vram_mb < skill.vram_mb {
            tracing::info!(
                instance = %comfyui_endpoint,
                vram_mb = instance_vram_mb,
                required_mb = skill.vram_mb,
                "skipping instance — insufficient VRAM for skill"
            );
            skipped_vram += 1;
            continue;
        }

        let mut all_present = true;

        for model_ref in &skill.required_models {
            let model_path = format!("{}/{}", model_ref.model_type, model_ref.filename);

            // Check if already present
            let volume = format!("comfyui-models");
            if model_exists_on_instance(http, moss_endpoint, fqn, &volume, &model_path).await {
                continue;
            }

            // Push from cache
            if let Some(local_path) = cached_models.get(&model_ref.filename) {
                match push_model_to_instance(http, moss_endpoint, fqn, &volume, &model_path, local_path).await {
                    Ok(()) => {
                        tracing::info!(
                            model = %model_ref.filename,
                            instance = %comfyui_endpoint,
                            "model pushed to instance"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            model = %model_ref.filename,
                            instance = %comfyui_endpoint,
                            error = %e,
                            "failed to push model"
                        );
                        all_present = false;
                    }
                }
            } else {
                all_present = false;
            }
        }

        if all_present {
            live_count += 1;
        }
    }

    let state = if live_count == instances.len() {
        SkillState::Live
    } else if live_count > 0 {
        SkillState::Degraded
    } else {
        SkillState::Provisioning
    };

    Ok(ProvisionResult {
        live_instances: live_count,
        total_instances: instances.len(),
        state,
    })
}

/// Result of a provisioning cycle.
#[derive(Debug)]
pub struct ProvisionResult {
    pub live_instances: usize,
    pub total_instances: usize,
    pub state: SkillState,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_state_is_live_check() {
        let mut tracker = ProvisioningTracker::new();
        assert!(!tracker.is_live("image.upscale"));

        tracker.skill_states.insert("image.upscale".into(), SkillState::Live);
        assert!(tracker.is_live("image.upscale"));

        tracker.skill_states.insert("image.upscale".into(), SkillState::Degraded);
        assert!(tracker.is_live("image.upscale"));

        tracker.skill_states.insert("image.upscale".into(), SkillState::Provisioning);
        assert!(!tracker.is_live("image.upscale"));
    }

    #[test]
    fn provision_result_states() {
        let r1 = ProvisionResult { live_instances: 2, total_instances: 2, state: SkillState::Live };
        assert_eq!(r1.state, SkillState::Live);

        let r2 = ProvisionResult { live_instances: 1, total_instances: 2, state: SkillState::Degraded };
        assert_eq!(r2.state, SkillState::Degraded);

        let r3 = ProvisionResult { live_instances: 0, total_instances: 2, state: SkillState::Provisioning };
        assert_eq!(r3.state, SkillState::Provisioning);
    }
}
