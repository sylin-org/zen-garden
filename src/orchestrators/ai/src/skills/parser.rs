//! ComfyUI workflow parser — extracts skill structure from workflow JSON.
//!
//! Analyzes the node graph to identify:
//! - Input nodes: what the user must provide (images, text)
//! - Model nodes: what models must be installed
//! - Output nodes: what the workflow produces
//! - Graph structure: for Mermaid diagram generation
//!
//! This is the foundation for both built-in skills (authored by us) and
//! imported skills (community workflows uploaded by users).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A parsed ComfyUI workflow — the raw analysis before it becomes a SkillDefinition.
#[derive(Debug, Clone)]
pub struct ParsedWorkflow {
    /// All nodes in the workflow, keyed by node ID.
    pub nodes: HashMap<String, ParsedNode>,
    /// Input placeholders discovered (LoadImage, text inputs, etc.).
    pub inputs: Vec<WorkflowInput>,
    /// Model references discovered (UpscaleModelLoader, CheckpointLoader, etc.).
    pub models: Vec<WorkflowModel>,
    /// Output nodes discovered (SaveImage, etc.).
    pub outputs: Vec<WorkflowOutput>,
    /// Generated Mermaid diagram of the workflow graph.
    pub diagram: String,
}

/// A single node in the parsed workflow.
#[derive(Debug, Clone)]
pub struct ParsedNode {
    pub id: String,
    pub class_type: String,
    pub inputs: serde_json::Value,
    /// Human-readable label for diagram.
    pub label: String,
}

/// An input the user must provide at invocation time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowInput {
    /// Node ID containing this input.
    pub node_id: String,
    /// Input field name (e.g., "image", "text").
    pub field: String,
    /// Placeholder value in the template (e.g., "PLACEHOLDER_IMAGE").
    pub placeholder: String,
    /// Detected input kind.
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
    /// Node ID of the loader.
    pub node_id: String,
    /// Model filename or placeholder.
    pub model_name: String,
    /// Model category (e.g., "upscale_models", "checkpoints", "loras").
    pub model_type: String,
    /// Whether this is a placeholder to be filled at runtime.
    pub is_placeholder: bool,
}

/// An output the workflow produces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowOutput {
    /// Node ID of the output node.
    pub node_id: String,
    /// Output kind.
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
    let mut edges: Vec<(String, String)> = Vec::new();

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

        let label = class_type_to_label(&class_type);

        // Detect input placeholders
        if let Some(input_map) = node_inputs.as_object() {
            for (field, value) in input_map {
                // Check for placeholder strings (our convention: PLACEHOLDER_*)
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

                // Detect edges: ["other_node_id", output_index]
                if let Some(arr) = value.as_array() {
                    if arr.len() == 2 {
                        if let Some(source_id) = arr[0].as_str() {
                            edges.push((source_id.to_string(), node_id.clone()));
                        }
                    }
                }
            }
        }

        // Detect model loaders
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

        // Detect output nodes
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
                label,
            },
        );
    }

    let diagram = generate_mermaid(&nodes, &edges);

    Ok(ParsedWorkflow {
        nodes,
        inputs,
        models,
        outputs,
        diagram,
    })
}

// ── Classification helpers ─────────────────────────────────────

/// Map a class_type to its model category and the input field that holds the model name.
fn classify_model_loader(class_type: &str) -> Option<(&'static str, &'static str)> {
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

/// Determine if a node is an output/terminal node.
fn is_output_node(class_type: &str) -> bool {
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
        _ => InputKind::Text, // default to text
    }
}

/// Generate a human-readable label from a ComfyUI class_type.
fn class_type_to_label(class_type: &str) -> String {
    match class_type {
        "LoadImage" => "Load Image".into(),
        "UpscaleModelLoader" => "Load Upscale Model".into(),
        "ImageUpscaleWithModel" => "Upscale".into(),
        "SaveImage" => "Save Image".into(),
        "PreviewImage" => "Preview".into(),
        "CheckpointLoaderSimple" => "Load Checkpoint".into(),
        "CLIPTextEncode" => "CLIP Encode".into(),
        "KSampler" => "KSampler".into(),
        "VAEDecode" => "VAE Decode".into(),
        "VAEEncode" => "VAE Encode".into(),
        "EmptyLatentImage" => "Empty Latent".into(),
        "ControlNetLoader" => "Load ControlNet".into(),
        "LoraLoader" => "Load LoRA".into(),
        other => other.to_string(),
    }
}

