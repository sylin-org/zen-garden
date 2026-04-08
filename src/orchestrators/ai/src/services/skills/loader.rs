//! Disk loader for the skill subsystem (ORCH-0029).
//!
//! Scans `{data_dir}/skills/{provider}/{moniker}/`, loads `skill.json`
//! plus every workflow file the skill references (default + variant
//! targets), and produces a typed [`SkillDefinition`] per entry.
//!
//! Two schema versions are supported:
//!
//! - **v3** (new) — parsed directly into [`SkillDefinition`]. Bindings
//!   reference canonical vocabulary field paths; constraints live
//!   inside `narrow`; selectors live in top-level `variants` and
//!   `model_selector`.
//! - **v1/v2** (legacy) — translated on read via
//!   [`legacy::translate_to_v3`]. The table maps skill-local field
//!   names (`"steps"`, `"cfg"`, `"negative"`, …) to canonical paths;
//!   `field == "workflow"` mappings become `variants`; `field ==
//!   "checkpoint"` and `field == "upscale_model"` become
//!   `model_selector`. **The disk file is never modified.**
//!
//! Draft skills (`"draft": true`) are skipped — they belong to the
//! import API's review lifecycle, not the runtime registry.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::Value;
use tokio::fs;

use crate::domain::field_path::FieldPath;
use crate::domain::media::MediaDelivery;
use crate::domain::moniker::Moniker;
use crate::domain::primitive::Primitive;

use super::types::{
    AutoKind, Binding, BindingTarget, FieldConstraint, ModelSelector, ParamOption, RawBindingV3,
    RawModelSelectorV3, RawSkillLegacy, RawSkillV3, SelfDescribedKind, SelfDescribedType,
    SkillDefinition, Variant,
};

#[derive(Debug, thiserror::Error)]
pub enum LoaderError {
    #[error("io: {0}")]
    Io(String),
    #[error("parse {file}: {message}")]
    Parse { file: String, message: String },
    #[error("legacy translation failed for {file}: {message}")]
    LegacyTranslation { file: String, message: String },
}

/// Scan a single provider's skill directory and load every non-draft
/// skill underneath. Used by adapters that want only their own
/// skills (the common case).
///
/// Layout expected:
/// ```text
/// {provider_dir}/{moniker}/skill.json
/// {provider_dir}/{moniker}/{workflow}.json
/// ```
///
/// Per-skill failures are logged as warnings and skipped.
pub async fn load_provider_skills(provider_dir: &Path) -> Vec<SkillDefinition> {
    let mut out = Vec::new();
    let moniker_dirs = match read_subdirs(provider_dir).await {
        Ok(dirs) => dirs,
        Err(e) => {
            tracing::warn!(
                dir = %provider_dir.display(),
                error = %e,
                "skills loader: cannot read provider dir"
            );
            return out;
        }
    };
    let provider_name = dir_name(provider_dir);

    for moniker_dir in moniker_dirs {
        let moniker_name = dir_name(&moniker_dir);
        let skill_path = moniker_dir.join("skill.json");
        if !skill_path.is_file() {
            continue;
        }
        match load_single(&moniker_dir, &skill_path).await {
            Ok(Some(def)) => out.push(def),
            Ok(None) => {
                tracing::debug!(
                    provider = %provider_name,
                    moniker = %moniker_name,
                    "skills loader: skipping draft"
                );
            }
            Err(e) => {
                tracing::warn!(
                    provider = %provider_name,
                    moniker = %moniker_name,
                    error = %e,
                    "skills loader: skipping broken skill"
                );
            }
        }
    }
    tracing::info!(
        provider = %provider_name,
        loaded = out.len(),
        "skills loader: provider scan complete"
    );
    out
}

