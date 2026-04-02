//! Built-in skill definitions — declarative mapping-driven (ORCH-0018).
//!
//! Each skill is pure data: mappings + workflow templates + metadata.
//! The execution engine iterates mappings — zero skill-specific code.

use std::collections::HashMap;

use crate::domain::skill::{
    AutoKind, ContentSlot, ContentType, ModelRef, ParamOption, ParamType, SkillDefinition,
    SkillMapping,
};
use crate::domain::types::Capability;

use super::parser;

// ── Workflow Templates ────────────────────────────────────────

const UPSCALE_2X_JSON: &str = include_str!("upscale_2x.json");
const UPSCALE_4X_JSON: &str = include_str!("upscale_4x.json");
const UPSCALE_8X_JSON: &str = include_str!("upscale_8x.json");
const UPSCALE_16X_JSON: &str = include_str!("upscale_16x.json");
const GENERATE_WORKFLOW_JSON: &str = include_str!("generate_workflow.json");
const IMG2IMG_WORKFLOW_JSON: &str = include_str!("img2img_workflow.json");
const INPAINT_WORKFLOW_JSON: &str = include_str!("inpaint_workflow.json");

fn load_workflow(json: &str) -> serde_json::Value {
    serde_json::from_str(json).expect("embedded workflow is valid JSON")
}

// ── image.upscale ─────────────────────────────────────────────

pub fn image_upscale(_available_models: &[String]) -> SkillDefinition {
    // Parse the 4x workflow for the Mermaid diagram
    let diagram_wf = load_workflow(UPSCALE_4X_JSON);
    let parsed = parser::parse_workflow(&diagram_wf)
        .expect("embedded upscale workflow parses correctly");

    let mut workflows = HashMap::new();
    workflows.insert("upscale_2x".into(), load_workflow(UPSCALE_2X_JSON));
    workflows.insert("upscale_4x".into(), load_workflow(UPSCALE_4X_JSON));
    workflows.insert("upscale_8x".into(), load_workflow(UPSCALE_8X_JSON));
    workflows.insert("upscale_16x".into(), load_workflow(UPSCALE_16X_JSON));

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
            overlay: None,
            default: None,
        }],
        mappings: vec![
            SkillMapping::Content {
                role: "source".into(),
                content_type: ContentType::Image,
                placeholder: "PLACEHOLDER_IMAGE".into(),
            },
            SkillMapping::Param {
                field: "workflow".into(),
                label: "Zoom".into(),
                node: None,
                input: None,
                placeholder: None,
                param_type: ParamType::Options {
                    options: vec![
                        ParamOption::named("upscale_2x", "2x"),
                        ParamOption::named("upscale_4x", "4x"),
                        ParamOption::named("upscale_8x", "8x"),
                        ParamOption::named("upscale_16x", "16x"),
                    ],
                },
                default: Some(serde_json::json!("upscale_4x")),
            },
            SkillMapping::Param {
                field: "upscale_model".into(),
                label: "Style".into(),
                node: None,
                input: None,
                placeholder: Some("PLACEHOLDER_MODEL".into()),
                param_type: ParamType::Options {
                    options: vec![
                        ParamOption::named("RealESRGAN_x4plus.pth", "Realistic"),
                        ParamOption::named("RealESRGAN_x4plus_anime_6B.pth", "Anime"),
                    ],
                },
                default: Some(serde_json::json!("RealESRGAN_x4plus.pth")),
            },
        ],
        diagram: Some(parsed.diagram),
        preview_url: None,
        required_models: recommended_upscale_models()
            .into_iter()
            .map(|m| ModelRef {
                filename: m.filename,
                model_type: m.model_type,
                url: Some(m.url),
                size_bytes: Some(m.size_bytes),
                sha256: None,
                license: Some(m.license),
                description: Some(m.description),
            })
            .collect(),
        default_workflow: "upscale_4x".into(),
        workflows,
    }
}

// ── image.generate ────────────────────────────────────────────

