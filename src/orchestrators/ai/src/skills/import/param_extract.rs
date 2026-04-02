//! Parameter extraction — walk workflow nodes, identify tunable values,
//! replace with placeholders, generate mappings.
//!
//! This turns a raw workflow (with hardcoded values) into a skill template
//! (with placeholders and mappings).

use crate::domain::skill::SkillMapping;

/// Result of parameter extraction.
pub struct ExtractionResult {
    /// The workflow with tunable values replaced by placeholders or marked for node targeting.
    pub workflow: serde_json::Value,
    /// Generated mappings (content + param).
    pub mappings: Vec<SkillMapping>,
    /// Detected content slots.
    pub content_slots: Vec<ContentSlotDetection>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ContentSlotDetection {
    pub role: String,
    pub content_type: String,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overlay: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

/// Extract parameters from a ComfyUI API-format workflow.
///
/// Walks every node, identifies tunable values by class_type,
/// replaces string values with `PLACEHOLDER_*` tokens,
/// generates SkillMapping entries for each.
pub fn extract(workflow: &serde_json::Value) -> ExtractionResult {
    let mut workflow = workflow.clone();
    let mut mappings: Vec<SkillMapping> = Vec::new();
    let mut content_slots: Vec<ContentSlotDetection> = Vec::new();

    // Track what we've seen to avoid duplicates and assign roles
    let mut image_count = 0;
    let mut text_count = 0;
    let mut has_negative = false;

    let obj = match workflow.as_object() {
        Some(o) => o.clone(),
        None => return ExtractionResult { workflow, mappings, content_slots },
    };

    // Sort node IDs for deterministic output
    let mut node_ids: Vec<String> = obj.keys().cloned().collect();
    node_ids.sort_by(|a, b| {
        a.parse::<u32>().unwrap_or(u32::MAX).cmp(&b.parse::<u32>().unwrap_or(u32::MAX))
    });

    for node_id in &node_ids {
        let node = match obj.get(node_id) {
            Some(n) => n,
            None => continue,
        };

        let class_type = match node.get("class_type").and_then(|v| v.as_str()) {
            Some(ct) => ct.to_string(),
            None => continue,
        };

        match class_type.as_str() {
            // ── Image loaders ─────────────────────────────────
            "LoadImage" => {
                let role = if image_count == 0 { "source" } else { "mask" };
                let placeholder = if image_count == 0 { "PLACEHOLDER_IMAGE" } else { "PLACEHOLDER_MASK" };

                set_input_str(&mut workflow, node_id, "image", placeholder);

                mappings.push(SkillMapping::Content {
                    role: role.into(),
                    content_type: crate::domain::skill::ContentType::Image,
                    placeholder: placeholder.into(),
                });

                let overlay = if image_count > 0 { Some("source".into()) } else { None };
                content_slots.push(ContentSlotDetection {
                    role: role.into(),
                    content_type: "image".into(),
                    required: true,
                    overlay,
                    default: None,
                });

                image_count += 1;
            }

            "LoadImageMask" => {
                let placeholder = "PLACEHOLDER_MASK";
                set_input_str(&mut workflow, node_id, "image", placeholder);

                mappings.push(SkillMapping::Content {
                    role: "mask".into(),
                    content_type: crate::domain::skill::ContentType::Image,
                    placeholder: placeholder.into(),
                });

                content_slots.push(ContentSlotDetection {
                    role: "mask".into(),
                    content_type: "image".into(),
                    required: true,
                    overlay: Some("source".into()),
                    default: None,
                });
            }

            // ── Checkpoint loaders ────────────────────────────
            "CheckpointLoaderSimple" => {
                let placeholder = "PLACEHOLDER_CHECKPOINT";
                let current = get_input_str(&workflow, node_id, "ckpt_name");
                set_input_str(&mut workflow, node_id, "ckpt_name", placeholder);

                mappings.push(SkillMapping::Param {
                    field: "checkpoint".into(),
                    label: "Model".into(),
                    node: None,
                    input: None,
                    placeholder: Some(placeholder.into()),
                    param_type: crate::domain::skill::ParamType::Options {
                        options: if let Some(name) = &current {
                            vec![crate::domain::skill::ParamOption::simple(name.as_str())]
                        } else {
                            vec![]
                        },
                    },
                    default: current.map(|v| serde_json::Value::String(v)),
                });
            }

            // ── LoRA loaders ──────────────────────────────────
            "LoraLoader" => {
                let placeholder = "PLACEHOLDER_LORA";
                let current = get_input_str(&workflow, node_id, "lora_name");
                set_input_str(&mut workflow, node_id, "lora_name", placeholder);

                mappings.push(SkillMapping::Param {
                    field: "lora".into(),
                    label: "LoRA".into(),
                    node: None,
                    input: None,
                    placeholder: Some(placeholder.into()),
                    param_type: crate::domain::skill::ParamType::Options {
                        options: if let Some(name) = &current {
                            vec![crate::domain::skill::ParamOption::simple(name.as_str())]
                        } else {
                            vec![]
                        },
                    },
                    default: current.map(|v| serde_json::Value::String(v)),
                });
            }

            // ── Upscale model loaders ─────────────────────────
            "UpscaleModelLoader" => {
                let placeholder = "PLACEHOLDER_MODEL";
                let current = get_input_str(&workflow, node_id, "model_name");
                set_input_str(&mut workflow, node_id, "model_name", placeholder);

                mappings.push(SkillMapping::Param {
                    field: "upscale_model".into(),
                    label: "Upscale Model".into(),
                    node: None,
                    input: None,
                    placeholder: Some(placeholder.into()),
                    param_type: crate::domain::skill::ParamType::Options {
                        options: if let Some(name) = &current {
                            vec![crate::domain::skill::ParamOption::simple(name.as_str())]
                        } else {
                            vec![]
                        },
                    },
                    default: current.map(|v| serde_json::Value::String(v)),
                });
            }

            // ── Text encoders ─────────────────────────────────
            "CLIPTextEncode" => {
                let current_text = get_input_str(&workflow, node_id, "text");

                // Heuristic: if the node feeds into a "negative" input of KSampler,
                // or if we already have a prompt, this is the negative.
                let is_negative = is_negative_encoder(&obj, node_id) || text_count > 0;

                if is_negative && !has_negative {
                    let placeholder = "PLACEHOLDER_NEGATIVE";
                    set_input_str(&mut workflow, node_id, "text", placeholder);

                    // Negative prompt is a Content slot (user-provided text) with a default
                    mappings.push(SkillMapping::Content {
                        role: "negative".into(),
                        content_type: crate::domain::skill::ContentType::Text,
                        placeholder: placeholder.into(),
                    });

                    content_slots.push(ContentSlotDetection {
                        role: "negative".into(),
                        content_type: "text".into(),
                        required: false,
                        overlay: None,
                        default: current_text,
                    });

                    has_negative = true;
                } else if !is_negative && text_count == 0 {
                    let placeholder = "PLACEHOLDER_PROMPT";
                    set_input_str(&mut workflow, node_id, "text", placeholder);

                    mappings.push(SkillMapping::Content {
                        role: "prompt".into(),
                        content_type: crate::domain::skill::ContentType::Text,
                        placeholder: placeholder.into(),
                    });

                    content_slots.push(ContentSlotDetection {
                        role: "prompt".into(),
                        content_type: "text".into(),
                        required: true,
                        overlay: None,
                        default: None,
                    });
                }

                text_count += 1;
            }

            // ── KSampler ──────────────────────────────────────
            "KSampler" | "KSamplerAdvanced" => {
                // Steps
                let steps = get_input_number(&workflow, node_id, "steps");
                mappings.push(SkillMapping::Param {
                    field: "steps".into(),
                    label: "Steps".into(),
                    node: Some(node_id.clone()),
                    input: Some("steps".into()),
                    placeholder: None,
                    param_type: crate::domain::skill::ParamType::Range {
                        min: 1.0, max: 50.0, step: Some(1.0),
                    },
                    default: steps.map(|v| serde_json::json!(v)),
                });

                // CFG
                let cfg = get_input_float(&workflow, node_id, "cfg");
                mappings.push(SkillMapping::Param {
                    field: "cfg".into(),
                    label: "CFG Scale".into(),
                    node: Some(node_id.clone()),
                    input: Some("cfg".into()),
                    placeholder: None,
                    param_type: crate::domain::skill::ParamType::Range {
                        min: 1.0, max: 30.0, step: Some(0.5),
                    },
                    default: cfg.map(|v| serde_json::json!(v)),
                });

                // Seed
                mappings.push(SkillMapping::Param {
                    field: "seed".into(),
                    label: "Seed".into(),
                    node: Some(node_id.clone()),
                    input: Some("seed".into()),
                    placeholder: None,
                    param_type: crate::domain::skill::ParamType::Auto {
                        kind: crate::domain::skill::AutoKind::RandomInt,
                    },
                    default: None,
                });

                // Denoise (present in img2img/inpaint workflows)
                let denoise = get_input_float(&workflow, node_id, "denoise");
                if let Some(d) = denoise {
                    if d < 1.0 {
                        // Only expose if it's not 1.0 (full denoise = txt2img, not tunable)
                        mappings.push(SkillMapping::Param {
                            field: "strength".into(),
                            label: "Strength".into(),
                            node: Some(node_id.clone()),
                            input: Some("denoise".into()),
                            placeholder: None,
                            param_type: crate::domain::skill::ParamType::Range {
                                min: 0.0, max: 1.0, step: Some(0.05),
                            },
                            default: Some(serde_json::json!(d)),
                        });
                    }
                }
            }

            // ── Empty latent image (txt2img dimensions) ───────
            "EmptyLatentImage" => {
                let width = get_input_number(&workflow, node_id, "width");
                let height = get_input_number(&workflow, node_id, "height");

                mappings.push(SkillMapping::Param {
                    field: "width".into(),
                    label: "Width".into(),
                    node: Some(node_id.clone()),
                    input: Some("width".into()),
                    placeholder: None,
                    param_type: crate::domain::skill::ParamType::Options {
                        options: common_size_options(),
                    },
                    default: width.map(|v| serde_json::json!(v)),
                });

                mappings.push(SkillMapping::Param {
                    field: "height".into(),
                    label: "Height".into(),
                    node: Some(node_id.clone()),
                    input: Some("height".into()),
                    placeholder: None,
                    param_type: crate::domain::skill::ParamType::Options {
                        options: common_size_options(),
                    },
                    default: height.map(|v| serde_json::json!(v)),
                });
            }

            _ => {} // Unknown node types are left as-is
        }
    }

    ExtractionResult { workflow, mappings, content_slots }
}

// ── Helpers ───────────────────────────────────────────────────

fn get_input_str(workflow: &serde_json::Value, node_id: &str, field: &str) -> Option<String> {
    workflow.get(node_id)?
        .get("inputs")?
        .get(field)?
        .as_str()
        .map(String::from)
}

fn get_input_number(workflow: &serde_json::Value, node_id: &str, field: &str) -> Option<u64> {
    workflow.get(node_id)?
        .get("inputs")?
        .get(field)?
        .as_u64()
}

fn get_input_float(workflow: &serde_json::Value, node_id: &str, field: &str) -> Option<f64> {
    workflow.get(node_id)?
        .get("inputs")?
        .get(field)?
        .as_f64()
}

fn set_input_str(workflow: &mut serde_json::Value, node_id: &str, field: &str, value: &str) {
    if let Some(inputs) = workflow
        .get_mut(node_id)
        .and_then(|n| n.get_mut("inputs"))
        .and_then(|i| i.as_object_mut())
    {
        inputs.insert(field.to_string(), serde_json::Value::String(value.to_string()));
    }
}

/// Check if a CLIPTextEncode node feeds into the "negative" input of a KSampler.
fn is_negative_encoder(nodes: &serde_json::Map<String, serde_json::Value>, encoder_node_id: &str) -> bool {
    for node in nodes.values() {
        let class_type = node.get("class_type").and_then(|v| v.as_str()).unwrap_or("");
        if class_type.contains("KSampler") {
            if let Some(inputs) = node.get("inputs") {
                if let Some(neg) = inputs.get("negative") {
                    if let Some(arr) = neg.as_array() {
                        if arr.first().and_then(|v| v.as_str()) == Some(encoder_node_id) {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

fn common_size_options() -> Vec<crate::domain::skill::ParamOption> {
    use crate::domain::skill::ParamOption;
    vec![
        ParamOption::simple(512),
        ParamOption::simple(768),
        ParamOption::simple(1024),
        ParamOption::simple(1280),
        ParamOption::simple(1536),
        ParamOption::simple(2048),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_txt2img_workflow() {
        let workflow = serde_json::json!({
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
        });

        let result = extract(&workflow);

        // Should have: content(prompt), param(checkpoint), param(negative), param(steps), param(cfg), param(seed), param(width), param(height)
        assert!(result.mappings.len() >= 6, "got {} mappings", result.mappings.len());

        // Checkpoint should be a placeholder
        assert_eq!(result.workflow["1"]["inputs"]["ckpt_name"], "PLACEHOLDER_CHECKPOINT");

        // Prompt should be a placeholder
        assert_eq!(result.workflow["2"]["inputs"]["text"], "PLACEHOLDER_PROMPT");

        // Negative should be a placeholder
        assert_eq!(result.workflow["3"]["inputs"]["text"], "PLACEHOLDER_NEGATIVE");

        // Content slots should have prompt
        assert!(result.content_slots.iter().any(|s| s.role == "prompt"));
    }

    #[test]
    fn extract_upscale_workflow() {
        let workflow = serde_json::json!({
            "1": { "class_type": "LoadImage", "inputs": { "image": "input.png" } },
            "2": { "class_type": "UpscaleModelLoader", "inputs": { "model_name": "RealESRGAN_x4plus.pth" } },
            "3": { "class_type": "ImageUpscaleWithModel", "inputs": { "upscale_model": ["2", 0], "image": ["1", 0] } },
            "4": { "class_type": "SaveImage", "inputs": { "images": ["3", 0] } }
        });

        let result = extract(&workflow);

        // Image input + upscale model param
        assert!(result.content_slots.iter().any(|s| s.role == "source"));
        assert_eq!(result.workflow["1"]["inputs"]["image"], "PLACEHOLDER_IMAGE");
        assert_eq!(result.workflow["2"]["inputs"]["model_name"], "PLACEHOLDER_MODEL");
    }

    #[test]
    fn negative_encoder_detection() {
        let nodes: serde_json::Map<String, serde_json::Value> = serde_json::from_value(serde_json::json!({
            "2": { "class_type": "CLIPTextEncode", "inputs": { "text": "positive" } },
            "3": { "class_type": "CLIPTextEncode", "inputs": { "text": "negative" } },
            "4": { "class_type": "KSampler", "inputs": { "positive": ["2", 0], "negative": ["3", 0] } }
        })).unwrap();

        assert!(!is_negative_encoder(&nodes, "2")); // positive encoder
        assert!(is_negative_encoder(&nodes, "3"));  // negative encoder
    }
}
