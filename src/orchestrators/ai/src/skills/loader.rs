//! Skill loader — scan disk, parse skill.json, resolve workflows.
//!
//! The skill repository lives at `{data_dir}/skills/{provider}/{moniker}/`.
//! Each directory contains a `skill.json` and one or more workflow template
//! JSON files. The loader reads everything from disk — no hardcoded skills.
//!
//! Embedded skills (compiled into the binary) are seeded to disk on first run.
//! After seeding, the disk is the sole source of truth.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tokio::fs;

use crate::domain::skill::SkillDefinition;

/// Scan the skills directory and load all valid skill definitions.
///
/// For each `{skills_dir}/{provider}/{moniker}/skill.json`:
/// 1. Parse the skill definition
/// 2. Resolve all referenced workflow templates from the same directory
/// 3. Validate: all workflows exist, required fields present
/// 4. Return the loaded skill (or log a warning and skip)
pub async fn load_skills(skills_dir: &Path) -> Vec<SkillDefinition> {
    let mut skills = Vec::new();

    // Scan: skills_dir / {provider} / {moniker} / skill.json
    let providers = match read_subdirs(skills_dir).await {
        Ok(dirs) => dirs,
        Err(e) => {
            tracing::warn!(
                dir = %skills_dir.display(),
                error = %e,
                "cannot read skills directory"
            );
            return skills;
        }
    };

    for provider_dir in providers {
        let provider_name = dir_name(&provider_dir);
        let monikers = match read_subdirs(&provider_dir).await {
            Ok(dirs) => dirs,
            Err(_) => continue,
        };

        for moniker_dir in monikers {
            let moniker_name = dir_name(&moniker_dir);
            let skill_path = moniker_dir.join("skill.json");

            match load_single_skill(&skill_path, &moniker_dir).await {
                Ok(skill) => {
                    skills.push(skill);
                }
                Err(e) => {
                    // Only warn for non-draft skills (drafts are expected to fail)
                    if !e.to_string().contains("draft") {
                        tracing::warn!(
                            provider = %provider_name,
                            moniker = %moniker_name,
                            error = %e,
                            "skipping skill — failed to load"
                        );
                    }
                }
            }
        }
    }

    tracing::debug!(count = skills.len(), "skill repository scanned");
    skills
}

/// Load a single skill definition from a skill.json file.
async fn load_single_skill(skill_path: &Path, skill_dir: &Path) -> Result<SkillDefinition> {
    // Read and parse skill.json
    let json_str = fs::read_to_string(skill_path)
        .await
        .with_context(|| format!("read {}", skill_path.display()))?;

    let raw: RawSkillDefinition = serde_json::from_str(&json_str)
        .with_context(|| format!("parse {}", skill_path.display()))?;

    // Skip draft skills — they're managed by the CRUD API, not the loader
    if raw.draft {
        anyhow::bail!("draft skill (not published)");
    }

    // Resolve workflow templates from the same directory
    let mut workflows = HashMap::new();

    // Collect all workflow names referenced in mappings (workflow selector options)
    let mut workflow_names: Vec<String> = vec![raw.default_workflow.clone()];
    for mapping in &raw.mappings {
        if let serde_json::Value::Object(map) = mapping {
            if map.get("field").and_then(|v| v.as_str()) == Some("workflow") {
                if let Some(options) = map.get("options").and_then(|v| v.as_array()) {
                    for opt in options {
                        if let Some(val) = opt.get("value").and_then(|v| v.as_str()) {
                            workflow_names.push(val.to_string());
                        }
                    }
                }
            }
        }
    }
    workflow_names.sort();
    workflow_names.dedup();

    // Load each referenced workflow
    for wf_name in &workflow_names {
        let wf_path = skill_dir.join(format!("{wf_name}.json"));
        let wf_str = fs::read_to_string(&wf_path)
            .await
            .with_context(|| format!("workflow '{}' not found: {}", wf_name, wf_path.display()))?;
        let wf_json: serde_json::Value = serde_json::from_str(&wf_str)
            .with_context(|| format!("parse workflow: {}", wf_path.display()))?;
        workflows.insert(wf_name.clone(), wf_json);
    }

    // Parse the Mermaid diagram from the default workflow
    let diagram = workflows
        .get(&raw.default_workflow)
        .and_then(|wf| crate::skills::parser::parse_workflow(wf).ok())
        .map(|parsed| parsed.diagram);

    // Deserialize mappings from raw JSON into typed SkillMapping
    let mappings: Vec<crate::domain::skill::SkillMapping> = raw
        .mappings
        .iter()
        .map(|v| serde_json::from_value(v.clone()))
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("parse mappings in {}", skill_path.display()))?;

    // Deserialize content slots
    let content_slots: Vec<crate::domain::skill::ContentSlot> = raw
        .content_slots
        .iter()
        .map(|v| serde_json::from_value(v.clone()))
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("parse content_slots in {}", skill_path.display()))?;

    // Deserialize required models
    let required_models: Vec<crate::domain::skill::ModelRef> = raw
        .required_models
        .iter()
        .map(|v| serde_json::from_value(v.clone()))
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("parse required_models in {}", skill_path.display()))?;

    // Parse capability
    let capability = crate::domain::types::Capability::ALL
        .iter()
        .find(|c| c.as_str() == raw.capability)
        .copied()
        .with_context(|| format!("unknown capability: {}", raw.capability))?;

    // Parse provider kind — try serde (snake_case like "comfy_ui") then as_str (like "comfyui")
    let provider_kind = serde_json::from_value::<crate::domain::types::OfferingKind>(
        serde_json::Value::String(raw.provider_kind.clone()),
    )
    .or_else(|_| {
        crate::domain::types::OfferingKind::from_str(&raw.provider_kind)
            .ok_or_else(|| anyhow::anyhow!("unknown provider_kind: {}", raw.provider_kind))
    })?;

    Ok(SkillDefinition {
        name: raw.name,
        display_name: raw.display_name,
        capability,
        description: raw.description,
        provider_kind,
        vram_mb: raw.vram_mb,
        content_slots,
        mappings,
        diagram,
        preview_url: None, // Not stored in loaded skills; management API reads from skill.json
        required_models,
        default_workflow: raw.default_workflow,
        workflows,
    })
}