pub fn image_generate(available_checkpoints: &[String]) -> SkillDefinition {
    let workflow = load_workflow(GENERATE_WORKFLOW_JSON);
    let parsed = parser::parse_workflow(&workflow)
        .expect("embedded generate workflow parses correctly");

    let (checkpoint_options, default_checkpoint) = checkpoint_options(available_checkpoints);

    let mut workflows = HashMap::new();
    workflows.insert("generate".into(), workflow);

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
            overlay: None,
            default: None,
        }],
        mappings: vec![
            SkillMapping::Content {
                role: "prompt".into(),
                content_type: ContentType::Text,
                placeholder: "PLACEHOLDER_PROMPT".into(),
            },
            SkillMapping::Param {
                field: "negative".into(),
                label: "Negative Prompt".into(),
                node: None,
                input: None,
                placeholder: Some("PLACEHOLDER_NEGATIVE".into()),
                param_type: ParamType::Text,
                default: Some(serde_json::json!("blurry, watermark, low quality, deformed")),
            },
            SkillMapping::Param {
                field: "checkpoint".into(),
                label: "Model".into(),
                node: None,
                input: None,
                placeholder: Some("PLACEHOLDER_CHECKPOINT".into()),
                param_type: ParamType::Options { options: checkpoint_options },
                default: Some(serde_json::json!(default_checkpoint)),
            },
            SkillMapping::Param {
                field: "width".into(),
                label: "Width".into(),
                node: Some("4".into()),
                input: Some("width".into()),
                placeholder: None,
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
                label: "Height".into(),
                node: Some("4".into()),
                input: Some("height".into()),
                placeholder: None,
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
                label: "Steps".into(),
                node: Some("5".into()),
                input: Some("steps".into()),
                placeholder: None,
                param_type: ParamType::Range { min: 1.0, max: 50.0, step: Some(1.0) },
                default: Some(serde_json::json!(20)),
            },
            SkillMapping::Param {
                field: "seed".into(),
                label: "Seed".into(),
                node: Some("5".into()),
                input: Some("seed".into()),
                placeholder: None,
                param_type: ParamType::Auto { kind: AutoKind::RandomInt },
                default: None,
            },
        ],
        diagram: Some(parsed.diagram),
        preview_url: None,
        required_models: recommended_checkpoint_models()
            .into_iter()
            .map(|m| ModelRef {
                filename: m.filename,
                model_type: m.model_type,
                url: Some(m.url),
                size_bytes: Some(m.size_bytes),
                sha256: None,
                license: Some(m.license),
                description: Some(m.description),
            })
            .collect(),
        default_workflow: "generate".into(),
        workflows,
    }
}

// ── image.img2img ─────────────────────────────────────────────