/// Scan a skill directory root and load every non-draft skill across
/// every provider underneath.
///
/// Layout expected:
/// ```text
/// {skills_dir}/{provider}/{moniker}/skill.json
/// {skills_dir}/{provider}/{moniker}/{workflow}.json
/// ```
///
/// Per-skill failures are logged as warnings and skipped; one broken
/// file never stops the loader from registering the rest.
pub async fn load_skills(skills_dir: &Path) -> Vec<SkillDefinition> {
    let mut out = Vec::new();

    let provider_dirs = match read_subdirs(skills_dir).await {
        Ok(dirs) => dirs,
        Err(e) => {
            tracing::warn!(
                dir = %skills_dir.display(),
                error = %e,
                "skills loader: cannot read root"
            );
            return out;
        }
    };

    for provider_dir in provider_dirs {
        let provider_name = dir_name(&provider_dir);
        let moniker_dirs = match read_subdirs(&provider_dir).await {
            Ok(dirs) => dirs,
            Err(e) => {
                tracing::warn!(
                    dir = %provider_dir.display(),
                    error = %e,
                    "skills loader: cannot read provider dir"
                );
                continue;
            }
        };

        for moniker_dir in moniker_dirs {
            let moniker_name = dir_name(&moniker_dir);
            let skill_path = moniker_dir.join("skill.json");
            if !skill_path.is_file() {
                continue;
            }
            match load_single(&moniker_dir, &skill_path).await {
                Ok(Some(def)) => out.push(def),
                Ok(None) => {
                    tracing::debug!(
                        provider = %provider_name,
                        moniker = %moniker_name,
                        "skills loader: skipping draft"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        provider = %provider_name,
                        moniker = %moniker_name,
                        error = %e,
                        "skills loader: skipping broken skill"
                    );
                }
            }
        }
    }

    tracing::info!(loaded = out.len(), "skills loader: scan complete");
    out
}

/// Load and parse a single `skill.json`, resolving all referenced
/// workflow files. Returns `Ok(None)` for drafts; `Err` for anything
/// else that can't be parsed.
async fn load_single(skill_dir: &Path, skill_path: &Path) -> Result<Option<SkillDefinition>> {
    let raw_bytes = fs::read(skill_path).await.with_context(|| {
        format!("read {}", skill_path.display())
    })?;
    let raw_value: Value = serde_json::from_slice(&raw_bytes).with_context(|| {
        format!("parse {} as JSON", skill_path.display())
    })?;

    // Draft gate — check before any version dispatch.
    if raw_value.get("draft").and_then(|v| v.as_bool()).unwrap_or(false) {
        return Ok(None);
    }

    let version = raw_value.get("version").and_then(|v| v.as_u64()).unwrap_or(1);
    let dir_raw = dir_name(skill_dir);
    let dir_moniker = sanitize_dir_name_to_moniker(&dir_raw);
    if dir_moniker.is_none() {
        tracing::warn!(
            dir = %dir_raw,
            "skills loader: directory name could not be sanitized to a valid moniker"
        );
    }

    let partial = match version {
        3 => {
            let v3: RawSkillV3 = serde_json::from_value(raw_value).with_context(|| {
                format!("parse v3 skill {}", skill_path.display())
            })?;
            v3_to_definition(v3, dir_moniker)?
        }
        1 | 2 => {
            let legacy: RawSkillLegacy = serde_json::from_value(raw_value.clone()).with_context(
                || format!("parse legacy skill {}", skill_path.display()),
            )?;
            let mappings_raw = raw_value
                .get("mappings")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            legacy::translate_to_v3(legacy, mappings_raw, dir_moniker).with_context(|| {
                format!("translate legacy skill {}", skill_path.display())
            })?
        }
        other => anyhow::bail!("unsupported skill version {other} in {}", skill_path.display()),
    };

    // Resolve the workflow files from sibling JSON.
    let mut workflow_names: BTreeSet<String> = BTreeSet::new();
    workflow_names.insert(partial.default_workflow.clone());
    if let Some(variants) = &partial.variants {
        for v in variants {
            workflow_names.insert(v.value.clone());
        }
    }

    let mut workflows = HashMap::new();
    for name in workflow_names {
        let path = skill_dir.join(format!("{name}.json"));
        let bytes = fs::read(&path).await.with_context(|| {
            format!(
                "workflow `{name}` for skill {} not found at {}",
                skill_path.display(),
                path.display()
            )
        })?;
        let wf: Value = serde_json::from_slice(&bytes).with_context(|| {
            format!("parse workflow {}", path.display())
        })?;
        workflows.insert(name, wf);
    }

    Ok(Some(SkillDefinition {
        moniker: partial.moniker,
        display_name: partial.display_name,
        primitive: partial.primitive,
        description: partial.description,
        vram_mb: partial.vram_mb,
        default_workflow: partial.default_workflow,
        workflows,
        bindings: partial.bindings,
        model_selector: partial.model_selector,
        variants: partial.variants,
        required_models: partial.required_models,
        source: partial.source,
        preview_url: partial.preview_url,
        output_node: partial.output_node,
    }))
}

