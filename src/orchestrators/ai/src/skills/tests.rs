//! End-to-end tests for the skill pipeline.
//!
//! Validates: workflow JSON → parser → skill definition → template filling
//! → expected ComfyUI prompt format → output extraction.

#[cfg(test)]
mod tests {
    use crate::domain::skill::*;
    use crate::domain::types::Capability;
    use crate::skills::{builtin, parser};

    // ================================================================
    // Pipeline: workflow JSON → parsed → skill → ready for execution
    // ================================================================

    #[test]
    fn full_pipeline_upscale_workflow_to_skill() {
        // 1. Load the embedded workflow
        let wf_json: serde_json::Value =
            serde_json::from_str(include_str!("upscale_workflow.json")).unwrap();

        // 2. Parse it
        let parsed = parser::parse_workflow(&wf_json).unwrap();

        // Verify structure
        assert_eq!(parsed.nodes.len(), 4, "upscale workflow has 4 nodes");
        assert_eq!(parsed.outputs.len(), 1, "one output node");
        assert_eq!(parsed.models.len(), 1, "one model loader");
        assert_eq!(parsed.models[0].model_type, "upscale_models");
        assert!(parsed.models[0].is_placeholder);

        // Verify we found the image input
        let image_inputs: Vec<_> = parsed
            .inputs
            .iter()
            .filter(|i| i.kind == parser::InputKind::Image)
            .collect();
        assert_eq!(image_inputs.len(), 1);
        assert_eq!(image_inputs[0].placeholder, "PLACEHOLDER_IMAGE");

        // 3. Build skill from parsed + available models
        let available = vec!["RealESRGAN_x4plus.pth".into(), "4x-UltraSharp.pth".into()];
        let skill = builtin::image_upscale(&available);

        // Verify skill metadata
        assert_eq!(skill.name, "image.upscale");
        assert_eq!(skill.capability, Capability::Image);
        assert_eq!(skill.content_slots.len(), 1);
        assert_eq!(skill.content_slots[0].content_type, ContentType::Image);
        assert!(skill.content_slots[0].required);

        // Verify Mermaid diagram was generated from the workflow
        let diagram = skill.diagram.as_ref().unwrap();
        assert!(diagram.contains("graph LR"), "Mermaid starts with graph LR");
        assert!(diagram.contains("Load Image"), "diagram has Load Image node");
        assert!(diagram.contains("Upscale"), "diagram has Upscale node");
        assert!(diagram.contains("Save Image"), "diagram has Save Image node");
        assert!(diagram.contains("-->"), "diagram has edges");

        // Verify parameter schema has the model dropdown
        let schema = &skill.parameter_schema.schema;
        let model_prop = &schema["properties"]["upscale_model"];
        assert_eq!(model_prop["type"], "string");
        let enum_vals = model_prop["enum"].as_array().unwrap();
        assert_eq!(enum_vals.len(), 2);
        // Should prefer RealESRGAN as default (BSD licensed)
        assert_eq!(model_prop["default"], "RealESRGAN_x4plus.pth");

        // 4. Verify the implementation is a valid workflow template
        let impl_wf = &skill.implementation;
        assert!(impl_wf.is_object());
        // All 4 nodes present
        assert!(impl_wf.get("1").is_some(), "node 1 (LoadImage)");
        assert!(impl_wf.get("2").is_some(), "node 2 (UpscaleModelLoader)");
        assert!(impl_wf.get("3").is_some(), "node 3 (ImageUpscaleWithModel)");
        assert!(impl_wf.get("4").is_some(), "node 4 (SaveImage)");
    }

    // ================================================================
    // Parser: edge cases and complex workflows
    // ================================================================

    #[test]
    fn parser_handles_workflow_with_no_placeholders() {
        let wf = serde_json::json!({
            "1": {
                "class_type": "LoadImage",
                "inputs": { "image": "fixed-image.png" }
            },
            "2": {
                "class_type": "UpscaleModelLoader",
                "inputs": { "model_name": "4x-UltraSharp.pth" }
            },
            "3": {
                "class_type": "ImageUpscaleWithModel",
                "inputs": { "upscale_model": ["2", 0], "image": ["1", 0] }
            },
            "4": {
                "class_type": "SaveImage",
                "inputs": { "images": ["3", 0], "filename_prefix": "out" }
            }
        });

        let parsed = parser::parse_workflow(&wf).unwrap();
        // No placeholders → no user inputs detected
        assert!(parsed.inputs.is_empty());
        // But still detects the model (fixed, not placeholder)
        assert_eq!(parsed.models.len(), 1);
        assert!(!parsed.models[0].is_placeholder);
        assert_eq!(parsed.models[0].model_name, "4x-UltraSharp.pth");
    }

