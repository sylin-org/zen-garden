//! Skill recovery — restore skills from ComfyUI instances on empty startup (ORCH-0025).
//!
//! Recovery cascade:
//! 1. Local /data/skills/ has content? → normal boot (no recovery needed)
//! 2. Scan ComfyUI instances for zen-garden/skills/ → pull definitions
//!
//! Called once at startup, before the discovery loop begins.

use std::path::Path;

use anyhow::Result;

/// Check if local skills directory is empty (no non-embedded skills).
///
/// Returns true if recovery is needed — the directory is empty or only
/// contains embedded skills that would be re-seeded anyway.
pub async fn needs_recovery(skills_dir: &Path, provider: &str) -> bool {
    let provider_dir = skills_dir.join(provider);
    if !provider_dir.exists() {
        return true;
    }

    let mut entries = match tokio::fs::read_dir(&provider_dir).await {
        Ok(e) => e,
        Err(_) => return true,
    };

    // Count non-embedded skill directories (those not in the embedded list)
    let embedded = crate::skills::loader::embedded_monikers();
    let mut user_skills = 0;

    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if !embedded.contains(&name.as_str()) {
            user_skills += 1;
        }
    }

    user_skills == 0
}

/// Attempt to recover skills from any reachable ComfyUI instance.
///
/// Scans all known ComfyUI endpoints in the topology for
/// `zen-garden/skills/` directories and pulls missing skills.
pub async fn recover_from_instances(
    http: &reqwest::Client,
    skills_dir: &Path,
    moss_endpoint: &str,
    offering_fqn: &str,
    provider: &str,
) -> Result<usize> {
    tracing::info!("recovery: scanning ComfyUI instances for skill definitions");

    let recovered = super::persistence::recover_skills_from_instance(
        http,
        moss_endpoint,
        offering_fqn,
        skills_dir,
        provider,
    ).await?;

    if recovered > 0 {
        tracing::info!(
            recovered,
            "recovery: restored skill definitions from ComfyUI instance"
        );
    } else {
        tracing::info!("recovery: no skill definitions found on instances");
    }

    Ok(recovered)
}

/// Recover cached models from ComfyUI instances.
///
/// For each skill's required_models, check if the model exists on any instance
/// but not in the local cache. If so, pull it back.
pub async fn recover_model_cache(
    http: &reqwest::Client,
    skills_dir: &Path,
    cache_dir: &Path,
    moss_endpoint: &str,
    offering_fqn: &str,
    provider: &str,
) -> Result<usize> {
    let provider_dir = skills_dir.join(provider);
    if !provider_dir.exists() {
        return Ok(0);
    }

    let mut recovered = 0;

    // Load all skill definitions
    let skills = crate::skills::loader::load_skills(skills_dir).await;

    for skill in &skills {
        for model in &skill.required_models {
            let cache_path = cache_dir.join(&model.filename);

            // Skip if already cached locally
            if cache_path.exists() {
                continue;
            }

            // Try to pull from instance
            let result = super::persistence::pull_model_from_instance(
                http,
                moss_endpoint,
                offering_fqn,
                &model.model_type,
                &model.filename,
                &cache_path,
            ).await;

            match result {
                Ok(()) => {
                    recovered += 1;
                    tracing::info!(
                        model = %model.filename,
                        skill = %skill.name,
                        "recovered model from instance to local cache"
                    );
                }
                Err(e) => {
                    // Model not on instance — will need to download from source
                    tracing::debug!(
                        model = %model.filename,
                        error = %e,
                        "model not available on instance for recovery"
                    );
                }
            }
        }
    }

    Ok(recovered)
}