/// A partially-assembled skill before the workflow files have been
/// resolved from disk. Both the v3 and the legacy paths return one of
/// these.
pub(crate) struct PartialDefinition {
    moniker: Moniker,
    display_name: String,
    primitive: Primitive,
    description: String,
    vram_mb: u64,
    default_workflow: String,
    bindings: Vec<Binding>,
    model_selector: Option<ModelSelector>,
    variants: Option<Vec<Variant>>,
    required_models: Vec<super::types::ModelRef>,
    source: Option<super::types::ImportSource>,
    preview_url: Option<String>,
    output_node: Option<String>,
}

fn v3_to_definition(raw: RawSkillV3, dir_moniker: Option<Moniker>) -> Result<PartialDefinition> {
    let primitive = Primitive::parse_dotted(&raw.primitive)
        .map_err(|e| anyhow::anyhow!("unknown primitive `{}`: {e}", raw.primitive))?;
    let moniker = dir_moniker
        .or_else(|| Moniker::new(&raw.name).ok())
        .ok_or_else(|| anyhow::anyhow!("could not derive skill moniker from `{}`", raw.name))?;

    let mut bindings = Vec::with_capacity(raw.bindings.len());
    for rb in raw.bindings {
        bindings.push(raw_binding_to_binding(rb)?);
    }

    Ok(PartialDefinition {
        moniker,
        display_name: raw.display_name,
        primitive,
        description: raw.description,
        vram_mb: raw.vram_mb,
        default_workflow: raw.default_workflow,
        bindings,
        model_selector: raw.model_selector.map(raw_model_selector_to_typed),
        variants: raw.variants,
        required_models: raw.required_models,
        source: raw.source,
        preview_url: raw.preview_url,
        output_node: raw.output_node,
    })
}

fn raw_binding_to_binding(rb: RawBindingV3) -> Result<Binding> {
    let field = FieldPath::parse(&rb.field)
        .map_err(|e| anyhow::anyhow!("invalid field path `{}`: {e}", rb.field))?;
    let target = match (rb.placeholder, rb.node, rb.input) {
        (Some(ph), None, None) => BindingTarget::Placeholder(ph),
        (None, Some(node), Some(input)) => BindingTarget::NodeInput { node, input },
        (Some(_), Some(_), _) | (Some(_), _, Some(_)) => {
            anyhow::bail!("binding `{field}` sets both placeholder and node/input")
        }
        (None, None, _) | (None, _, None) => {
            anyhow::bail!("binding `{field}` must set either placeholder or both node and input")
        }
    };

    Ok(Binding {
        field,
        target,
        default: rb.default,
        narrow: rb.narrow,
        label: rb.label,
        required: rb.required,
        delivery: rb.delivery,
        accepted_types: rb.accepted_types,
        overlay: rb.overlay,
        self_described_type: None,
    })
}

fn raw_model_selector_to_typed(raw: RawModelSelectorV3) -> ModelSelector {
    ModelSelector {
        placeholder: raw.placeholder,
        default: raw.default,
        options: raw.options,
    }
}

// ── Directory helpers ────────────────────────────────────────