    #[test]
    fn parser_handles_multiple_model_types() {
        let wf = serde_json::json!({
            "1": {
                "class_type": "CheckpointLoaderSimple",
                "inputs": { "ckpt_name": "sd_xl_base_1.0.safetensors" }
            },
            "2": {
                "class_type": "LoraLoader",
                "inputs": {
                    "lora_name": "add_detail.safetensors",
                    "model": ["1", 0],
                    "clip": ["1", 1]
                }
            },
            "3": {
                "class_type": "UpscaleModelLoader",
                "inputs": { "model_name": "4x-UltraSharp.pth" }
            },
            "4": {
                "class_type": "VAELoader",
                "inputs": { "vae_name": "sdxl_vae.safetensors" }
            },
            "5": {
                "class_type": "SaveImage",
                "inputs": { "images": ["3", 0], "filename_prefix": "out" }
            }
        });

        let parsed = parser::parse_workflow(&wf).unwrap();
        assert_eq!(parsed.models.len(), 4);

        let types: Vec<&str> = parsed.models.iter().map(|m| m.model_type.as_str()).collect();
        assert!(types.contains(&"checkpoints"));
        assert!(types.contains(&"loras"));
        assert!(types.contains(&"upscale_models"));
        assert!(types.contains(&"vae"));
    }

    #[test]
    fn parser_handles_text_placeholders() {
        let wf = serde_json::json!({
            "1": {
                "class_type": "CLIPTextEncode",
                "inputs": { "text": "PLACEHOLDER_PROMPT", "clip": ["2", 1] }
            },
            "2": {
                "class_type": "CheckpointLoaderSimple",
                "inputs": { "ckpt_name": "sd15.safetensors" }
            },
            "3": {
                "class_type": "CLIPTextEncode",
                "inputs": { "text": "PLACEHOLDER_NEGATIVE", "clip": ["2", 1] }
            },
            "4": {
                "class_type": "SaveImage",
                "inputs": { "images": ["1", 0], "filename_prefix": "out" }
            }
        });

        let parsed = parser::parse_workflow(&wf).unwrap();

        let text_inputs: Vec<_> = parsed
            .inputs
            .iter()
            .filter(|i| i.kind == parser::InputKind::Text)
            .collect();
        assert_eq!(text_inputs.len(), 2);

        let placeholders: Vec<&str> = text_inputs.iter().map(|i| i.placeholder.as_str()).collect();
        assert!(placeholders.contains(&"PLACEHOLDER_PROMPT"));
        assert!(placeholders.contains(&"PLACEHOLDER_NEGATIVE"));
    }

    #[test]
    fn parser_single_node_workflow() {
        let wf = serde_json::json!({
            "1": {
                "class_type": "SaveImage",
                "inputs": { "images": "PLACEHOLDER_IMAGE", "filename_prefix": "out" }
            }
        });

        let parsed = parser::parse_workflow(&wf).unwrap();
        assert_eq!(parsed.nodes.len(), 1);
        assert_eq!(parsed.outputs.len(), 1);
    }

    // ================================================================
    // Mermaid diagram validation
    // ================================================================

    #[test]
    fn mermaid_diagram_has_all_edges_for_upscale() {
        let wf: serde_json::Value =
            serde_json::from_str(include_str!("upscale_workflow.json")).unwrap();
        let parsed = parser::parse_workflow(&wf).unwrap();

        // 3 edges: 1→3 (image), 2→3 (model), 3→4 (output)
        let edge_count = parsed.diagram.matches("-->").count();
        assert_eq!(edge_count, 3, "upscale workflow has 3 edges");
    }

