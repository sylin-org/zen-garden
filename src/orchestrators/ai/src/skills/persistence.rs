//! Skill persistence — three-tier backup/restore (ORCH-0025).
//!
//! Tier 1: Host filesystem (handled by Docker bind mount in start.bat)
//! Tier 2: Stone Moss storage (backup/restore via volume API)
//! Tier 3: ComfyUI instances (co-located with models)
//!
//! This module handles Tier 2 and Tier 3 operations.

use std::path::Path;

use anyhow::{Context, Result};
use reqwest::Client;

/// Subdirectory inside ComfyUI's model volume for skill definitions.
const SKILL_VOLUME_PREFIX: &str = "zen-garden/skills";

/// Volume name on ComfyUI instances.
const COMFYUI_VOLUME: &str = "comfyui-models";

// ── Tier 3: Push Skills to ComfyUI Instances ─────────────────

/// Push a skill's definition files to a ComfyUI instance.
///
/// Stores skill.json + workflow templates under
/// `comfyui-models/zen-garden/skills/{skill-name}/` so the instance
/// becomes a self-describing recovery source.
pub async fn push_skill_to_instance(
    http: &Client,
    moss_endpoint: &str,
    offering_fqn: &str,
    skill_dir: &Path,
    skill_name: &str,
) -> Result<()> {
    let entries = list_skill_files(skill_dir).await?;
    if entries.is_empty() {
        return Ok(());
    }

    for (filename, contents) in &entries {
        let remote_path = format!("{SKILL_VOLUME_PREFIX}/{skill_name}/{filename}");
        push_file(http, moss_endpoint, offering_fqn, COMFYUI_VOLUME, &remote_path, contents).await
            .with_context(|| format!("push {filename} to instance"))?;
    }

    tracing::debug!(
        skill = skill_name,
        files = entries.len(),
        "pushed skill definition to ComfyUI instance"
    );

    Ok(())
}