async fn read_subdirs(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut entries = fs::read_dir(dir)
        .await
        .with_context(|| format!("read_dir {}", dir.display()))?;
    let mut out = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        if entry.file_type().await?.is_dir() {
            out.push(entry.path());
        }
    }
    out.sort();
    Ok(out)
}

fn dir_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string()
}

/// Turn an on-disk directory name into a valid [`Moniker`].
///
/// The prior skill system used directory names like `generate`,
/// `upscale`, and `554a4380-1a6e-...` that collide with ORCH-0028's
/// reserved moniker table or fail the "must start with a letter"
/// rule. Since we preserve disk state byte-for-byte, we translate at
/// load time:
///
/// - Directory name clashes with a reserved moniker (e.g. `generate`,
///   `upscale`, `edit`, `analyze`) → suffix with `-skill` to escape
///   the reserved set: `generate` → `generate-skill`.
/// - Directory name starts with a digit → prefix with `skill-`:
///   `554a4380-...` → `skill-554a4380-...`.
/// - Directory name contains invalid characters → replace with `-`
///   and collapse runs.
/// - Directory name is already a valid moniker → pass through.
///
/// Returns `None` only if no transformation produces a valid moniker
/// (e.g. the name is too long after sanitization).
fn sanitize_dir_name_to_moniker(raw: &str) -> Option<Moniker> {
    // Fast path: already valid.
    if let Ok(m) = Moniker::new(raw) {
        return Some(m);
    }

    // Slow path: sanitize characters, then suffix/prefix as needed.
    let lower: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();

    // Collapse consecutive dashes.
    let mut collapsed = String::with_capacity(lower.len());
    let mut prev_dash = false;
    for c in lower.chars() {
        if c == '-' {
            if !prev_dash && !collapsed.is_empty() {
                collapsed.push('-');
            }
            prev_dash = true;
        } else {
            collapsed.push(c);
            prev_dash = false;
        }
    }
    let collapsed = collapsed.trim_matches('-').to_string();
    if collapsed.is_empty() {
        return None;
    }

    // Ensure starts with a lowercase letter.
    let needs_prefix = !collapsed.as_bytes().first().is_some_and(|b| b.is_ascii_lowercase());
    let with_prefix = if needs_prefix {
        format!("skill-{collapsed}")
    } else {
        collapsed
    };

    // Try as-is; if it collides with the reserved set, suffix `-skill`.
    if let Ok(m) = Moniker::new(&with_prefix) {
        return Some(m);
    }
    let suffixed = format!("{with_prefix}-skill");
    Moniker::new(suffixed).ok()
}

// ── Legacy translation ───────────────────────────────────────

pub(crate) mod legacy {
    //! Translate v1/v2 `skill.json` files to the v3 in-memory model.
    //!
    //! The translation is deterministic and idempotent: the same
    //! legacy file always produces the same `SkillDefinition`. The
    //! disk file is NEVER modified.
    //!
    //! Rules:
    //!
    //! 1. `name` prefix + `capability` → canonical `Primitive`.
    //! 2. `mappings[*]` are walked:
    //!    - `type: "content"` → a media binding (image/audio role)
    //!      or a canonical text binding (prompt/negative role).
    //!    - `type: "param"` + `field == "workflow"` → hoisted into
    //!      top-level `variants`.
    //!    - `type: "param"` + `field == "checkpoint" | "upscale_model"`
    //!      → hoisted into top-level `model_selector`.
    //!    - `type: "param"` + other → translated to a canonical
    //!      vocabulary field via the per-primitive table; `param_type`
    //!      becomes `FieldConstraint` (Options / Range / Auto).
    //!    - Fields not in the translation table land as `x_<legacy>`
    //!      bindings with a self-described type.
    //! 3. Legacy `content_slots` are ignored as a standalone section —
    //!    the information they carry is folded into the bindings via
    //!    the content mappings.

    use super::*;
    use crate::domain::keys;
    use crate::services::skills::types::{RawContentSlotLegacy, RawMappingLegacy};

