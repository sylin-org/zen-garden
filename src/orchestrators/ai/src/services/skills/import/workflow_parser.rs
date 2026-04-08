//! ComfyUI workflow parser — extracts skill structure from an
//! API-format workflow JSON.
//!
//! Analyzes the node graph to identify:
//!
//! - **Input placeholders** — `PLACEHOLDER_*` strings the user must
//!   supply at dispatch time.
//! - **Model references** — which nodes load which model files
//!   (checkpoints, LoRAs, VAEs, upscalers, controlnet, CLIP).
//! - **Output nodes** — `SaveImage`, `PreviewImage`, etc.
//!
//! This is the foundation for the import pipeline's
//! [`super::param_extract`] pass, which walks the parsed workflow
//! and emits typed [`crate::services::skills::types::Binding`]s
//! ready for the loader.
//!
//! ORCH-0029 drops the prior Mermaid diagram generator — the
//! dashboard no longer renders workflow graphs, and the legacy
//! `diagram` field on `skill.json` is parsed but unused.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A parsed ComfyUI workflow — the raw analysis the import
/// pipeline consumes before producing a v3 skill definition.
#[derive(Debug, Clone)]
pub struct ParsedWorkflow {
    pub nodes: HashMap<String, ParsedNode>,
    pub inputs: Vec<WorkflowInput>,
    pub models: Vec<WorkflowModel>,
    pub outputs: Vec<WorkflowOutput>,
}

#[derive(Debug, Clone)]
pub struct ParsedNode {
    pub id: String,
    pub class_type: String,
    pub inputs: serde_json::Value,
}

/// An input the user must provide at invocation time — detected by
/// scanning `inputs` for string values starting with `PLACEHOLDER_`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowInput {
    pub node_id: String,
    pub field: String,
    pub placeholder: String,
    pub kind: InputKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputKind {
    Image,
    Text,
}