// ── Mermaid Generation ─────────────────────────────────────────

/// Generate a Mermaid graph from the parsed nodes and edges.
/// Core pipeline node types that belong in a diagram.
fn is_pipeline_node(class_type: &str) -> bool {
    let lower = class_type.to_lowercase();

    // Loaders (model, image, audio)
    if lower.contains("loader") || lower.contains("load image") || lower.contains("loadimage") {
        return true;
    }
    // Samplers
    if lower.contains("sampler") && !lower.contains("select") {
        return true;
    }
    // Encoders / decoders
    if lower.contains("encode") || lower.contains("decode") {
        return true;
    }
    // Output nodes
    if lower.contains("save") || lower.contains("preview") {
        return true;
    }
    // Latent operations
    if lower.starts_with("empty") || lower.contains("latent") {
        return true;
    }
    // Conditioning
    if lower.contains("conditioning") || lower.contains("controlnet") {
        return true;
    }
    // Inpaint
    if lower.contains("inpaint") {
        return true;
    }
    // Upscale / scale
    if lower.contains("upscale") || lower.contains("imagescale") {
        return true;
    }

    false
}

/// Sanitize a label for Mermaid (escape special chars).
/// Mermaid uses [] for node shapes, so inner brackets must be stripped.
fn mermaid_safe(label: &str) -> String {
    label
        .replace(['[', ']', '(', ')'], "")
        .replace(['+', '|', '"', '<', '>', '{', '}'], "")
        .replace("  ", " ")
        .trim()
        .to_string()
}

const MAX_DIAGRAM_NODES: usize = 30;

fn generate_mermaid(
    nodes: &HashMap<String, ParsedNode>,
    edges: &[(String, String)],
) -> String {
    let mut lines = vec!["graph LR".to_string()];

    // Filter to pipeline nodes only
    let pipeline: std::collections::HashSet<&String> = nodes.keys()
        .filter(|id| nodes.get(*id).map(|n| is_pipeline_node(&n.class_type)).unwrap_or(false))
        .collect();

    // If still too large, truncate to the most-connected nodes
    let included: std::collections::HashSet<&String> = if pipeline.len() > MAX_DIAGRAM_NODES {
        // Count edges per node, keep the most connected
        let mut edge_count: HashMap<&String, usize> = HashMap::new();
        for (from, to) in edges {
            if pipeline.contains(from) { *edge_count.entry(from).or_default() += 1; }
            if pipeline.contains(to) { *edge_count.entry(to).or_default() += 1; }
        }
        let mut ranked: Vec<_> = pipeline.iter().collect();
        ranked.sort_by(|a, b| edge_count.get(*b).unwrap_or(&0).cmp(&edge_count.get(*a).unwrap_or(&0)));
        ranked.into_iter().take(MAX_DIAGRAM_NODES).copied().collect()
    } else {
        pipeline
    };

    // Sort for deterministic output
    let mut sorted_ids: Vec<&&String> = included.iter().collect();
    sorted_ids.sort();

    // Node declarations — use quoted labels to handle special chars
    for id in &sorted_ids {
        if let Some(node) = nodes.get(**id) {
            let label = mermaid_safe(&node.label);
            lines.push(format!("    {}[\"{}\"]", id, label));
        }
    }

    // Edges — only between included nodes
    let mut seen_edges = std::collections::HashSet::new();
    for (from, to) in edges {
        if included.contains(from) && included.contains(to) {
            let key = format!("{}->{}", from, to);
            if seen_edges.insert(key) {
                lines.push(format!("    {} --> {}", from, to));
            }
        }
    }

    lines.join("\n")
}