    /// Translate a legacy skill definition to the v3 in-memory model.
    pub fn translate_to_v3(
        legacy: RawSkillLegacy,
        mappings_raw: Vec<Value>,
        dir_moniker: Option<Moniker>,
    ) -> Result<PartialDefinition> {
        // ── 1. Primitive resolution ───────────────────────────
        let primitive = resolve_primitive(&legacy.name, &legacy.capability)?;

        // ── 2. Moniker resolution ─────────────────────────────
        //
        // Prefer the directory name (stable across edits). Fall back
        // to deriving from the skill `name` field.
        let moniker = dir_moniker
            .or_else(|| derive_moniker_from_name(&legacy.name))
            .ok_or_else(|| anyhow::anyhow!("could not derive moniker from name `{}`", legacy.name))?;

        // ── 3. Walk mappings ──────────────────────────────────
        let mut bindings: Vec<Binding> = Vec::new();
        let model_selector_unused: Option<RawBindingV3> = None; // (sentinel, see finalize block below)
        let mut variants: Option<Vec<Variant>> = None;
        let mut selector_options: Vec<ParamOption> = Vec::new();
        let mut selector_default: Option<String> = None;
        let mut selector_placeholder: Option<String> = None;
        let mut lora_counter: u32 = 0;

        let field_table = field_table_for(primitive);

        for raw in &mappings_raw {
            let parsed: RawMappingLegacy = match serde_json::from_value(raw.clone()) {
                Ok(m) => m,
                Err(e) => {
                    tracing::debug!(
                        error = %e,
                        raw = %raw,
                        "legacy loader: skipping unparseable mapping"
                    );
                    continue;
                }
            };

            match parsed {
                RawMappingLegacy::Content { role, content_type, placeholder } => {
                    if let Some(binding) = content_mapping_to_binding(primitive, &role, &content_type, placeholder, &legacy.content_slots) {
                        bindings.push(binding);
                    }
                }
                RawMappingLegacy::Param {
                    field,
                    label,
                    node,
                    input,
                    placeholder,
                    param_type,
                    options,
                    min,
                    max,
                    step,
                    kind,
                    default,
                } => {
                    // ── Workflow selector → variants ────────
                    if field == "workflow" {
                        if let Some(opts) = options {
                            let list = opts
                                .into_iter()
                                .map(|opt| Variant {
                                    value: opt.value.as_str().unwrap_or_default().to_string(),
                                    label: opt.label,
                                })
                                .filter(|v| !v.value.is_empty())
                                .collect::<Vec<_>>();
                            if !list.is_empty() {
                                variants = Some(list);
                            }
                        }
                        continue;
                    }

                    // ── Checkpoint / upscale_model → model_selector ──
                    if matches!(field.as_str(), "checkpoint" | "upscale_model") {
                        if let Some(ph) = placeholder.clone() {
                            if selector_placeholder.is_none() {
                                selector_placeholder = Some(ph);
                            }
                        }
                        if let Some(d) = default.as_ref().and_then(|v| v.as_str()) {
                            if selector_default.is_none() {
                                selector_default = Some(d.to_string());
                            }
                        }
                        if let Some(opts) = options {
                            for opt in opts {
                                if !selector_options.iter().any(|existing| existing.value == opt.value) {
                                    selector_options.push(opt);
                                }
                            }
                        }
                        continue;
                    }

                    // ── Multiple lora mappings — fold into x_lora_{n} ─
                    if field == "lora" {
                        lora_counter += 1;
                        let x_field = format!("x_lora_{lora_counter}");
                        if let Some(binding) = param_to_x_binding(
                            &x_field,
                            label.clone().or_else(|| Some(format!("LoRA {lora_counter}"))),
                            node.clone(),
                            input.clone(),
                            placeholder.clone(),
                            &param_type,
                            options.clone(),
                            min,
                            max,
                            step,
                            kind.as_deref(),
                            default.clone(),
                        ) {
                            bindings.push(binding);
                        }
                        continue;
                    }

                    // ── Known canonical field ──────────────
                    if let Some(canonical) = field_table.get(field.as_str()) {
                        if let Some(binding) = param_to_canonical_binding(
                            canonical.clone(),
                            label,
                            node,
                            input,
                            placeholder,
                            &param_type,
                            options,
                            min,
                            max,
                            step,
                            kind.as_deref(),
                            default,
                        ) {
                            bindings.push(binding);
                        }
                        continue;
                    }

                    // ── Unknown legacy field → x_{field} ──
                    let x_field = format!("x_{}", sanitize_x_field(&field));
                    if let Some(binding) = param_to_x_binding(
                        &x_field,
                        label,
                        node,
                        input,
                        placeholder,
                        &param_type,
                        options,
                        min,
                        max,
                        step,
                        kind.as_deref(),
                        default,
                    ) {
                        bindings.push(binding);
                    }
                }
            }
        }

        // ── 4. Finalize model selector from collected pieces ──
        let final_model_selector = if !selector_options.is_empty() || selector_placeholder.is_some() {
            let placeholder = selector_placeholder
                .or_else(|| {
                    // Infer from primitive if it's a known case.
                    match primitive {
                        Primitive::ImageGenerate | Primitive::ImageEdit => {
                            Some("PLACEHOLDER_CHECKPOINT".to_string())
                        }
                        Primitive::ImageUpscale => Some("PLACEHOLDER_MODEL".to_string()),
                        _ => None,
                    }
                })
                .unwrap_or_default();
            let default = selector_default
                .or_else(|| {
                    selector_options
                        .first()
                        .and_then(|o| o.value.as_str().map(String::from))
                })
                .unwrap_or_default();
            if !placeholder.is_empty() && !default.is_empty() {
                Some(ModelSelector {
                    placeholder,
                    default,
                    options: selector_options,
                })
            } else {
                None
            }
        } else {
            None
        };
        // `model_selector_unused` was only reserved as a sentinel for
        // this block; consume it to silence `dead_code` until the
        // CRUD surface lands.
        let _ = model_selector_unused;

        Ok(PartialDefinition {
            moniker,
            display_name: legacy.display_name,
            primitive,
            description: legacy.description,
            vram_mb: legacy.vram_mb,
            default_workflow: legacy.default_workflow,
            bindings,
            model_selector: final_model_selector,
            variants,
            required_models: legacy.required_models,
            source: legacy.source,
            preview_url: legacy.preview_url,
            output_node: None,
        })
    }

