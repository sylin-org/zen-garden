//! Built-in skill definitions — declarative mapping-driven (ORCH-0018).
//!
//! Each built-in skill is a workflow template + mappings + metadata.
//! The mappings declare how user inputs map to workflow parameters.
//! The execution engine iterates mappings — zero skill-specific code.

use crate::domain::skill::{
    AutoKind, ContentSlot, ContentType, ModelRef, ParamOption, ParamType, SkillDefinition,
    SkillMapping,
};
use crate::domain::types::Capability;

use super::parser;

// ── Workflow Templates ────────────────────────────────────────

const UPSCALE_WORKFLOW_JSON: &str = include_str!("upscale_workflow.json");
const GENERATE_WORKFLOW_JSON: &str = include_str!("generate_workflow.json");
const IMG2IMG_WORKFLOW_JSON: &str = include_str!("img2img_workflow.json");

// ── image.upscale ─────────────────────────────────────────────

/// Build the `image.upscale` skill definition.
///
/// `available_models` comes from the ComfyUI instance's `/models/upscale_models`.
/// When empty, recommended models are used as defaults.
pub fn image_upscale(available_models: &[String]) -> SkillDefinition {
    let workflow: serde_json::Value =
        serde_json::from_str(UPSCALE_WORKFLOW_JSON).expect("embedded upscale workflow is valid JSON");

    let parsed = parser::parse_workflow(&workflow)
        .expect("embedded upscale workflow parses correctly");

    // Build named options: filename → friendly label
    let model_options: Vec<ParamOption> = if available_models.is_empty() {
        recommended_upscale_models()
            .iter()
            .map(|m| ParamOption::named(m.filename.as_str(), &m.description))
            .collect()
    } else {
        available_models
            .iter()
            .map(|m| {
                // Match known models to friendly labels
                let label = recommended_upscale_models()
                    .iter()
                    .find(|r| r.filename == *m)
                    .map(|r| r.description.clone())
                    .unwrap_or_else(|| m.clone());
                ParamOption::named(m.as_str(), label)
            })
            .collect()
    };

    let default_model = available_models
        .iter()
        .find(|m| m.contains("RealESRGAN_x4plus"))
        .or_else(|| available_models.first())
        .cloned()
        .unwrap_or_else(|| "RealESRGAN_x4plus.pth".into());

    SkillDefinition {
        name: "image.upscale".into(),
        display_name: "Upscale".into(),
        capability: Capability::Image,
        description: "Enhance image resolution using AI super-resolution".into(),
        provider_kind: crate::domain::types::OfferingKind::ComfyUi,
        vram_mb: 1024,
        content_slots: vec![ContentSlot {
            role: "source".into(),
            content_type: ContentType::Image,
            required: true,
        }],
        mappings: vec![
            SkillMapping::Content {
                role: "source".into(),
                content_type: ContentType::Image,
                placeholder: "PLACEHOLDER_IMAGE".into(),
            },
            SkillMapping::Param {
                field: "upscale_model".into(),
                node: "2".into(),
                input: "model_name".into(),
                label: "Zoom".into(),
                param_type: ParamType::Options { options: model_options },
                default: Some(serde_json::json!(default_model)),
            },
        ],
        diagram: Some(parsed.diagram),
        required_models: recommended_upscale_models()
            .into_iter()
            .map(|m| ModelRef {
                filename: m.filename,
                model_type: m.model_type,
                description: Some(m.description),
            })
            .collect(),
        workflow,
    }
}

// ── image.generate ────────────────────────────────────────────