    #[test]
    fn mermaid_diagram_labels_are_readable() {
        let wf: serde_json::Value =
            serde_json::from_str(include_str!("upscale_workflow.json")).unwrap();
        let parsed = parser::parse_workflow(&wf).unwrap();

        // Labels should be human-readable, not raw class_type
        assert!(parsed.diagram.contains("Load Image"));
        assert!(parsed.diagram.contains("Load Upscale Model"));
        assert!(parsed.diagram.contains("Upscale"));
        assert!(parsed.diagram.contains("Save Image"));

        // Should NOT contain raw class types
        assert!(!parsed.diagram.contains("LoadImage]"));
        assert!(!parsed.diagram.contains("UpscaleModelLoader]"));
        assert!(!parsed.diagram.contains("ImageUpscaleWithModel]"));
    }

    // ================================================================
    // Skill definition validation
    // ================================================================

    #[test]
    fn skill_with_no_models_has_empty_enum() {
        // Edge case: builtin::image_upscale called with empty list
        // (used internally by workflow() to get the template)
        let skill = builtin::image_upscale(&[]);
        assert!(skill.required_models.is_empty());

        let enum_vals = skill.parameter_schema.schema["properties"]["upscale_model"]["enum"]
            .as_array()
            .unwrap();
        assert!(enum_vals.is_empty());
    }

    #[test]
    fn skill_prefers_realesrgan_as_default() {
        let models = vec![
            "4x-UltraSharp.pth".into(),
            "RealESRGAN_x4plus.pth".into(),
            "4x-AnimeSharp.pth".into(),
        ];
        let skill = builtin::image_upscale(&models);

        let default = skill.parameter_schema.schema["properties"]["upscale_model"]["default"]
            .as_str()
            .unwrap();
        assert_eq!(default, "RealESRGAN_x4plus.pth");
    }

    #[test]
    fn skill_falls_back_to_first_model_when_no_realesrgan() {
        let models = vec!["SomeCustom_4x.pth".into(), "4x-AnimeSharp.pth".into()];
        let skill = builtin::image_upscale(&models);

        let default = skill.parameter_schema.schema["properties"]["upscale_model"]["default"]
            .as_str()
            .unwrap();
        assert_eq!(default, "SomeCustom_4x.pth");
    }

    #[test]
    fn skill_presentation_round_trip() {
        let models = vec!["RealESRGAN_x4plus.pth".into()];
        let skill = builtin::image_upscale(&models);
        let pres = SkillPresentation::from_definition(&skill);

        // Presentation should serialize cleanly
        let json = serde_json::to_value(&pres).unwrap();
        assert!(json.get("schema").is_some());
        assert!(json.get("ui_schema").is_some());
        assert!(json.get("content").is_some());
        assert!(json.get("diagram").is_some());

        // Content slots preserved
        assert_eq!(json["content"][0]["role"], "source");
        assert_eq!(json["content"][0]["content_type"], "image");
        assert_eq!(json["content"][0]["required"], true);
    }

    // ================================================================
    // WorkflowRequest deserialization edge cases
    // ================================================================