    /// Map legacy `name` prefix + `capability` to a canonical primitive.
    ///
    /// Legacy `name` field shape: `<modality>.<leaf>[.<rest>]` —
    /// `image.generate`, `image.img2img`, `image.inpaint`,
    /// `image.upscale`, `vision.tag`, `speech.tts`, or `image.<slug>`
    /// for CivitAI imports. We split on `.` and route by the first
    /// two segments. Capabilities `"vision"` / `"speech"` are
    /// secondary signals when the name doesn't match a known leaf.
    fn resolve_primitive(name: &str, capability: &str) -> Result<Primitive> {
        let mut parts = name.split('.');
        let modality = parts.next().unwrap_or("");
        let leaf = parts.next().unwrap_or("");

        let primitive = match (modality, leaf) {
            ("image", "upscale") => Primitive::ImageUpscale,
            ("image", "inpaint") | ("image", "img2img") => Primitive::ImageEdit,
            ("image", _) => {
                // Generic image.* — built-in `generate` plus all
                // CivitAI-imported skills (which use the slug as leaf).
                Primitive::ImageGenerate
            }
            ("vision", _) => Primitive::ImageAnalyze,
            ("speech", _) => Primitive::AudioGenerate,
            _ => match capability {
                "vision" => Primitive::ImageAnalyze,
                "speech" => Primitive::AudioGenerate,
                _ => anyhow::bail!(
                    "unrecognized legacy skill name `{name}` / capability `{capability}`"
                ),
            },
        };
        Ok(primitive)
    }