pub fn image_img2img(available_checkpoints: &[String]) -> SkillDefinition {
    let workflow = load_workflow(IMG2IMG_WORKFLOW_JSON);
    let parsed = parser::parse_workflow(&workflow)
        .expect("embedded img2img workflow parses correctly");

    let (checkpoint_options, default_checkpoint) = checkpoint_options(available_checkpoints);

    let mut workflows = HashMap::new();
    workflows.insert("img2img".into(), workflow);

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
                overlay: None,
                default: None,
            },
            ContentSlot {
                role: "prompt".into(),
                content_type: ContentType::Text,
                required: true,
                overlay: None,
                default: None,
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
                label: "Negative Prompt".into(),
                node: None,
                input: None,
                placeholder: Some("PLACEHOLDER_NEGATIVE".into()),
                param_type: ParamType::Text,
                default: Some(serde_json::json!("blurry, watermark, low quality")),
            },
            SkillMapping::Param {
                field: "checkpoint".into(),
                label: "Model".into(),
                node: None,
                input: None,
                placeholder: Some("PLACEHOLDER_CHECKPOINT".into()),
                param_type: ParamType::Options { options: checkpoint_options },
                default: Some(serde_json::json!(default_checkpoint)),
            },
            SkillMapping::Param {
                field: "strength".into(),
                label: "Strength".into(),
                node: Some("6".into()),
                input: Some("denoise".into()),
                placeholder: None,
                param_type: ParamType::Range { min: 0.0, max: 1.0, step: Some(0.05) },
                default: Some(serde_json::json!(0.7)),
            },
            SkillMapping::Param {
                field: "steps".into(),
                label: "Steps".into(),
                node: Some("6".into()),
                input: Some("steps".into()),
                placeholder: None,
                param_type: ParamType::Range { min: 1.0, max: 50.0, step: Some(1.0) },
                default: Some(serde_json::json!(20)),
            },
            SkillMapping::Param {
                field: "seed".into(),
                label: "Seed".into(),
                node: Some("6".into()),
                input: Some("seed".into()),
                placeholder: None,
                param_type: ParamType::Auto { kind: AutoKind::RandomInt },
                default: None,
            },
        ],
        diagram: Some(parsed.diagram),
        preview_url: None,
        required_models: recommended_checkpoint_models()
            .into_iter()
            .map(|m| ModelRef {
                filename: m.filename,
                model_type: m.model_type,
                url: Some(m.url),
                size_bytes: Some(m.size_bytes),
                sha256: None,
                license: Some(m.license),
                description: Some(m.description),
            })
            .collect(),
        default_workflow: "img2img".into(),
        workflows,
    }
}

// ── image.inpaint ─────────────────────────────────────────────

pub fn image_inpaint(available_checkpoints: &[String]) -> SkillDefinition {
    let workflow = load_workflow(INPAINT_WORKFLOW_JSON);
    let parsed = parser::parse_workflow(&workflow)
        .expect("embedded inpaint workflow parses correctly");

    let (checkpoint_options, default_checkpoint) = inpaint_checkpoint_options(available_checkpoints);

    let mut workflows = HashMap::new();
    workflows.insert("inpaint".into(), workflow);

    SkillDefinition {
        name: "image.inpaint".into(),
        display_name: "Inpaint".into(),
        capability: Capability::Image,
        description: "Edit specific regions of an image using a mask and prompt".into(),
        provider_kind: crate::domain::types::OfferingKind::ComfyUi,
        vram_mb: 4096,
        content_slots: vec![
            ContentSlot {
                role: "source".into(),
                content_type: ContentType::Image,
                required: true,
                overlay: None,
                default: None,
            },
            ContentSlot {
                role: "mask".into(),
                content_type: ContentType::Image,
                required: true,
                overlay: Some("source".into()),
                default: None,
            },
            ContentSlot {
                role: "prompt".into(),
                content_type: ContentType::Text,
                required: true,
                overlay: None,
                default: None,
            },
        ],
        mappings: vec![
            SkillMapping::Content {
                role: "source".into(),
                content_type: ContentType::Image,
                placeholder: "PLACEHOLDER_IMAGE".into(),
            },
            SkillMapping::Content {
                role: "mask".into(),
                content_type: ContentType::Image,
                placeholder: "PLACEHOLDER_MASK".into(),
            },
            SkillMapping::Content {
                role: "prompt".into(),
                content_type: ContentType::Text,
                placeholder: "PLACEHOLDER_PROMPT".into(),
            },
            SkillMapping::Param {
                field: "negative".into(),
                label: "Negative Prompt".into(),
                node: None,
                input: None,
                placeholder: Some("PLACEHOLDER_NEGATIVE".into()),
                param_type: ParamType::Text,
                default: Some(serde_json::json!("blurry, watermark, low quality, deformed")),
            },
            SkillMapping::Param {
                field: "checkpoint".into(),
                label: "Model".into(),
                node: None,
                input: None,
                placeholder: Some("PLACEHOLDER_CHECKPOINT".into()),
                param_type: ParamType::Options { options: checkpoint_options },
                default: Some(serde_json::json!(default_checkpoint)),
            },
            SkillMapping::Param {
                field: "strength".into(),
                label: "Strength".into(),
                node: Some("7".into()),
                input: Some("denoise".into()),
                placeholder: None,
                param_type: ParamType::Range { min: 0.0, max: 1.0, step: Some(0.05) },
                default: Some(serde_json::json!(1.0)),
            },
            SkillMapping::Param {
                field: "steps".into(),
                label: "Steps".into(),
                node: Some("7".into()),
                input: Some("steps".into()),
                placeholder: None,
                param_type: ParamType::Range { min: 1.0, max: 50.0, step: Some(1.0) },
                default: Some(serde_json::json!(20)),
            },
            SkillMapping::Param {
                field: "seed".into(),
                label: "Seed".into(),
                node: Some("7".into()),
                input: Some("seed".into()),
                placeholder: None,
                param_type: ParamType::Auto { kind: AutoKind::RandomInt },
                default: None,
            },
        ],
        diagram: Some(parsed.diagram),
        preview_url: None,
        required_models: recommended_inpaint_models()
            .into_iter()
            .map(|m| ModelRef {
                filename: m.filename,
                model_type: m.model_type,
                url: Some(m.url),
                size_bytes: Some(m.size_bytes),
                sha256: None,
                license: Some(m.license),
                description: Some(m.description),
            })
            .collect(),
        default_workflow: "inpaint".into(),
        workflows,
    }
}

