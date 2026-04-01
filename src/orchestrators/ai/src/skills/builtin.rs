//! Built-in skill definitions — official, curated, embedded in the orchestrator.
//!
//! Each built-in skill is a workflow template + parameter schema + metadata.
//! The parser validates the template structure at startup.

use crate::catalog::traits::FormSchema;
use crate::domain::skill::{
    ContentSlot, ContentType, ModelRef, SkillDefinition,
};
use crate::domain::types::Capability;
use super::parser;

/// The embedded upscale workflow template (API format).
const UPSCALE_WORKFLOW_JSON: &str = include_str!("upscale_workflow.json");

/// Build the `image.upscale` skill definition.
///
/// The `available_models` parameter comes from the ComfyUI instance's
/// `/models/upscale_models` endpoint. The skill is only published when
/// at least one model is installed.
pub fn image_upscale(available_models: &[String]) -> SkillDefinition {
    let workflow: serde_json::Value =
        serde_json::from_str(UPSCALE_WORKFLOW_JSON).expect("embedded upscale workflow is valid JSON");

    // Parse the workflow to extract structure and generate the Mermaid diagram
    let parsed = parser::parse_workflow(&workflow)
        .expect("embedded upscale workflow parses correctly");

    // Build the model enum — use installed models if any, otherwise recommended
    let effective_models: Vec<String> = if available_models.is_empty() {
        recommended_upscale_models()
            .iter()
            .map(|m| m.filename.clone())
            .collect()
    } else {
        available_models.to_vec()
    };

    let model_enum: Vec<serde_json::Value> = effective_models
        .iter()
        .map(|m| serde_json::Value::String(m.clone()))
        .collect();
    let default_model = effective_models
        .iter()
        .find(|m| m.contains("RealESRGAN_x4plus"))
        .or_else(|| effective_models.first())
        .cloned()
        .unwrap_or_default();

    SkillDefinition {
        name: "image.upscale".into(),
        display_name: "Upscale".into(),
        capability: Capability::Image,
        description: "Enhance image resolution using AI super-resolution".into(),
        status: crate::domain::skill::SkillStatus::Initializing,
        vram_mb: 1024,
        content_slots: vec![ContentSlot {
            role: "source".into(),
            content_type: ContentType::Image,
            required: true,
        }],
        parameter_schema: FormSchema {
            schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "scale": {
                        "type": "integer",
                        "title": "Scale Factor",
                        "description": "Output size multiplier",
                        "enum": [2, 4],
                        "default": 4
                    },
                    "upscale_model": {
                        "type": "string",
                        "title": "Model",
                        "description": "AI model used for upscaling. RealESRGAN_x4plus is recommended for general use.",
                        "enum": model_enum,
                        "default": default_model
                    }
                }
            }),
            ui_schema: serde_json::json!({
                "scale": { "ui:widget": "radio" },
                "upscale_model": { "ui:widget": "select" }
            }),
        },
        diagram: Some(parsed.diagram),
        required_models: recommended_upscale_models()
            .into_iter()
            .map(|m| ModelRef {
                filename: m.filename,
                model_type: m.model_type,
                description: Some(m.description),
            })
            .collect(),
        implementation: workflow,
    }
}

// ── image.generate ─────────────────────────────────────────────

const GENERATE_WORKFLOW_JSON: &str = include_str!("generate_workflow.json");