    /// Derive a moniker from a legacy `name` like `"image.generate"` or
    /// `"image.flux-47477"` — everything after the first dot.
    fn derive_moniker_from_name(name: &str) -> Option<Moniker> {
        let slug = name.split_once('.').map(|(_, rest)| rest).unwrap_or(name);
        Moniker::new(slug).ok()
    }

    /// Content mapping → typed binding.
    fn content_mapping_to_binding(
        primitive: Primitive,
        role: &str,
        content_type: &str,
        placeholder: String,
        content_slots: &[RawContentSlotLegacy],
    ) -> Option<Binding> {
        let (field, accepted_types, delivery, overlay) = match (primitive, role, content_type) {
            // ── Images ─────────────────────────────────────
            (_, "source", "image") => (
                keys::image::SOURCE,
                vec![
                    "image/png".to_string(),
                    "image/jpeg".to_string(),
                    "image/webp".to_string(),
                ],
                Some(MediaDelivery::Transfer),
                None,
            ),
            (_, "mask", "image") => (
                keys::image::MASK,
                vec!["image/png".to_string()],
                Some(MediaDelivery::Transfer),
                Some("source".to_string()),
            ),

            // ── Text prompts ───────────────────────────────
            (Primitive::ImageGenerate | Primitive::ImageEdit, "prompt", "text") => {
                (keys::image::PROMPT_POSITIVE, vec![], None, None)
            }
            (Primitive::ImageGenerate | Primitive::ImageEdit, "negative", "text") => {
                (keys::image::PROMPT_NEGATIVE, vec![], None, None)
            }
            (Primitive::ImageAnalyze, "prompt", "text") => {
                (keys::text::PROMPT_USER, vec![], None, None)
            }
            (Primitive::AudioGenerate, "prompt", "text") => {
                (keys::audio::TEXT, vec![], None, None)
            }

            _ => {
                tracing::debug!(role, content_type, primitive = %primitive, "legacy loader: dropping content mapping with no canonical target");
                return None;
            }
        };

        // Pull the default from the matching content slot, if any.
        let default = content_slots
            .iter()
            .find(|s| s.role == role)
            .and_then(|s| s.default.clone())
            .filter(|d| !d.starts_with("PLACEHOLDER")) // skip legacy sentinel strings
            .map(serde_json::Value::String);

        let required = content_slots
            .iter()
            .find(|s| s.role == role)
            .map(|s| s.required)
            .unwrap_or(true);

        Some(Binding {
            field,
            target: BindingTarget::Placeholder(placeholder),
            default,
            narrow: None,
            label: None,
            required,
            delivery,
            accepted_types,
            overlay,
            self_described_type: None,
        })
    }

    /// Param mapping → canonical binding.
    #[allow(clippy::too_many_arguments)]
    fn param_to_canonical_binding(
        field: FieldPath,
        label: Option<String>,
        node: Option<String>,
        input: Option<String>,
        placeholder: Option<String>,
        param_type: &str,
        options: Option<Vec<ParamOption>>,
        min: Option<f64>,
        max: Option<f64>,
        step: Option<f64>,
        kind: Option<&str>,
        default: Option<serde_json::Value>,
    ) -> Option<Binding> {
        let target = binding_target_from_pieces(placeholder, node, input)?;
        let narrow = build_constraint(param_type, options, min, max, step, kind);

        Some(Binding {
            field,
            target,
            default,
            narrow,
            label,
            required: false,
            delivery: None,
            accepted_types: Vec::new(),
            overlay: None,
            self_described_type: None,
        })
    }