/// Build the `image.generate` skill definition (text-to-image).
pub fn image_generate(available_checkpoints: &[String]) -> SkillDefinition {
    let workflow: serde_json::Value =
        serde_json::from_str(GENERATE_WORKFLOW_JSON).expect("embedded generate workflow is valid JSON");

    let parsed = parser::parse_workflow(&workflow)
        .expect("embedded generate workflow parses correctly");

    let (checkpoint_options, default_checkpoint) = checkpoint_options(available_checkpoints);

    SkillDefinition {
        name: "image.generate".into(),
        display_name: "Generate".into(),
        capability: Capability::Image,
        description: "Create an image from a text description".into(),
        provider_kind: crate::domain::types::OfferingKind::ComfyUi,
        vram_mb: 4096,
        content_slots: vec![ContentSlot {
            role: "prompt".into(),
            content_type: ContentType::Text,
            required: true,
        }],
        mappings: vec![
            SkillMapping::Content {
                role: "prompt".into(),
                content_type: ContentType::Text,
                placeholder: "PLACEHOLDER_PROMPT".into(),
            },
            SkillMapping::Param {
                field: "negative".into(),
                node: "3".into(),
                input: "text".into(),
                label: "Negative Prompt".into(),
                param_type: ParamType::Text,
                default: Some(serde_json::json!("blurry, watermark, low quality, deformed")),
            },
            SkillMapping::Param {
                field: "checkpoint".into(),
                node: "1".into(),
                input: "ckpt_name".into(),
                label: "Model".into(),
                param_type: ParamType::Options { options: checkpoint_options },
                default: Some(serde_json::json!(default_checkpoint)),
            },
            SkillMapping::Param {
                field: "width".into(),
                node: "4".into(),
                input: "width".into(),
                label: "Width".into(),
                param_type: ParamType::Options {
                    options: vec![
                        ParamOption::simple(512),
                        ParamOption::simple(768),
                        ParamOption::simple(1024),
                    ],
                },
                default: Some(serde_json::json!(512)),
            },
            SkillMapping::Param {
                field: "height".into(),
                node: "4".into(),
                input: "height".into(),
                label: "Height".into(),
                param_type: ParamType::Options {
                    options: vec![
                        ParamOption::simple(512),
                        ParamOption::simple(768),
                        ParamOption::simple(1024),
                    ],
                },
                default: Some(serde_json::json!(512)),
            },
            SkillMapping::Param {
                field: "steps".into(),
                node: "5".into(),
                input: "steps".into(),
                label: "Steps".into(),
                param_type: ParamType::Range { min: 1.0, max: 50.0, step: Some(1.0) },
                default: Some(serde_json::json!(20)),
            },
            SkillMapping::Param {
                field: "seed".into(),
                node: "5".into(),
                input: "seed".into(),
                label: "Seed".into(),
                param_type: ParamType::Auto { kind: AutoKind::RandomInt },
                default: None,
            },
        ],
        diagram: Some(parsed.diagram),
        required_models: recommended_checkpoint_models()
            .into_iter()
            .map(|m| ModelRef {
                filename: m.filename,
                model_type: m.model_type,
                description: Some(m.description),
            })
            .collect(),
        workflow,
    }
}

// ── image.img2img ─────────────────────────────────────────────

/// Build the `image.img2img` skill definition (image + prompt → transformed image).
pub fn image_img2img(available_checkpoints: &[String]) -> SkillDefinition {
    let workflow: serde_json::Value =
        serde_json::from_str(IMG2IMG_WORKFLOW_JSON).expect("embedded img2img workflow is valid JSON");

    let parsed = parser::parse_workflow(&workflow)
        .expect("embedded img2img workflow parses correctly");

    let (checkpoint_options, default_checkpoint) = checkpoint_options(available_checkpoints);

    SkillDefinition {
        name: "image.img2img".into(),
        display_name: "Transform".into(),
        capability: Capability::Image,
        description: "Transform an image guided by a text prompt".into(),
        provider_kind: crate::domain::types::OfferingKind::ComfyUi,
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
        mappings: vec![
            SkillMapping::Content {
                role: "source".into(),
                content_type: ContentType::Image,
                placeholder: "PLACEHOLDER_IMAGE".into(),
            },
            SkillMapping::Content {
                role: "prompt".into(),
                content_type: ContentType::Text,
                placeholder: "PLACEHOLDER_PROMPT".into(),
            },
            SkillMapping::Param {
                field: "negative".into(),
                node: "3".into(),
                input: "text".into(),
                label: "Negative Prompt".into(),
                param_type: ParamType::Text,
                default: Some(serde_json::json!("blurry, watermark, low quality")),
            },
            SkillMapping::Param {
                field: "checkpoint".into(),
                node: "1".into(),
                input: "ckpt_name".into(),
                label: "Model".into(),
                param_type: ParamType::Options { options: checkpoint_options },
                default: Some(serde_json::json!(default_checkpoint)),
            },
            SkillMapping::Param {
                field: "strength".into(),
                node: "6".into(),
                input: "denoise".into(),
                label: "Strength".into(),
                param_type: ParamType::Range { min: 0.0, max: 1.0, step: Some(0.05) },
                default: Some(serde_json::json!(0.7)),
            },
            SkillMapping::Param {
                field: "steps".into(),
                node: "6".into(),
                input: "steps".into(),
                label: "Steps".into(),
                param_type: ParamType::Range { min: 1.0, max: 50.0, step: Some(1.0) },
                default: Some(serde_json::json!(20)),
            },
            SkillMapping::Param {
                field: "seed".into(),
                node: "6".into(),
                input: "seed".into(),
                label: "Seed".into(),
                param_type: ParamType::Auto { kind: AutoKind::RandomInt },
                default: None,
            },
        ],
        diagram: Some(parsed.diagram),
        required_models: recommended_checkpoint_models()
            .into_iter()
            .map(|m| ModelRef {
                filename: m.filename,
                model_type: m.model_type,
                description: Some(m.description),
            })
            .collect(),
        workflow,
    }
}