/// Raw skill definition — deserialized from skill.json with minimal typing.
/// Mappings, content_slots, and required_models are kept as raw JSON values
/// so the loader can handle schema evolution gracefully.
#[derive(Debug, serde::Deserialize)]
struct RawSkillDefinition {
    #[allow(dead_code)]
    version: u32,
    /// Draft skills are ignored by the loader.
    #[serde(default)]
    draft: bool,
    name: String,
    display_name: String,
    capability: String,
    description: String,
    provider_kind: String,
    vram_mb: u64,
    default_workflow: String,
    content_slots: Vec<serde_json::Value>,
    mappings: Vec<serde_json::Value>,
    #[serde(default)]
    required_models: Vec<serde_json::Value>,
}

/// Read immediate subdirectories of a path.
pub async fn read_subdirs(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut entries = fs::read_dir(dir)
        .await
        .with_context(|| format!("read_dir: {}", dir.display()))?;

    let mut dirs = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        if entry.file_type().await?.is_dir() {
            dirs.push(entry.path());
        }
    }
    dirs.sort();
    Ok(dirs)
}

/// Extract the last component of a path as a string.
fn dir_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}

// ── Embedded Skill Seeding ────────────────────────────────────

/// Seed embedded skill definitions to disk if not already present.
///
/// Each embedded skill is a (provider, moniker, files) tuple where files
/// is a list of (filename, content) pairs. The seeder writes them to
/// `{skills_dir}/{provider}/{moniker}/{filename}`.
pub async fn seed_embedded_skills(skills_dir: &Path) {
    for (provider, moniker, files) in embedded_skills() {
        let skill_dir = skills_dir.join(provider).join(moniker);
        let skill_json = skill_dir.join("skill.json");

        if skill_json.exists() {
            // Check version: only overwrite if embedded is newer
            if let Ok(existing) = fs::read_to_string(&skill_json).await {
                if let Ok(existing_raw) = serde_json::from_str::<serde_json::Value>(&existing) {
                    let existing_version = existing_raw.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
                    // Find the embedded skill.json content
                    let embedded_content = files.iter().find(|(name, _)| *name == "skill.json").map(|(_, c)| *c).unwrap_or("");
                    if let Ok(embedded_raw) = serde_json::from_str::<serde_json::Value>(embedded_content) {
                        let embedded_version = embedded_raw.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
                        if embedded_version <= existing_version {
                            tracing::debug!(provider, moniker, "embedded skill already seeded (version match)");
                            continue;
                        }
                        tracing::info!(provider, moniker, embedded_version, existing_version, "updating embedded skill (newer version)");
                    }
                }
            }
        }

        // Create directory and write files
        if let Err(e) = fs::create_dir_all(&skill_dir).await {
            tracing::warn!(provider, moniker, error = %e, "failed to create skill directory");
            continue;
        }

        let mut seeded = true;
        for (filename, content) in &files {
            let path = skill_dir.join(filename);
            if let Err(e) = fs::write(&path, content).await {
                tracing::warn!(provider, moniker, filename, error = %e, "failed to write skill file");
                seeded = false;
            }
        }

        if seeded {
            tracing::info!(provider, moniker, files = files.len(), "seeded embedded skill to disk");
        }
    }
}

/// Returns embedded skill definitions compiled into the binary.
fn embedded_skills() -> Vec<(&'static str, &'static str, Vec<(&'static str, &'static str)>)> {
    vec![
        ("comfyui", "upscale", vec![
            ("skill.json", include_str!("definitions/comfyui/upscale/skill.json")),
            ("upscale_2x.json", include_str!("definitions/comfyui/upscale/upscale_2x.json")),
            ("upscale_4x.json", include_str!("definitions/comfyui/upscale/upscale_4x.json")),
            ("upscale_8x.json", include_str!("definitions/comfyui/upscale/upscale_8x.json")),
            ("upscale_16x.json", include_str!("definitions/comfyui/upscale/upscale_16x.json")),
        ]),
        ("comfyui", "generate", vec![
            ("skill.json", include_str!("definitions/comfyui/generate/skill.json")),
            ("generate.json", include_str!("definitions/comfyui/generate/generate.json")),
        ]),
        ("comfyui", "transform", vec![
            ("skill.json", include_str!("definitions/comfyui/transform/skill.json")),
            ("img2img.json", include_str!("definitions/comfyui/transform/img2img.json")),
        ]),
        ("comfyui", "inpaint", vec![
            ("skill.json", include_str!("definitions/comfyui/inpaint/skill.json")),
            ("inpaint.json", include_str!("definitions/comfyui/inpaint/inpaint.json")),
        ]),
        ("comfyui", "tag", vec![
            ("skill.json", include_str!("definitions/comfyui/tag/skill.json")),
            ("tag.json", include_str!("definitions/comfyui/tag/tag.json")),
        ]),
        ("comfyui", "tts", vec![
            ("skill.json", include_str!("definitions/comfyui/tts/skill.json")),
            ("tts.json", include_str!("definitions/comfyui/tts/tts.json")),
            ("tts_f5.json", include_str!("definitions/comfyui/tts/tts_f5.json")),
        ]),
    ]
}