    /// Param mapping → `x_*` binding with self-described type.
    #[allow(clippy::too_many_arguments)]
    fn param_to_x_binding(
        x_field: &str,
        label: Option<String>,
        node: Option<String>,
        input: Option<String>,
        placeholder: Option<String>,
        param_type: &str,
        options: Option<Vec<ParamOption>>,
        min: Option<f64>,
        max: Option<f64>,
        step: Option<f64>,
        kind: Option<&str>,
        default: Option<serde_json::Value>,
    ) -> Option<Binding> {
        let target = binding_target_from_pieces(placeholder, node, input)?;
        let field = FieldPath::parse(x_field).ok()?;
        let narrow = build_constraint(param_type, options, min, max, step, kind);

        // Self-described type — enough for the dashboard to render a
        // widget even though the vocabulary doesn't catalogue this
        // field.
        let self_described = match param_type {
            "options" | "text" => SelfDescribedKind::String,
            "range" => SelfDescribedKind::Number {
                min,
                max,
            },
            "auto" => SelfDescribedKind::Integer { min: None, max: None },
            _ => SelfDescribedKind::String,
        };

        Some(Binding {
            field,
            target,
            default,
            narrow,
            label,
            required: false,
            delivery: None,
            accepted_types: Vec::new(),
            overlay: None,
            self_described_type: Some(SelfDescribedType {
                kind: self_described,
                description: None,
            }),
        })
    }

    fn binding_target_from_pieces(
        placeholder: Option<String>,
        node: Option<String>,
        input: Option<String>,
    ) -> Option<BindingTarget> {
        match (placeholder, node, input) {
            (Some(ph), _, _) => Some(BindingTarget::Placeholder(ph)),
            (None, Some(n), Some(i)) => Some(BindingTarget::NodeInput { node: n, input: i }),
            _ => None,
        }
    }

    fn build_constraint(
        param_type: &str,
        options: Option<Vec<ParamOption>>,
        min: Option<f64>,
        max: Option<f64>,
        step: Option<f64>,
        kind: Option<&str>,
    ) -> Option<FieldConstraint> {
        match param_type {
            "options" => options.map(|options| FieldConstraint::Options { options }),
            "range" => match (min, max) {
                (Some(min), Some(max)) => Some(FieldConstraint::Range { min, max, step }),
                _ => None,
            },
            "auto" => {
                let k = match kind.unwrap_or("random_int") {
                    "random_int" => AutoKind::RandomInt,
                    _ => AutoKind::RandomInt,
                };
                Some(FieldConstraint::Auto { kind_inner: k })
            }
            _ => None,
        }
    }

    fn sanitize_x_field(raw: &str) -> String {
        raw.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect()
    }

    /// Legacy skill-local field name → canonical vocabulary path.
    ///
    /// Per-primitive; omitted entries fall through to `x_*`
    /// passthrough.
    fn field_table_for(primitive: Primitive) -> HashMap<&'static str, FieldPath> {
        let mut map = HashMap::new();
        match primitive {
            Primitive::ImageGenerate => {
                map.insert("negative", keys::image::PROMPT_NEGATIVE);
                map.insert("width", keys::image::DIMENSIONS_WIDTH);
                map.insert("height", keys::image::DIMENSIONS_HEIGHT);
                map.insert("steps", keys::image::SAMPLING_STEPS);
                map.insert("cfg", keys::image::SAMPLING_GUIDANCE);
                map.insert("seed", keys::image::SAMPLING_SEED);
            }
            Primitive::ImageEdit => {
                map.insert("negative", keys::image::PROMPT_NEGATIVE);
                map.insert("steps", keys::image::SAMPLING_STEPS);
                map.insert("cfg", keys::image::SAMPLING_GUIDANCE);
                map.insert("seed", keys::image::SAMPLING_SEED);
                // `strength`/`denoise` isn't in the vocabulary today —
                // it falls through to x_strength.
            }
            Primitive::ImageUpscale => {
                map.insert("scale", keys::image::SCALE);
            }
            Primitive::ImageAnalyze => {
                // `vision.tag` params (threshold, character_threshold,
                // tag_limit, model) don't have canonical equivalents;
                // they all fall through to x_*.
            }
            Primitive::AudioGenerate => {
                map.insert("voice", keys::audio::VOICE_ID);
                map.insert("speed", keys::audio::VOICE_SPEED);
            }
            _ => {}
        }
        map
    }
}