/// A model that must be installed for the workflow to run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowModel {
    pub node_id: String,
    pub model_name: String,
    /// ComfyUI model-dir name: `checkpoints`, `loras`, `upscale_models`,
    /// `vae`, `controlnet`, `clip`.
    pub model_type: String,
    /// `true` when the model name is itself a `PLACEHOLDER_*` string
    /// (the user will pick it at dispatch time via `selectors.model`).
    pub is_placeholder: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowOutput {
    pub node_id: String,
    pub kind: OutputKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputKind {
    Image,
}

/// Parse a ComfyUI API-format workflow JSON into structured components.
///
/// The workflow is a flat object: `{ "node_id": { "class_type": "...", "inputs": {...} } }`.
pub fn parse_workflow(workflow: &serde_json::Value) -> Result<ParsedWorkflow, String> {
    let obj = workflow
        .as_object()
        .ok_or_else(|| "workflow must be a JSON object".to_string())?;

    let mut nodes = HashMap::new();
    let mut inputs = Vec::new();
    let mut models = Vec::new();
    let mut outputs = Vec::new();

    for (node_id, node_value) in obj {
        let class_type = node_value
            .get("class_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("node '{}' missing class_type", node_id))?
            .to_string();

        let node_inputs = node_value
            .get("inputs")
            .cloned()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

        // Detect input placeholders.
        if let Some(input_map) = node_inputs.as_object() {
            for (field, value) in input_map {
                if let Some(s) = value.as_str() {
                    if s.starts_with("PLACEHOLDER_") {
                        let kind = infer_input_kind(&class_type, field);
                        inputs.push(WorkflowInput {
                            node_id: node_id.clone(),
                            field: field.clone(),
                            placeholder: s.to_string(),
                            kind,
                        });
                    }
                }
            }
        }

        // Detect model loaders.
        if let Some((model_type, model_field)) = classify_model_loader(&class_type) {
            let model_name = node_inputs
                .get(model_field)
                .and_then(|v| v.as_str())
                .unwrap_or("PLACEHOLDER_MODEL")
                .to_string();
            let is_placeholder = model_name.starts_with("PLACEHOLDER_");
            models.push(WorkflowModel {
                node_id: node_id.clone(),
                model_name,
                model_type: model_type.to_string(),
                is_placeholder,
            });
        }

        // Detect output nodes.
        if is_output_node(&class_type) {
            outputs.push(WorkflowOutput {
                node_id: node_id.clone(),
                kind: OutputKind::Image,
            });
        }

        nodes.insert(
            node_id.clone(),
            ParsedNode {
                id: node_id.clone(),
                class_type,
                inputs: node_inputs,
            },
        );
    }

    Ok(ParsedWorkflow {
        nodes,
        inputs,
        models,
        outputs,
    })
}

// ── Classification helpers ────────────────────────────────────

/// Map a `class_type` to its model category and the input field that
/// holds the model name. Used by both the parser (classification)
/// and the import pipeline's param extractor (to know where to plant
/// placeholders).
pub(crate) fn classify_model_loader(class_type: &str) -> Option<(&'static str, &'static str)> {
    match class_type {
        "UpscaleModelLoader" => Some(("upscale_models", "model_name")),
        "CheckpointLoaderSimple" | "CheckpointLoader" => Some(("checkpoints", "ckpt_name")),
        "LoraLoader" | "LoraLoaderModelOnly" => Some(("loras", "lora_name")),
        "VAELoader" => Some(("vae", "vae_name")),
        "CLIPLoader" => Some(("clip", "clip_name")),
        "ControlNetLoader" => Some(("controlnet", "control_net_name")),
        "unCLIPCheckpointLoader" => Some(("checkpoints", "ckpt_name")),
        _ => None,
    }
}

/// Determine if a node is an output / terminal node. The dispatcher
/// polls `/history/{prompt_id}` looking for a non-empty `images`
/// array under one of these node types.
pub(crate) fn is_output_node(class_type: &str) -> bool {
    matches!(
        class_type,
        "SaveImage" | "PreviewImage" | "SaveAnimatedWEBP" | "SaveAnimatedPNG"
    )
}

/// Infer input kind from the class type and field name.
fn infer_input_kind(class_type: &str, field: &str) -> InputKind {
    match (class_type, field) {
        ("LoadImage", _) => InputKind::Image,
        ("LoadImageMask", _) => InputKind::Image,
        ("CLIPTextEncode", "text") => InputKind::Text,
        (_, "image") => InputKind::Image,
        (_, "text") | (_, "prompt") => InputKind::Text,
        _ => InputKind::Text,
    }
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn upscale_workflow() -> serde_json::Value {
        serde_json::json!({
            "1": {
                "class_type": "LoadImage",
                "inputs": { "image": "PLACEHOLDER_IMAGE" }
            },
            "2": {
                "class_type": "UpscaleModelLoader",
                "inputs": { "model_name": "PLACEHOLDER_MODEL" }
            },
            "3": {
                "class_type": "ImageUpscaleWithModel",
                "inputs": {
                    "upscale_model": ["2", 0],
                    "image": ["1", 0]
                }
            },
            "4": {
                "class_type": "SaveImage",
                "inputs": {
                    "images": ["3", 0],
                    "filename_prefix": "zen-upscale"
                }
            }
        })
    }

    #[test]
    fn parse_upscale_workflow_nodes() {
        let parsed = parse_workflow(&upscale_workflow()).unwrap();
        assert_eq!(parsed.nodes.len(), 4);
        assert_eq!(parsed.nodes["1"].class_type, "LoadImage");
        assert_eq!(parsed.nodes["2"].class_type, "UpscaleModelLoader");
        assert_eq!(parsed.nodes["3"].class_type, "ImageUpscaleWithModel");
        assert_eq!(parsed.nodes["4"].class_type, "SaveImage");
    }

    #[test]
    fn parse_upscale_workflow_inputs() {
        let parsed = parse_workflow(&upscale_workflow()).unwrap();
        assert_eq!(parsed.inputs.len(), 2);

        let image_input = parsed
            .inputs
            .iter()
            .find(|i| i.kind == InputKind::Image)
            .unwrap();
        assert_eq!(image_input.node_id, "1");
        assert_eq!(image_input.placeholder, "PLACEHOLDER_IMAGE");

        let model_input = parsed
            .inputs
            .iter()
            .find(|i| i.placeholder == "PLACEHOLDER_MODEL")
            .unwrap();
        assert_eq!(model_input.node_id, "2");
    }

    #[test]
    fn parse_upscale_workflow_models() {
        let parsed = parse_workflow(&upscale_workflow()).unwrap();
        assert_eq!(parsed.models.len(), 1);
        assert_eq!(parsed.models[0].model_type, "upscale_models");
        assert_eq!(parsed.models[0].model_name, "PLACEHOLDER_MODEL");
        assert!(parsed.models[0].is_placeholder);
    }

    #[test]
    fn parse_upscale_workflow_outputs() {
        let parsed = parse_workflow(&upscale_workflow()).unwrap();
        assert_eq!(parsed.outputs.len(), 1);
        assert_eq!(parsed.outputs[0].kind, OutputKind::Image);
        assert_eq!(parsed.outputs[0].node_id, "4");
    }

    #[test]
    fn parse_workflow_with_fixed_model() {
        let wf = serde_json::json!({
            "1": { "class_type": "LoadImage", "inputs": { "image": "PLACEHOLDER_IMAGE" } },
            "2": { "class_type": "UpscaleModelLoader", "inputs": { "model_name": "4x-UltraSharp.pth" } },
            "3": { "class_type": "ImageUpscaleWithModel", "inputs": { "upscale_model": ["2", 0], "image": ["1", 0] } },
            "4": { "class_type": "SaveImage", "inputs": { "images": ["3", 0], "filename_prefix": "out" } }
        });

        let parsed = parse_workflow(&wf).unwrap();
        assert_eq!(parsed.models.len(), 1);
        assert_eq!(parsed.models[0].model_name, "4x-UltraSharp.pth");
        assert!(!parsed.models[0].is_placeholder);
    }

    #[test]
    fn parse_complex_generate_plus_upscale() {
        let wf = serde_json::json!({
            "3": {
                "class_type": "KSampler",
                "inputs": {
                    "model": ["4", 0],
                    "positive": ["6", 0],
                    "negative": ["7", 0],
                    "latent_image": ["5", 0]
                }
            },
            "4": {
                "class_type": "CheckpointLoaderSimple",
                "inputs": { "ckpt_name": "v1-5-pruned.ckpt" }
            },
            "5": {
                "class_type": "EmptyLatentImage",
                "inputs": { "width": 512, "height": 512, "batch_size": 1 }
            },
            "6": {
                "class_type": "CLIPTextEncode",
                "inputs": { "text": "PLACEHOLDER_PROMPT", "clip": ["4", 1] }
            },
            "7": {
                "class_type": "CLIPTextEncode",
                "inputs": { "text": "bad quality", "clip": ["4", 1] }
            },
            "8": {
                "class_type": "VAEDecode",
                "inputs": { "samples": ["3", 0], "vae": ["4", 2] }
            },
            "13": {
                "class_type": "UpscaleModelLoader",
                "inputs": { "model_name": "RealESRGAN_x2.pth" }
            },
            "14": {
                "class_type": "ImageUpscaleWithModel",
                "inputs": { "upscale_model": ["13", 0], "image": ["8", 0] }
            },
            "9": {
                "class_type": "SaveImage",
                "inputs": { "images": ["14", 0], "filename_prefix": "ComfyUI" }
            }
        });

        let parsed = parse_workflow(&wf).unwrap();
        assert_eq!(parsed.models.len(), 2);
        let checkpoint = parsed
            .models
            .iter()
            .find(|m| m.model_type == "checkpoints")
            .unwrap();
        assert_eq!(checkpoint.model_name, "v1-5-pruned.ckpt");
        let upscale = parsed
            .models
            .iter()
            .find(|m| m.model_type == "upscale_models")
            .unwrap();
        assert_eq!(upscale.model_name, "RealESRGAN_x2.pth");

        let text_input = parsed
            .inputs
            .iter()
            .find(|i| i.kind == InputKind::Text)
            .unwrap();
        assert_eq!(text_input.placeholder, "PLACEHOLDER_PROMPT");

        assert_eq!(parsed.outputs.len(), 1);
    }

    #[test]
    fn parse_empty_workflow_errors() {
        let result = parse_workflow(&serde_json::json!("not an object"));
        assert!(result.is_err());
    }

    #[test]
    fn classify_model_loaders() {
        assert_eq!(
            classify_model_loader("UpscaleModelLoader"),
            Some(("upscale_models", "model_name"))
        );
        assert_eq!(
            classify_model_loader("CheckpointLoaderSimple"),
            Some(("checkpoints", "ckpt_name"))
        );
        assert_eq!(
            classify_model_loader("LoraLoader"),
            Some(("loras", "lora_name"))
        );
        assert_eq!(
            classify_model_loader("VAELoader"),
            Some(("vae", "vae_name"))
        );
        assert_eq!(classify_model_loader("ImageUpscaleWithModel"), None);
    }
}