    #[test]
    fn workflow_request_with_inline_image() {
        let json = serde_json::json!({
            "skill": "image.upscale",
            "content": [{
                "type": "image",
                "role": "source",
                "data": "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="
            }],
            "parameters": { "upscale_model": "RealESRGAN_x4plus.pth" }
        });

        let req: WorkflowRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.skill, "image.upscale");
        assert_eq!(req.content[0].content_type, ContentType::Image);
        assert_eq!(req.content[0].role.as_deref(), Some("source"));
        assert!(req.content[0].data.is_some());
        assert!(req.content[0].url.is_none());
    }

    #[test]
    fn workflow_request_with_url_image() {
        let json = serde_json::json!({
            "skill": "image.upscale",
            "content": [{
                "type": "image",
                "url": "https://example.com/photo.png"
            }]
        });

        let req: WorkflowRequest = serde_json::from_value(json).unwrap();
        assert!(req.content[0].url.is_some());
        assert!(req.content[0].data.is_none());
        assert!(req.parameters.is_null());
    }

    #[test]
    fn workflow_request_with_data_uri() {
        let json = serde_json::json!({
            "skill": "image.upscale",
            "content": [{
                "type": "image",
                "data": "data:image/png;base64,iVBORw0KGgo="
            }]
        });

        let req: WorkflowRequest = serde_json::from_value(json).unwrap();
        let data = req.content[0].data.as_ref().unwrap();
        assert!(data.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn workflow_request_empty_content() {
        let json = serde_json::json!({
            "skill": "image.remove_bg"
        });

        let req: WorkflowRequest = serde_json::from_value(json).unwrap();
        assert!(req.content.is_empty());
    }

    // ================================================================
    // WorkflowJob serialization
    // ================================================================

    #[test]
    fn workflow_job_queued_serialization() {
        let job = WorkflowJob {
            id: "prompt-abc123".into(),
            skill: "image.upscale".into(),
            status: WorkflowJobStatus::Queued,
            progress: None,
            content: None,
            error: None,
            usage: None,
        };

        let json = serde_json::to_value(&job).unwrap();
        assert_eq!(json["status"], "queued");
        assert!(json.get("progress").is_none());
        assert!(json.get("content").is_none());
        assert!(json.get("error").is_none());
        assert!(json.get("usage").is_none());
    }

    #[test]
    fn workflow_job_running_with_progress() {
        let job = WorkflowJob {
            id: "prompt-abc123".into(),
            skill: "image.upscale".into(),
            status: WorkflowJobStatus::Running,
            progress: Some(0.65),
            content: None,
            error: None,
            usage: None,
        };

        let json = serde_json::to_value(&job).unwrap();
        assert_eq!(json["status"], "running");
        assert_eq!(json["progress"], 0.65);
    }

    #[test]
    fn workflow_job_completed_has_content_url() {
        let job = WorkflowJob {
            id: "prompt-abc123".into(),
            skill: "image.upscale".into(),
            status: WorkflowJobStatus::Completed,
            progress: Some(1.0),
            content: Some(vec![ContentBlock {
                content_type: ContentType::Image,
                role: None,
                data: None,
                url: Some("http://stone:8188/view?filename=zen-upscale_00001_.png&type=output&subfolder=".into()),
                format: Some("png".into()),
            }]),
            error: None,
            usage: Some(WorkflowUsage { duration_ms: 3200 }),
        };

        let json = serde_json::to_value(&job).unwrap();
        assert_eq!(json["status"], "completed");
        assert_eq!(json["content"][0]["type"], "image");
        assert!(json["content"][0]["url"].as_str().unwrap().contains("/view?filename="));
        assert_eq!(json["usage"]["duration_ms"], 3200);
    }

    // ================================================================
    // Recommended models
    // ================================================================

    #[test]
    fn recommended_models_have_valid_urls() {
        for model in builtin::recommended_upscale_models() {
            assert!(model.url.starts_with("https://"), "{} url not https", model.filename);
            assert!(model.size_bytes > 0, "{} has zero size", model.filename);
            assert!(!model.license.is_empty(), "{} has empty license", model.filename);
            assert!(!model.description.is_empty(), "{} has empty description", model.filename);
            assert_eq!(model.model_type, "upscale_models");
        }
    }

    #[test]
    fn recommended_models_include_bsd_option() {
        let models = builtin::recommended_upscale_models();
        let bsd = models.iter().find(|m| m.license == "BSD-3-Clause");
        assert!(bsd.is_some(), "must include at least one BSD-licensed model");
    }

    // ================================================================
    // SkillRegistry integration
    // ================================================================

    #[test]
    fn registry_accepts_builtin_skill() {
        let mut reg = SkillRegistry::new();
        let skill = builtin::image_upscale(&["RealESRGAN_x4plus.pth".into()]);
        reg.register(skill);

        assert_eq!(reg.len(), 1);
        let retrieved = reg.get("image.upscale").unwrap();
        assert_eq!(retrieved.capability, Capability::Image);
        assert_eq!(retrieved.content_slots.len(), 1);
        assert!(retrieved.diagram.is_some());
    }

    #[test]
    fn registry_updates_skill_when_models_change() {
        let mut reg = SkillRegistry::new();

        // First registration with one model
        reg.register(builtin::image_upscale(&["4x-UltraSharp.pth".into()]));
        let v1 = reg.get("image.upscale").unwrap();
        assert_eq!(v1.required_models.len(), 1);

        // Re-register with two models (simulating discovery finding more)
        reg.register(builtin::image_upscale(&[
            "4x-UltraSharp.pth".into(),
            "RealESRGAN_x4plus.pth".into(),
        ]));
        let v2 = reg.get("image.upscale").unwrap();
        assert_eq!(v2.required_models.len(), 2);

        // Still just one skill
        assert_eq!(reg.len(), 1);
    }
}