// ── Helpers ───────────────────────────────────────────────────

fn inpaint_checkpoint_options(available: &[String]) -> (Vec<ParamOption>, String) {
    let effective: Vec<String> = if available.is_empty() {
        recommended_inpaint_models()
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

pub fn recommended_inpaint_models() -> Vec<RecommendedModel> {
    vec![RecommendedModel {
        filename: "sd-v1-5-inpainting.ckpt".into(),
        model_type: "checkpoints".into(),
        url: "https://huggingface.co/runwayml/stable-diffusion-inpainting/resolve/main/sd-v1-5-inpainting.ckpt".into(),
        size_bytes: 4_265_380_512,
        license: "CreativeML Open RAIL-M".into(),
        description: "SD 1.5 inpainting — dedicated inpainting checkpoint.".into(),
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

    #[test]
    fn upscale_has_four_workflows() {
        let skill = image_upscale(&[]);
        assert_eq!(skill.workflows.len(), 4);
        assert!(skill.workflows.contains_key("upscale_2x"));
        assert!(skill.workflows.contains_key("upscale_4x"));
        assert!(skill.workflows.contains_key("upscale_8x"));
        assert!(skill.workflows.contains_key("upscale_16x"));
        assert_eq!(skill.default_workflow, "upscale_4x");
    }

    #[test]
    fn upscale_has_zoom_and_style() {
        let skill = image_upscale(&[]);
        let zoom = skill.mappings.iter().find(|m| matches!(m, SkillMapping::Param { field, .. } if field == "workflow"));
        assert!(zoom.is_some(), "zoom (workflow) param exists");

        let style = skill.mappings.iter().find(|m| matches!(m, SkillMapping::Param { field, .. } if field == "upscale_model"));
        assert!(style.is_some(), "style param exists");
    }

    #[test]
    fn generate_has_text_content() {
        let skill = image_generate(&[]);
        assert_eq!(skill.content_slots.len(), 1);
        assert_eq!(skill.content_slots[0].content_type, ContentType::Text);
        assert_eq!(skill.default_workflow, "generate");
        assert_eq!(skill.workflows.len(), 1);
    }

    #[test]
    fn img2img_has_both_content_types() {
        let skill = image_img2img(&[]);
        assert_eq!(skill.content_slots.len(), 2);
        assert_eq!(skill.default_workflow, "img2img");
    }

    #[test]
    fn recommended_models_are_valid() {
        let models = recommended_upscale_models();
        assert!(models.len() >= 2);
        assert!(models.iter().any(|m| m.filename == "RealESRGAN_x4plus.pth"));
    }
}
