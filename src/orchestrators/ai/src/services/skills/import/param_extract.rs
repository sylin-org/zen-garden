//! Parameter extraction — walk an API-format workflow, identify
//! tunable values, replace them with `PLACEHOLDER_*` tokens, and
//! emit v3 [`Binding`]s ready for the skill loader (ORCH-0029).
//!
//! ## KSampler-driven role detection
//!
//! The hard part is telling the positive prompt from the negative
//! prompt when a workflow has multiple `CLIPTextEncode` nodes.
//! Naive "first is positive, second is negative" heuristics break on
//! workflows with conditioning concat nodes between the encoder and
//! the sampler.
//!
//! This module runs a **pre-pass** that walks every `KSampler` /
//! `KSamplerAdvanced` node, follows its `positive` and `negative`
//! link references back to the source nodes, and records a
//! `clip_roles: HashMap<node_id, "prompt" | "negative">`. The main
//! walk then uses this map authoritatively — no heuristics.
//!
//! ## Output
//!
//! Returns an [`ExtractionResult`] with:
//!
//! - The workflow with `PLACEHOLDER_*` strings inlined at the right
//!   locations (the executor substitutes them at dispatch time).
//! - A list of typed [`Binding`] entries in canonical field-path
//!   form, ready for the skill loader to split into the public
//!   Registration and the private LoadedSkill.
//! - A hoisted [`ExtractedModelSelector`] when the workflow has a
//!   checkpoint loader — the import pipeline converts this into the
//!   top-level `model_selector` on the generated `skill.json`.

use serde_json::Value;

use crate::domain::field_path::FieldPath;
use crate::domain::keys;
use crate::domain::media::MediaDelivery;
use crate::domain::provider::{AutoKind, FieldConstraint, ParamOption};

use crate::services::skills::types::{Binding, BindingTarget};

/// Result of the extraction pass.
pub struct ExtractionResult {
    /// The workflow with tunable values replaced by placeholders or
    /// left in place for node+input addressing.
    pub workflow: Value,
    /// Typed bindings the loader can consume directly.
    pub bindings: Vec<Binding>,
    /// Detected model selector — hoisted to the top-level
    /// `model_selector` on the generated `skill.json`.
    pub model_selector: Option<ExtractedModelSelector>,
}

/// Intermediate shape for the model selector, emitted by the walk
/// and converted to [`super::super::types::ModelSelector`] by
/// `analyze.rs` once the model resolver has reconciled the default.
#[derive(Debug, Clone)]
pub struct ExtractedModelSelector {
    pub placeholder: String,
    /// The filename the workflow originally used — becomes the
    /// default option. May be a legacy garbage value (bare name
    /// without extension, node ID) which `analyze.rs` reconciles.
    pub default: Option<String>,
    pub options: Vec<ParamOption>,
}