// ── Helpers ───────────────────────────────────────────────────

/// Build checkpoint options + default from available or recommended models.
fn checkpoint_options(available: &[String]) -> (Vec<ParamOption>, String) {
    let effective: Vec<String> = if available.is_empty() {
        recommended_checkpoint_models()
            .iter()
            .map(|m| m.filename.clone())
            .collect()
    } else {
        available.to_vec()
    };

    let default = effective.first().cloned().unwrap_or_default();
    let options = effective.iter().map(|m| ParamOption::simple(m.as_str())).collect();
    (options, default)
}

// ── Recommended Models ────────────────────────────────────────

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

pub fn recommended_upscale_models() -> Vec<RecommendedModel> {
    vec![
        RecommendedModel {
            filename: "RealESRGAN_x4plus.pth".into(),
            model_type: "upscale_models".into(),
            url: "https://github.com/xinntao/Real-ESRGAN/releases/download/v0.1.0/RealESRGAN_x4plus.pth".into(),
            size_bytes: 67_040_989,
            license: "BSD-3-Clause".into(),
            description: "General-purpose 4x upscaler. Commercial-friendly.".into(),
        },
        RecommendedModel {
            filename: "RealESRGAN_x4plus_anime_6B.pth".into(),
            model_type: "upscale_models".into(),
            url: "https://github.com/xinntao/Real-ESRGAN/releases/download/v0.2.2.4/RealESRGAN_x4plus_anime_6B.pth".into(),
            size_bytes: 17_938_799,
            license: "BSD-3-Clause".into(),
            description: "Lightweight 4x upscaler for anime and illustration.".into(),
        },
    ]
}

#[derive(Debug, Clone)]
pub struct RecommendedModel {
    pub filename: String,
    pub model_type: String,
    pub url: String,
    pub size_bytes: u64,
    pub license: String,
    pub description: String,
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::skill::SkillMapping;

    #[test]
    fn embedded_workflow_parses() {
        let wf: serde_json::Value =
            serde_json::from_str(UPSCALE_WORKFLOW_JSON).unwrap();
        let parsed = parser::parse_workflow(&wf).unwrap();

        assert_eq!(parsed.nodes.len(), 4);
        assert_eq!(parsed.inputs.len(), 2);
        assert_eq!(parsed.models.len(), 1);
        assert_eq!(parsed.outputs.len(), 1);
        assert!(parsed.diagram.contains("Upscale"));
    }

    #[test]
    fn upscale_skill_with_models() {
        let models = vec![
            "RealESRGAN_x4plus.pth".into(),
            "4x-UltraSharp.pth".into(),
        ];
        let skill = image_upscale(&models);

        assert_eq!(skill.name, "image.upscale");
        assert_eq!(skill.capability, Capability::Image);
        assert_eq!(skill.content_slots.len(), 1);
        assert!(skill.diagram.is_some());

        // Default should be RealESRGAN
        let model_mapping = skill.mappings.iter().find(|m| matches!(m, SkillMapping::Param { field, .. } if field == "upscale_model"));
        assert!(model_mapping.is_some());
        if let Some(SkillMapping::Param { default: Some(d), .. }) = model_mapping {
            assert_eq!(d, &serde_json::json!("RealESRGAN_x4plus.pth"));
        }
    }

    #[test]
    fn upscale_skill_fallback_default() {
        let models = vec!["4x-UltraSharp.pth".into()];
        let skill = image_upscale(&models);

        let model_mapping = skill.mappings.iter().find(|m| matches!(m, SkillMapping::Param { field, .. } if field == "upscale_model"));
        if let Some(SkillMapping::Param { default: Some(d), .. }) = model_mapping {
            assert_eq!(d, &serde_json::json!("4x-UltraSharp.pth"));
        }
    }

    #[test]
    fn generate_skill_has_text_content() {
        let skill = image_generate(&[]);
        assert_eq!(skill.content_slots.len(), 1);
        assert_eq!(skill.content_slots[0].content_type, ContentType::Text);
        assert_eq!(skill.content_slots[0].role, "prompt");

        // Should have a content mapping for prompt
        let prompt_mapping = skill.mappings.iter().find(|m| matches!(m, SkillMapping::Content { role, .. } if role == "prompt"));
        assert!(prompt_mapping.is_some());
    }

    #[test]
    fn img2img_skill_has_both_content_types() {
        let skill = image_img2img(&[]);
        assert_eq!(skill.content_slots.len(), 2);

        let image_slot = skill.content_slots.iter().find(|s| s.content_type == ContentType::Image);
        let text_slot = skill.content_slots.iter().find(|s| s.content_type == ContentType::Text);
        assert!(image_slot.is_some());
        assert!(text_slot.is_some());
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