// ── Tests ──────────────────────────────────────────────────────

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

        let image_input = parsed.inputs.iter().find(|i| i.kind == InputKind::Image).unwrap();
        assert_eq!(image_input.node_id, "1");
        assert_eq!(image_input.field, "image");
        assert_eq!(image_input.placeholder, "PLACEHOLDER_IMAGE");

        // Model name is also a placeholder input
        let model_input = parsed.inputs.iter().find(|i| i.placeholder == "PLACEHOLDER_MODEL").unwrap();
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
    fn parse_upscale_workflow_edges() {
        let parsed = parse_workflow(&upscale_workflow()).unwrap();
        assert!(parsed.diagram.contains("graph LR"));
        assert!(parsed.diagram.contains("1[\"Load Image\"]"));
        assert!(parsed.diagram.contains("2[\"Load Upscale Model\"]"));
        assert!(parsed.diagram.contains("3[\"Upscale\"]"));
        assert!(parsed.diagram.contains("4[\"Save Image\"]"));
        // Edges: 1→3, 2→3, 3→4
        assert!(parsed.diagram.contains("1 --> 3"));
        assert!(parsed.diagram.contains("2 --> 3"));
        assert!(parsed.diagram.contains("3 --> 4"));
    }

    #[test]
    fn parse_workflow_with_fixed_model() {
        let wf = serde_json::json!({
            "1": {
                "class_type": "LoadImage",
                "inputs": { "image": "PLACEHOLDER_IMAGE" }
            },
            "2": {
                "class_type": "UpscaleModelLoader",
                "inputs": { "model_name": "4x-UltraSharp.pth" }
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
                "inputs": { "images": ["3", 0], "filename_prefix": "out" }
            }
        });

        let parsed = parse_workflow(&wf).unwrap();
        assert_eq!(parsed.models.len(), 1);
        assert_eq!(parsed.models[0].model_name, "4x-UltraSharp.pth");
        assert!(!parsed.models[0].is_placeholder); // Fixed, not a placeholder
    }

    #[test]
    fn parse_complex_generate_plus_upscale() {
        // Simplified version of the official ComfyUI example
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

        // Should find 2 models: checkpoint + upscale
        assert_eq!(parsed.models.len(), 2);
        let checkpoint = parsed.models.iter().find(|m| m.model_type == "checkpoints").unwrap();
        assert_eq!(checkpoint.model_name, "v1-5-pruned.ckpt");
        let upscale = parsed.models.iter().find(|m| m.model_type == "upscale_models").unwrap();
        assert_eq!(upscale.model_name, "RealESRGAN_x2.pth");

        // Should find 1 text input placeholder
        let text_input = parsed.inputs.iter().find(|i| i.kind == InputKind::Text).unwrap();
        assert_eq!(text_input.placeholder, "PLACEHOLDER_PROMPT");

        // Should find 1 output
        assert_eq!(parsed.outputs.len(), 1);

        // Mermaid should have all nodes
        assert!(parsed.diagram.contains("KSampler"));
        assert!(parsed.diagram.contains("Load Checkpoint"));
        assert!(parsed.diagram.contains("Upscale"));
    }

    #[test]
    fn parse_empty_workflow_errors() {
        let result = parse_workflow(&serde_json::json!("not an object"));
        assert!(result.is_err());
    }

    #[test]
    fn classify_model_loaders() {
        assert_eq!(classify_model_loader("UpscaleModelLoader"), Some(("upscale_models", "model_name")));
        assert_eq!(classify_model_loader("CheckpointLoaderSimple"), Some(("checkpoints", "ckpt_name")));
        assert_eq!(classify_model_loader("LoraLoader"), Some(("loras", "lora_name")));
        assert_eq!(classify_model_loader("VAELoader"), Some(("vae", "vae_name")));
        assert_eq!(classify_model_loader("ImageUpscaleWithModel"), None);
    }

    #[test]
    fn mermaid_diagram_is_deterministic() {
        let wf = upscale_workflow();
        let p1 = parse_workflow(&wf).unwrap();
        let p2 = parse_workflow(&wf).unwrap();
        assert_eq!(p1.diagram, p2.diagram);
    }
}