/// Extract parameters from a ComfyUI API-format workflow.
///
/// Walks every node, identifies tunable values by `class_type`,
/// replaces string values with `PLACEHOLDER_*` tokens, and generates
/// one [`Binding`] per tunable field.
pub fn extract(workflow: &Value) -> ExtractionResult {
    let mut workflow = workflow.clone();
    let mut bindings: Vec<Binding> = Vec::new();
    let mut model_selector: Option<ExtractedModelSelector> = None;

    let mut image_count: u32 = 0;
    let mut has_prompt = false;
    let mut has_negative = false;

    let Some(obj) = workflow.as_object().cloned() else {
        return ExtractionResult {
            workflow,
            bindings,
            model_selector,
        };
    };

    // ── Pre-pass: map CLIPTextEncode nodes to their KSampler role ─
    //
    // Walk every KSampler node, follow its `positive` and `negative`
    // links back to the source nodes. This is authoritative — no
    // heuristics.
    let mut clip_roles: std::collections::HashMap<String, &'static str> =
        std::collections::HashMap::new();
    for (_nid, node) in &obj {
        let ct = node
            .get("class_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if ct == "KSampler" || ct == "KSamplerAdvanced" {
            if let Some(inputs) = node.get("inputs") {
                if let Some(pos_link) = inputs.get("positive").and_then(|v| v.as_array()) {
                    if let Some(pos_id) = pos_link.first().and_then(|v| v.as_str()) {
                        clip_roles.insert(pos_id.to_string(), "prompt");
                    }
                }
                if let Some(neg_link) = inputs.get("negative").and_then(|v| v.as_array()) {
                    if let Some(neg_id) = neg_link.first().and_then(|v| v.as_str()) {
                        clip_roles.insert(neg_id.to_string(), "negative");
                    }
                }
            }
        }
    }

    // Deterministic iteration order (numeric node IDs sort by value).
    let mut node_ids: Vec<String> = obj.keys().cloned().collect();
    node_ids.sort_by(|a, b| {
        a.parse::<u32>()
            .unwrap_or(u32::MAX)
            .cmp(&b.parse::<u32>().unwrap_or(u32::MAX))
    });

    for node_id in &node_ids {
        let Some(node) = obj.get(node_id) else {
            continue;
        };
        let Some(class_type) = node.get("class_type").and_then(|v| v.as_str()).map(String::from)
        else {
            continue;
        };

        match class_type.as_str() {
            // ── Image loaders ───────────────────────────────
            "LoadImage" => {
                let (role, placeholder) = if image_count == 0 {
                    ("source", "PLACEHOLDER_IMAGE")
                } else {
                    ("mask", "PLACEHOLDER_MASK")
                };
                let field = if role == "source" {
                    keys::image::SOURCE
                } else {
                    keys::image::MASK
                };
                set_input_str(&mut workflow, node_id, "image", placeholder);
                bindings.push(Binding {
                    field,
                    target: BindingTarget::Placeholder(placeholder.into()),
                    default: None,
                    narrow: None,
                    label: None,
                    required: true,
                    delivery: Some(MediaDelivery::Transfer),
                    accepted_types: vec![
                        "image/png".into(),
                        "image/jpeg".into(),
                        "image/webp".into(),
                    ],
                    overlay: if image_count > 0 {
                        Some("source".into())
                    } else {
                        None
                    },
                    self_described_type: None,
                });
                image_count += 1;
            }

            "LoadImageMask" => {
                set_input_str(&mut workflow, node_id, "image", "PLACEHOLDER_MASK");
                bindings.push(Binding {
                    field: keys::image::MASK,
                    target: BindingTarget::Placeholder("PLACEHOLDER_MASK".into()),
                    default: None,
                    narrow: None,
                    label: None,
                    required: true,
                    delivery: Some(MediaDelivery::Transfer),
                    accepted_types: vec!["image/png".into()],
                    overlay: Some("source".into()),
                    self_described_type: None,
                });
            }

            // ── Checkpoint loaders → model_selector ─────────
            "CheckpointLoaderSimple" | "CheckpointLoader" | "unCLIPCheckpointLoader" => {
                let current = get_input_str(&workflow, node_id, "ckpt_name");
                set_input_str(&mut workflow, node_id, "ckpt_name", "PLACEHOLDER_CHECKPOINT");
                if model_selector.is_none() {
                    model_selector = Some(ExtractedModelSelector {
                        placeholder: "PLACEHOLDER_CHECKPOINT".into(),
                        default: current.clone(),
                        options: current
                            .as_ref()
                            .map(|v| vec![param_option_string(v)])
                            .unwrap_or_default(),
                    });
                }
            }

            // ── Upscale model loaders → model_selector ──────
            "UpscaleModelLoader" => {
                let current = get_input_str(&workflow, node_id, "model_name");
                set_input_str(&mut workflow, node_id, "model_name", "PLACEHOLDER_MODEL");
                if model_selector.is_none() {
                    model_selector = Some(ExtractedModelSelector {
                        placeholder: "PLACEHOLDER_MODEL".into(),
                        default: current.clone(),
                        options: current
                            .as_ref()
                            .map(|v| vec![param_option_string(v)])
                            .unwrap_or_default(),
                    });
                }
            }

            // ── LoRA loaders → x_lora_{n} bindings ──────────
            "LoraLoader" => {
                let current = get_input_str(&workflow, node_id, "lora_name");
                set_input_str(&mut workflow, node_id, "lora_name", "PLACEHOLDER_LORA");
                // Emit as a string-typed binding. The field path is
                // `x_lora_{count}` — the dashboard renders it via
                // the self-described type system.
                let lora_index = bindings
                    .iter()
                    .filter(|b| b.field.as_str().starts_with("x_lora_"))
                    .count()
                    + 1;
                let field_str = format!("x_lora_{lora_index}");
                if let Ok(field) = FieldPath::parse(&field_str) {
                    bindings.push(Binding {
                        field,
                        target: BindingTarget::Placeholder("PLACEHOLDER_LORA".into()),
                        default: current
                            .as_ref()
                            .map(|v| Value::String(v.clone())),
                        narrow: current.as_ref().map(|v| FieldConstraint::Options {
                            options: vec![param_option_string(v)],
                        }),
                        label: Some(format!("LoRA {lora_index}")),
                        required: false,
                        delivery: None,
                        accepted_types: Vec::new(),
                        overlay: None,
                        self_described_type: None,
                    });
                }
            }

            // ── Text encoders ───────────────────────────────
            "CLIPTextEncode" => {
                let role = clip_roles.get(node_id).copied();
                let current_text = get_input_str(&workflow, node_id, "text")
                    .or_else(|| resolve_linked_text(&workflow, node_id, "text"));

                match role {
                    Some("prompt") if !has_prompt => {
                        set_input_str(&mut workflow, node_id, "text", "PLACEHOLDER_PROMPT");
                        bindings.push(Binding {
                            field: keys::image::PROMPT_POSITIVE,
                            target: BindingTarget::Placeholder("PLACEHOLDER_PROMPT".into()),
                            default: current_text.map(Value::String),
                            narrow: None,
                            label: None,
                            required: true,
                            delivery: None,
                            accepted_types: Vec::new(),
                            overlay: None,
                            self_described_type: None,
                        });
                        has_prompt = true;
                    }
                    Some("negative") if !has_negative => {
                        set_input_str(&mut workflow, node_id, "text", "PLACEHOLDER_NEGATIVE");
                        bindings.push(Binding {
                            field: keys::image::PROMPT_NEGATIVE,
                            target: BindingTarget::Placeholder("PLACEHOLDER_NEGATIVE".into()),
                            default: current_text.map(Value::String),
                            narrow: None,
                            label: None,
                            required: false,
                            delivery: None,
                            accepted_types: Vec::new(),
                            overlay: None,
                            self_described_type: None,
                        });
                        has_negative = true;
                    }
                    _ => {
                        // Not connected to a KSampler, or duplicate — leave as-is.
                    }
                }
            }

            // ── KSampler knobs ──────────────────────────────
            "KSampler" | "KSamplerAdvanced" => {
                // Steps
                let steps = get_input_number(&workflow, node_id, "steps");
                bindings.push(Binding {
                    field: keys::image::SAMPLING_STEPS,
                    target: BindingTarget::NodeInput {
                        node: node_id.clone(),
                        input: "steps".into(),
                    },
                    default: steps.map(|v| serde_json::json!(v)),
                    narrow: Some(FieldConstraint::Range {
                        min: 1.0,
                        max: 50.0,
                        step: Some(1.0),
                    }),
                    label: Some("Steps".into()),
                    required: false,
                    delivery: None,
                    accepted_types: Vec::new(),
                    overlay: None,
                    self_described_type: None,
                });

                // CFG
                let cfg = get_input_float(&workflow, node_id, "cfg");
                bindings.push(Binding {
                    field: keys::image::SAMPLING_GUIDANCE,
                    target: BindingTarget::NodeInput {
                        node: node_id.clone(),
                        input: "cfg".into(),
                    },
                    default: cfg.map(|v| serde_json::json!(v)),
                    narrow: Some(FieldConstraint::Range {
                        min: 1.0,
                        max: 30.0,
                        step: Some(0.5),
                    }),
                    label: Some("CFG Scale".into()),
                    required: false,
                    delivery: None,
                    accepted_types: Vec::new(),
                    overlay: None,
                    self_described_type: None,
                });

                // Seed — auto-generated random_int per request.
                let seed = get_input_number(&workflow, node_id, "seed");
                bindings.push(Binding {
                    field: keys::image::SAMPLING_SEED,
                    target: BindingTarget::NodeInput {
                        node: node_id.clone(),
                        input: "seed".into(),
                    },
                    default: seed.map(|v| serde_json::json!(v)),
                    narrow: Some(FieldConstraint::Auto {
                        kind_inner: AutoKind::RandomInt,
                    }),
                    label: Some("Seed".into()),
                    required: false,
                    delivery: None,
                    accepted_types: Vec::new(),
                    overlay: None,
                    self_described_type: None,
                });
            }

            // ── Empty latent image — dimensions ─────────────
            "EmptyLatentImage" | "EmptySD3LatentImage" => {
                let width = get_input_number(&workflow, node_id, "width");
                let height = get_input_number(&workflow, node_id, "height");

                bindings.push(Binding {
                    field: keys::image::DIMENSIONS_WIDTH,
                    target: BindingTarget::NodeInput {
                        node: node_id.clone(),
                        input: "width".into(),
                    },
                    default: width.map(|v| serde_json::json!(v)),
                    narrow: Some(FieldConstraint::Options {
                        options: common_size_options(),
                    }),
                    label: Some("Width".into()),
                    required: false,
                    delivery: None,
                    accepted_types: Vec::new(),
                    overlay: None,
                    self_described_type: None,
                });

                bindings.push(Binding {
                    field: keys::image::DIMENSIONS_HEIGHT,
                    target: BindingTarget::NodeInput {
                        node: node_id.clone(),
                        input: "height".into(),
                    },
                    default: height.map(|v| serde_json::json!(v)),
                    narrow: Some(FieldConstraint::Options {
                        options: common_size_options(),
                    }),
                    label: Some("Height".into()),
                    required: false,
                    delivery: None,
                    accepted_types: Vec::new(),
                    overlay: None,
                    self_described_type: None,
                });
            }

            _ => {}
        }
    }

    ExtractionResult {
        workflow,
        bindings,
        model_selector,
    }
}