/// Build the `image.generate` skill definition (text-to-image).
pub fn image_generate(available_checkpoints: &[String]) -> SkillDefinition {
    let workflow: serde_json::Value =
        serde_json::from_str(GENERATE_WORKFLOW_JSON).expect("embedded generate workflow is valid JSON");

    let parsed = parser::parse_workflow(&workflow)
        .expect("embedded generate workflow parses correctly");

    let effective_checkpoints: Vec<String> = if available_checkpoints.is_empty() {
        recommended_checkpoint_models()
            .iter()
            .map(|m| m.filename.clone())
            .collect()
    } else {
        available_checkpoints.to_vec()
    };

    let checkpoint_enum: Vec<serde_json::Value> = effective_checkpoints
        .iter()
        .map(|m| serde_json::Value::String(m.clone()))
        .collect();
    let default_checkpoint = effective_checkpoints.first().cloned().unwrap_or_default();

    SkillDefinition {
        name: "image.generate".into(),
        display_name: "Generate".into(),
        capability: Capability::Image,
        description: "Create an image from a text description".into(),
        status: crate::domain::skill::SkillStatus::Initializing,
        vram_mb: 4096,
        content_slots: vec![
            ContentSlot {
                role: "prompt".into(),
                content_type: ContentType::Text,
                required: true,
            },
        ],
        parameter_schema: FormSchema {
            schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "negative": {
                        "type": "string",
                        "title": "Negative Prompt",
                        "description": "What to avoid in the generated image",
                        "default": "blurry, watermark, low quality, deformed"
                    },
                    "width": {
                        "type": "integer",
                        "title": "Width",
                        "enum": [512, 768, 1024],
                        "default": 512
                    },
                    "height": {
                        "type": "integer",
                        "title": "Height",
                        "enum": [512, 768, 1024],
                        "default": 512
                    },
                    "steps": {
                        "type": "integer",
                        "title": "Steps",
                        "description": "More steps = higher quality, slower",
                        "minimum": 1,
                        "maximum": 50,
                        "default": 20
                    },
                    "checkpoint": {
                        "type": "string",
                        "title": "Model",
                        "enum": checkpoint_enum,
                        "default": default_checkpoint
                    }
                },
                "required": ["negative"]
            }),
            ui_schema: serde_json::json!({
                "negative": { "ui:widget": "textarea", "ui:options": { "rows": 2 } },
                "width": { "ui:widget": "select" },
                "height": { "ui:widget": "select" },
                "steps": { "ui:widget": "range" },
                "checkpoint": { "ui:widget": "select" }
            }),
        },
        diagram: Some(parsed.diagram),
        required_models: recommended_checkpoint_models()
            .into_iter()
            .map(|m| ModelRef {
                filename: m.filename,
                model_type: m.model_type,
                description: Some(m.description),
            })
            .collect(),
        implementation: workflow,
    }
}

// ── image.img2img ──────────────────────────────────────────────

const IMG2IMG_WORKFLOW_JSON: &str = include_str!("img2img_workflow.json");

/// Build the `image.img2img` skill definition (image + prompt → transformed image).
pub fn image_img2img(available_checkpoints: &[String]) -> SkillDefinition {
    let workflow: serde_json::Value =
        serde_json::from_str(IMG2IMG_WORKFLOW_JSON).expect("embedded img2img workflow is valid JSON");

    let parsed = parser::parse_workflow(&workflow)
        .expect("embedded img2img workflow parses correctly");

    let effective_checkpoints: Vec<String> = if available_checkpoints.is_empty() {
        recommended_checkpoint_models()
            .iter()
            .map(|m| m.filename.clone())
            .collect()
    } else {
        available_checkpoints.to_vec()
    };

    let checkpoint_enum: Vec<serde_json::Value> = effective_checkpoints
        .iter()
        .map(|m| serde_json::Value::String(m.clone()))
        .collect();
    let default_checkpoint = effective_checkpoints.first().cloned().unwrap_or_default();

    SkillDefinition {
        name: "image.img2img".into(),
        display_name: "Transform".into(),
        capability: Capability::Image,
        description: "Transform an image guided by a text prompt".into(),
        status: crate::domain::skill::SkillStatus::Initializing,
        vram_mb: 4096,
        content_slots: vec![
            ContentSlot {
                role: "source".into(),
                content_type: ContentType::Image,
                required: true,
            },
            ContentSlot {
                role: "prompt".into(),
                content_type: ContentType::Text,
                required: true,
            },
        ],
        parameter_schema: FormSchema {
            schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "strength": {
                        "type": "number",
                        "title": "Strength",
                        "description": "How much to transform (0.0 = no change, 1.0 = full generation)",
                        "minimum": 0.0,
                        "maximum": 1.0,
                        "default": 0.7
                    },
                    "negative": {
                        "type": "string",
                        "title": "Negative Prompt",
                        "default": "blurry, watermark, low quality"
                    },
                    "steps": {
                        "type": "integer",
                        "title": "Steps",
                        "minimum": 1,
                        "maximum": 50,
                        "default": 20
                    },
                    "checkpoint": {
                        "type": "string",
                        "title": "Model",
                        "enum": checkpoint_enum,
                        "default": default_checkpoint
                    }
                }
            }),
            ui_schema: serde_json::json!({
                "strength": { "ui:widget": "range" },
                "negative": { "ui:widget": "textarea", "ui:options": { "rows": 2 } },
                "steps": { "ui:widget": "range" },
                "checkpoint": { "ui:widget": "select" }
            }),
        },
        diagram: Some(parsed.diagram),
        required_models: recommended_checkpoint_models()
            .into_iter()
            .map(|m| ModelRef {
                filename: m.filename,
                model_type: m.model_type,
                description: Some(m.description),
            })
            .collect(),
        implementation: workflow,
    }
}