/// Remove a skill's definition files from a ComfyUI instance.
pub async fn remove_skill_from_instance(
    http: &Client,
    moss_endpoint: &str,
    offering_fqn: &str,
    skill_name: &str,
) -> Result<()> {
    // Delete the skill directory (Moss handles recursive delete)
    let remote_path = format!("{SKILL_VOLUME_PREFIX}/{skill_name}");
    let url = format!(
        "{moss_endpoint}/api/v1/stone/offerings/{offering_fqn}/volumes/{COMFYUI_VOLUME}/{remote_path}"
    );

    let _ = http.delete(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await;

    Ok(())
}

// ── Tier 3: Recover Skills from ComfyUI Instances ────────────

/// Scan a ComfyUI instance for skill definitions and restore missing ones locally.
///
/// Returns the number of skills recovered.
pub async fn recover_skills_from_instance(
    http: &Client,
    moss_endpoint: &str,
    offering_fqn: &str,
    local_skills_dir: &Path,
    provider: &str,
) -> Result<usize> {
    // List skill directories on the instance
    let list_url = format!(
        "{moss_endpoint}/api/v1/stone/offerings/{offering_fqn}/volumes/{COMFYUI_VOLUME}/{SKILL_VOLUME_PREFIX}"
    );

    let resp = http.get(&list_url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await;

    let resp = match resp {
        Ok(r) if r.status().is_success() => r,
        _ => return Ok(0), // instance doesn't have skills — not an error
    };

    let body: serde_json::Value = resp.json().await.unwrap_or_default();
    let entries = body.get("entries")
        .or(body.get("data"))
        .and_then(|v| v.as_array());

    let Some(entries) = entries else {
        return Ok(0);
    };

    let mut recovered = 0;

    for entry in entries {
        let name = match entry.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => continue,
        };

        let local_dir = local_skills_dir.join(provider).join(name);
        let local_skill = local_dir.join("skill.json");

        // Skip if already exists locally
        if local_skill.exists() {
            continue;
        }

        // Pull skill.json from instance
        let skill_url = format!(
            "{moss_endpoint}/api/v1/stone/offerings/{offering_fqn}/volumes/{COMFYUI_VOLUME}/{SKILL_VOLUME_PREFIX}/{name}/skill.json"
        );

        let skill_resp = http.get(&skill_url)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await;

        if let Ok(resp) = skill_resp {
            if resp.status().is_success() {
                if let Ok(bytes) = resp.bytes().await {
                    // Validate it's valid JSON before writing
                    if serde_json::from_slice::<serde_json::Value>(&bytes).is_ok() {
                        tokio::fs::create_dir_all(&local_dir).await?;
                        tokio::fs::write(&local_skill, &bytes).await?;

                        // Also pull workflow templates
                        pull_workflow_files(http, moss_endpoint, offering_fqn, name, &local_dir).await;

                        tracing::info!(
                            skill = name,
                            source = "comfyui-instance",
                            "recovered skill definition"
                        );
                        recovered += 1;
                    }
                }
            }
        }
    }

    Ok(recovered)
}

/// Pull workflow template files for a skill from an instance.
async fn pull_workflow_files(
    http: &Client,
    moss_endpoint: &str,
    offering_fqn: &str,
    skill_name: &str,
    local_dir: &Path,
) {
    // List files in the skill directory on the instance
    let list_url = format!(
        "{moss_endpoint}/api/v1/stone/offerings/{offering_fqn}/volumes/{COMFYUI_VOLUME}/{SKILL_VOLUME_PREFIX}/{skill_name}"
    );

    let resp = match http.get(&list_url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => r,
        _ => return,
    };

    let body: serde_json::Value = resp.json().await.unwrap_or_default();
    let entries = body.get("entries")
        .or(body.get("data"))
        .and_then(|v| v.as_array());

    let Some(entries) = entries else { return };

    for entry in entries {
        let filename = match entry.get("name").and_then(|v| v.as_str()) {
            Some(n) if n.ends_with(".json") && n != "skill.json" => n,
            _ => continue,
        };

        let file_url = format!(
            "{moss_endpoint}/api/v1/stone/offerings/{offering_fqn}/volumes/{COMFYUI_VOLUME}/{SKILL_VOLUME_PREFIX}/{skill_name}/{filename}"
        );

        if let Ok(resp) = http.get(&file_url)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
        {
            if resp.status().is_success() {
                if let Ok(bytes) = resp.bytes().await {
                    let _ = tokio::fs::write(local_dir.join(filename), &bytes).await;
                }
            }
        }
    }
}

// ── Tier 3: Reverse Provisioning (Model Recovery) ────────────

/// Pull a model file from a ComfyUI instance back to local cache.
///
/// Streams to disk — never buffers in memory.
pub async fn pull_model_from_instance(
    http: &Client,
    moss_endpoint: &str,
    offering_fqn: &str,
    model_type: &str,
    filename: &str,
    local_path: &Path,
) -> Result<()> {
    let remote_path = format!("{model_type}/{filename}");
    let url = format!(
        "{moss_endpoint}/api/v1/stone/offerings/{offering_fqn}/volumes/{COMFYUI_VOLUME}/{remote_path}"
    );

    let resp = http.get(&url)
        .send()
        .await
        .with_context(|| format!("GET model from instance: {url}"))?;

    if !resp.status().is_success() {
        anyhow::bail!("instance returned HTTP {} for model {filename}", resp.status());
    }

    // Stream to disk
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::File::create(local_path).await
        .with_context(|| format!("create cache file: {}", local_path.display()))?;

    let mut stream = resp.bytes_stream();
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("read model stream")?;
        file.write_all(&chunk).await.context("write model chunk")?;
    }

    file.flush().await?;

    tracing::info!(
        model = filename,
        path = %local_path.display(),
        "recovered model from instance"
    );

    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────

/// List all files in a skill directory (skill.json + workflow templates).
async fn list_skill_files(skill_dir: &Path) -> Result<Vec<(String, Vec<u8>)>> {
    let mut files = Vec::new();
    let mut entries = tokio::fs::read_dir(skill_dir).await
        .with_context(|| format!("read skill dir: {}", skill_dir.display()))?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            let filename = entry.file_name().to_string_lossy().to_string();
            // Skip debug files
            if filename.starts_with('_') {
                continue;
            }
            let contents = tokio::fs::read(&path).await?;
            files.push((filename, contents));
        }
    }

    Ok(files)
}

/// Push a small file to a Moss volume.
async fn push_file(
    http: &Client,
    moss_endpoint: &str,
    offering_fqn: &str,
    volume: &str,
    remote_path: &str,
    contents: &[u8],
) -> Result<()> {
    let url = format!(
        "{moss_endpoint}/api/v1/stone/offerings/{offering_fqn}/volumes/{volume}/{remote_path}"
    );

    let resp = http.put(&url)
        .header(reqwest::header::CONTENT_LENGTH, contents.len())
        .body(contents.to_vec())
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .with_context(|| format!("PUT file to: {url}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("push file failed HTTP {status}: {text}");
    }

    Ok(())
}