fn param_option_string(value: &str) -> ParamOption {
    ParamOption {
        value: Value::String(value.to_string()),
        label: None,
    }
}

fn common_size_options() -> Vec<ParamOption> {
    [512u64, 768, 1024, 1280, 1536, 2048]
        .into_iter()
        .map(|v| ParamOption {
            value: serde_json::json!(v),
            label: None,
        })
        .collect()
}

// ── Helpers ───────────────────────────────────────────────────

/// Follow a link reference to resolve the text value from the source
/// node. Some workflows wire `CLIPTextEncode.text` to a `Text
/// Multiline`, `PrimitiveNode`, or similar string node — the link is
/// `["source_id", slot]`. Walks two levels deep before giving up.
fn resolve_linked_text(workflow: &Value, node_id: &str, field: &str) -> Option<String> {
    let link = workflow.get(node_id)?.get("inputs")?.get(field)?;
    let arr = link.as_array()?;
    if arr.len() < 2 {
        return None;
    }
    let source_id = arr[0].as_str()?;
    let source_inputs = workflow.get(source_id)?.get("inputs")?;

    for text_field in &["text", "string", "value", "Text", "STRING"] {
        if let Some(s) = source_inputs.get(*text_field).and_then(|v| v.as_str()) {
            if !s.is_empty() && !s.starts_with("PLACEHOLDER") {
                return Some(s.to_string());
            }
        }
    }

    // One level deeper, bounded to avoid loops.
    for text_field in &["text", "string", "value"] {
        if let Some(inner) = resolve_linked_text_inner(workflow, source_id, text_field) {
            return Some(inner);
        }
    }
    None
}