// ── Recommended Models ─────────────────────────────────────────

/// Recommended checkpoint models for generation skills.
pub fn recommended_checkpoint_models() -> Vec<RecommendedModel> {
    vec![RecommendedModel {
        filename: "v1-5-pruned-emaonly.safetensors".into(),
        model_type: "checkpoints".into(),
        url: "https://huggingface.co/stable-diffusion-v1-5/stable-diffusion-v1-5/resolve/main/v1-5-pruned-emaonly.safetensors".into(),
        size_bytes: 4_265_380_512,
        license: "CreativeML Open RAIL-M".into(),
        description: "Stable Diffusion 1.5 — versatile, runs on 4GB+ VRAM.".into(),
    }]
}

/// Recommended upscale models with download URLs.
///
/// Used by the prep module to download models to ComfyUI instances.
pub fn recommended_upscale_models() -> Vec<RecommendedModel> {
    vec![
        RecommendedModel {
            filename: "RealESRGAN_x4plus.pth".into(),
            model_type: "upscale_models".into(),
            url: "https://github.com/xinntao/Real-ESRGAN/releases/download/v0.1.0/RealESRGAN_x4plus.pth".into(),
            size_bytes: 67_040_989,
            license: "BSD-3-Clause".into(),
            description: "General-purpose 4x upscaler for photos and AI-generated images. Commercial-friendly license.".into(),
        },
        RecommendedModel {
            filename: "RealESRGAN_x4plus_anime_6B.pth".into(),
            model_type: "upscale_models".into(),
            url: "https://github.com/xinntao/Real-ESRGAN/releases/download/v0.2.2.4/RealESRGAN_x4plus_anime_6B.pth".into(),
            size_bytes: 17_938_799,
            license: "BSD-3-Clause".into(),
            description: "Lightweight 4x upscaler optimized for anime and illustration content.".into(),
        },
    ]
}

/// A recommended model with download metadata.
#[derive(Debug, Clone)]
pub struct RecommendedModel {
    pub filename: String,
    pub model_type: String,
    pub url: String,
    pub size_bytes: u64,
    pub license: String,
    pub description: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_workflow_parses() {
        let wf: serde_json::Value =
            serde_json::from_str(UPSCALE_WORKFLOW_JSON).unwrap();
        let parsed = parser::parse_workflow(&wf).unwrap();

        assert_eq!(parsed.nodes.len(), 4);
        assert_eq!(parsed.inputs.len(), 2); // image + model placeholders
        assert_eq!(parsed.models.len(), 1);
        assert_eq!(parsed.outputs.len(), 1);
        assert!(parsed.diagram.contains("Upscale"));
    }

    #[test]
    fn image_upscale_skill_with_models() {
        let models = vec![
            "RealESRGAN_x4plus.pth".into(),
            "4x-UltraSharp.pth".into(),
        ];
        let skill = image_upscale(&models);

        assert_eq!(skill.name, "image.upscale");
        assert_eq!(skill.capability, Capability::Image);
        assert_eq!(skill.content_slots.len(), 1);
        assert!(skill.diagram.is_some());

        // Should prefer RealESRGAN as default
        let default = skill.parameter_schema.schema["properties"]["upscale_model"]["default"]
            .as_str()
            .unwrap();
        assert_eq!(default, "RealESRGAN_x4plus.pth");
    }

    #[test]
    fn image_upscale_skill_fallback_default() {
        let models = vec!["4x-UltraSharp.pth".into()];
        let skill = image_upscale(&models);

        // No RealESRGAN, should fall back to first model
        let default = skill.parameter_schema.schema["properties"]["upscale_model"]["default"]
            .as_str()
            .unwrap();
        assert_eq!(default, "4x-UltraSharp.pth");
    }

    #[test]
    fn recommended_models_are_valid() {
        let models = recommended_upscale_models();
        assert!(models.len() >= 2);

        let realesrgan = models.iter().find(|m| m.filename == "RealESRGAN_x4plus.pth").unwrap();
        assert_eq!(realesrgan.license, "BSD-3-Clause");
        assert!(realesrgan.url.starts_with("https://"));
        assert!(realesrgan.size_bytes > 0);
    }
}
