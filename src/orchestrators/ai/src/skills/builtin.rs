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

    // Build the model enum for the parameter schema
    let model_enum: Vec<serde_json::Value> = available_models
        .iter()
        .map(|m| serde_json::Value::String(m.clone()))
        .collect();
    let default_model = available_models
        .iter()
        // Prefer RealESRGAN_x4plus (BSD licensed) if available
        .find(|m| m.contains("RealESRGAN_x4plus"))
        .or_else(|| available_models.first())
        .cloned()
        .unwrap_or_default();

    SkillDefinition {
        name: "image.upscale".into(),
        display_name: "Upscale".into(),
        capability: Capability::Image,
        description: "Enhance image resolution using AI super-resolution".into(),
        status: crate::domain::skill::SkillStatus::Initializing,
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
        required_models: available_models
            .iter()
            .map(|m| ModelRef {
                filename: m.clone(),
                model_type: "upscale_models".into(),
                description: None,
            })
            .collect(),
        implementation: workflow,
    }
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