fn resolve_linked_text_inner(workflow: &Value, node_id: &str, field: &str) -> Option<String> {
    let link = workflow.get(node_id)?.get("inputs")?.get(field)?;
    let arr = link.as_array()?;
    let source_id = arr.first()?.as_str()?;
    let source_inputs = workflow.get(source_id)?.get("inputs")?;
    for text_field in &["text", "string", "value"] {
        if let Some(s) = source_inputs.get(*text_field).and_then(|v| v.as_str()) {
            if !s.is_empty() && !s.starts_with("PLACEHOLDER") {
                return Some(s.to_string());
            }
        }
    }
    None
}

fn get_input_str(workflow: &Value, node_id: &str, field: &str) -> Option<String> {
    workflow
        .get(node_id)?
        .get("inputs")?
        .get(field)?
        .as_str()
        .map(String::from)
}

fn get_input_number(workflow: &Value, node_id: &str, field: &str) -> Option<u64> {
    workflow.get(node_id)?.get("inputs")?.get(field)?.as_u64()
}

fn get_input_float(workflow: &Value, node_id: &str, field: &str) -> Option<f64> {
    workflow.get(node_id)?.get("inputs")?.get(field)?.as_f64()
}

fn set_input_str(workflow: &mut Value, node_id: &str, field: &str, value: &str) {
    if let Some(inputs) = workflow
        .get_mut(node_id)
        .and_then(|n| n.get_mut("inputs"))
        .and_then(|i| i.as_object_mut())
    {
        inputs.insert(field.to_string(), Value::String(value.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn txt2img_workflow() -> Value {
        serde_json::json!({
            "1": { "class_type": "CheckpointLoaderSimple", "inputs": { "ckpt_name": "model.safetensors" } },
            "2": { "class_type": "CLIPTextEncode", "inputs": { "text": "a cat", "clip": ["1", 1] } },
            "3": { "class_type": "CLIPTextEncode", "inputs": { "text": "ugly", "clip": ["1", 1] } },
            "4": { "class_type": "KSampler", "inputs": {
                "model": ["1", 0], "positive": ["2", 0], "negative": ["3", 0],
                "latent_image": ["5", 0], "seed": 42, "steps": 20, "cfg": 7.0,
                "sampler_name": "euler", "scheduler": "normal", "denoise": 1.0
            }},
            "5": { "class_type": "EmptyLatentImage", "inputs": { "width": 512, "height": 768, "batch_size": 1 } },
            "6": { "class_type": "VAEDecode", "inputs": { "samples": ["4", 0], "vae": ["1", 2] } },
            "7": { "class_type": "SaveImage", "inputs": { "images": ["6", 0] } }
        })
    }

    #[test]
    fn extract_txt2img_plants_checkpoint_placeholder() {
        let result = extract(&txt2img_workflow());
        assert_eq!(
            result.workflow["1"]["inputs"]["ckpt_name"],
            "PLACEHOLDER_CHECKPOINT"
        );
        let sel = result.model_selector.expect("model selector emitted");
        assert_eq!(sel.placeholder, "PLACEHOLDER_CHECKPOINT");
        assert_eq!(sel.default.as_deref(), Some("model.safetensors"));
    }

    #[test]
    fn extract_txt2img_roles_prompts_via_ksampler_links() {
        let result = extract(&txt2img_workflow());
        // Positive and negative are distinguished via KSampler's
        // `positive` / `negative` link references, not by order.
        assert_eq!(result.workflow["2"]["inputs"]["text"], "PLACEHOLDER_PROMPT");
        assert_eq!(
            result.workflow["3"]["inputs"]["text"],
            "PLACEHOLDER_NEGATIVE"
        );
        let positive = result
            .bindings
            .iter()
            .find(|b| b.field.as_str() == "image.prompt.positive")
            .expect("positive binding");
        assert_eq!(positive.default, Some(Value::String("a cat".into())));
        let negative = result
            .bindings
            .iter()
            .find(|b| b.field.as_str() == "image.prompt.negative")
            .expect("negative binding");
        assert_eq!(negative.default, Some(Value::String("ugly".into())));
    }

    #[test]
    fn extract_txt2img_emits_sampler_knobs() {
        let result = extract(&txt2img_workflow());
        let steps = result
            .bindings
            .iter()
            .find(|b| b.field.as_str() == "image.sampling.steps")
            .expect("steps binding");
        assert_eq!(steps.default, Some(serde_json::json!(20)));
        assert!(matches!(
            steps.narrow,
            Some(FieldConstraint::Range { .. })
        ));

        let cfg = result
            .bindings
            .iter()
            .find(|b| b.field.as_str() == "image.sampling.guidance")
            .expect("cfg binding");
        assert_eq!(cfg.default, Some(serde_json::json!(7.0)));

        let seed = result
            .bindings
            .iter()
            .find(|b| b.field.as_str() == "image.sampling.seed")
            .expect("seed binding");
        assert!(matches!(
            seed.narrow,
            Some(FieldConstraint::Auto { .. })
        ));
    }

    #[test]
    fn extract_txt2img_emits_dimensions_from_empty_latent() {
        let result = extract(&txt2img_workflow());
        let width = result
            .bindings
            .iter()
            .find(|b| b.field.as_str() == "image.dimensions.width")
            .expect("width binding");
        assert_eq!(width.default, Some(serde_json::json!(512)));
        let height = result
            .bindings
            .iter()
            .find(|b| b.field.as_str() == "image.dimensions.height")
            .expect("height binding");
        assert_eq!(height.default, Some(serde_json::json!(768)));
    }

    #[test]
    fn extract_upscale_workflow_builds_source_and_model_selector() {
        let workflow = serde_json::json!({
            "1": { "class_type": "LoadImage", "inputs": { "image": "input.png" } },
            "2": { "class_type": "UpscaleModelLoader", "inputs": { "model_name": "RealESRGAN_x4plus.pth" } },
            "3": { "class_type": "ImageUpscaleWithModel", "inputs": { "upscale_model": ["2", 0], "image": ["1", 0] } },
            "4": { "class_type": "SaveImage", "inputs": { "images": ["3", 0] } }
        });
        let result = extract(&workflow);

        assert_eq!(result.workflow["1"]["inputs"]["image"], "PLACEHOLDER_IMAGE");
        assert_eq!(
            result.workflow["2"]["inputs"]["model_name"],
            "PLACEHOLDER_MODEL"
        );
        let source = result
            .bindings
            .iter()
            .find(|b| b.field.as_str() == "image.source")
            .expect("source binding");
        assert!(source.delivery.is_some());
        let sel = result.model_selector.expect("upscale model selector");
        assert_eq!(sel.placeholder, "PLACEHOLDER_MODEL");
        assert_eq!(sel.default.as_deref(), Some("RealESRGAN_x4plus.pth"));
    }
}
