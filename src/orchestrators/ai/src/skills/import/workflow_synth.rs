//! Synthesize a ComfyUI API-format workflow from generation parameters.
//!
//! When we have generation params (from CivitAI meta or pasted generation text)
//! but no actual workflow JSON, we build a standard txt2img pipeline.

use super::gen_data_parse::GenerationParams;

/// Build a standard txt2img ComfyUI API workflow from generation parameters.
///
/// Node graph:
/// 1: CheckpointLoaderSimple → 2: CLIPTextEncode (positive) → 4: KSampler
///                            → 3: CLIPTextEncode (negative) → 4
///                                                  5: EmptyLatentImage → 4
///                            → 4 → 6: VAEDecode → 7: SaveImage
pub fn synthesize_txt2img(params: &GenerationParams) -> serde_json::Value {
    let checkpoint = params.model.as_deref().unwrap_or("PLACEHOLDER_CHECKPOINT");
    let prompt = if params.prompt.is_empty() { "PLACEHOLDER_PROMPT" } else { &params.prompt };
    let negative = if params.negative_prompt.is_empty() { "PLACEHOLDER_NEGATIVE" } else { &params.negative_prompt };
    let steps = params.steps.unwrap_or(20);
    let cfg = params.cfg_scale.unwrap_or(7.0);
    let sampler = map_sampler(params.sampler.as_deref().unwrap_or("euler"));
    let scheduler = map_scheduler(params.sampler.as_deref().unwrap_or("euler"));
    let seed = params.seed.unwrap_or(0);
    let width = params.width.unwrap_or(512);
    let height = params.height.unwrap_or(512);

    serde_json::json!({
        "1": {
            "class_type": "CheckpointLoaderSimple",
            "inputs": { "ckpt_name": checkpoint }
        },
        "2": {
            "class_type": "CLIPTextEncode",
            "inputs": { "text": prompt, "clip": ["1", 1] }
        },
        "3": {
            "class_type": "CLIPTextEncode",
            "inputs": { "text": negative, "clip": ["1", 1] }
        },
        "4": {
            "class_type": "KSampler",
            "inputs": {
                "model": ["1", 0],
                "positive": ["2", 0],
                "negative": ["3", 0],
                "latent_image": ["5", 0],
                "seed": seed,
                "steps": steps,
                "cfg": cfg,
                "sampler_name": sampler,
                "scheduler": scheduler,
                "denoise": 1.0
            }
        },
        "5": {
            "class_type": "EmptyLatentImage",
            "inputs": {
                "width": width,
                "height": height,
                "batch_size": 1
            }
        },
        "6": {
            "class_type": "VAEDecode",
            "inputs": { "samples": ["4", 0], "vae": ["1", 2] }
        },
        "7": {
            "class_type": "SaveImage",
            "inputs": { "images": ["6", 0], "filename_prefix": "zen-generate" }
        }
    })
}

/// Build a txt2img workflow with LoRA support.
///
/// Inserts a LoraLoader between the checkpoint and the CLIP encoders.
pub fn synthesize_txt2img_with_lora(
    params: &GenerationParams,
    lora_filename: &str,
    lora_weight: f64,
) -> serde_json::Value {
    let checkpoint = params.model.as_deref().unwrap_or("PLACEHOLDER_CHECKPOINT");
    let prompt = if params.prompt.is_empty() { "PLACEHOLDER_PROMPT" } else { &params.prompt };
    let negative = if params.negative_prompt.is_empty() { "PLACEHOLDER_NEGATIVE" } else { &params.negative_prompt };
    let steps = params.steps.unwrap_or(20);
    let cfg = params.cfg_scale.unwrap_or(7.0);
    let sampler = map_sampler(params.sampler.as_deref().unwrap_or("euler"));
    let scheduler = map_scheduler(params.sampler.as_deref().unwrap_or("euler"));
    let seed = params.seed.unwrap_or(0);
    let width = params.width.unwrap_or(512);
    let height = params.height.unwrap_or(512);

    serde_json::json!({
        "1": {
            "class_type": "CheckpointLoaderSimple",
            "inputs": { "ckpt_name": checkpoint }
        },
        "8": {
            "class_type": "LoraLoader",
            "inputs": {
                "model": ["1", 0],
                "clip": ["1", 1],
                "lora_name": lora_filename,
                "strength_model": lora_weight,
                "strength_clip": lora_weight
            }
        },
        "2": {
            "class_type": "CLIPTextEncode",
            "inputs": { "text": prompt, "clip": ["8", 1] }
        },
        "3": {
            "class_type": "CLIPTextEncode",
            "inputs": { "text": negative, "clip": ["8", 1] }
        },
        "4": {
            "class_type": "KSampler",
            "inputs": {
                "model": ["8", 0],
                "positive": ["2", 0],
                "negative": ["3", 0],
                "latent_image": ["5", 0],
                "seed": seed,
                "steps": steps,
                "cfg": cfg,
                "sampler_name": sampler,
                "scheduler": scheduler,
                "denoise": 1.0
            }
        },
        "5": {
            "class_type": "EmptyLatentImage",
            "inputs": {
                "width": width,
                "height": height,
                "batch_size": 1
            }
        },
        "6": {
            "class_type": "VAEDecode",
            "inputs": { "samples": ["4", 0], "vae": ["1", 2] }
        },
        "7": {
            "class_type": "SaveImage",
            "inputs": { "images": ["6", 0], "filename_prefix": "zen-generate" }
        }
    })
}

/// Map A1111 sampler names to ComfyUI sampler_name values.
fn map_sampler(a1111_name: &str) -> &'static str {
    match a1111_name.to_lowercase().as_str() {
        "euler a" | "euler_a" => "euler_ancestral",
        "euler" => "euler",
        "lms" => "lms",
        "heun" => "heun",
        "dpm2" => "dpm_2",
        "dpm2 a" => "dpm_2_ancestral",
        "dpm++ 2s a" | "dpmpp_2s_a" => "dpmpp_2s_ancestral",
        "dpm++ 2m" | "dpmpp_2m" => "dpmpp_2m",
        "dpm++ sde" | "dpmpp_sde" => "dpmpp_sde",
        "dpm++ 2m sde" | "dpmpp_2m_sde" => "dpmpp_2m_sde",
        "ddim" => "ddim",
        "uni_pc" | "unipc" => "uni_pc",
        _ => "euler",
    }
}

/// Map A1111 sampler to ComfyUI scheduler.
fn map_scheduler(a1111_name: &str) -> &'static str {
    let lower = a1111_name.to_lowercase();
    if lower.contains("karras") {
        "karras"
    } else {
        "normal"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthesize_basic_workflow() {
        let params = GenerationParams {
            prompt: "a cat".into(),
            negative_prompt: "ugly".into(),
            steps: Some(30),
            cfg_scale: Some(7.0),
            sampler: Some("Euler a".into()),
            seed: Some(42),
            model: Some("dreamshaper.safetensors".into()),
            width: Some(512),
            height: Some(768),
            ..Default::default()
        };

        let wf = synthesize_txt2img(&params);
        assert_eq!(wf["1"]["inputs"]["ckpt_name"], "dreamshaper.safetensors");
        assert_eq!(wf["2"]["inputs"]["text"], "a cat");
        assert_eq!(wf["3"]["inputs"]["text"], "ugly");
        assert_eq!(wf["4"]["inputs"]["steps"], 30);
        assert_eq!(wf["4"]["inputs"]["seed"], 42);
        assert_eq!(wf["5"]["inputs"]["width"], 512);
        assert_eq!(wf["5"]["inputs"]["height"], 768);
    }

    #[test]
    fn synthesize_with_lora() {
        let params = GenerationParams {
            prompt: "test".into(),
            model: Some("base.safetensors".into()),
            ..Default::default()
        };

        let wf = synthesize_txt2img_with_lora(&params, "detail_lora.safetensors", 0.8);
        assert_eq!(wf["8"]["class_type"], "LoraLoader");
        assert_eq!(wf["8"]["inputs"]["lora_name"], "detail_lora.safetensors");
        assert_eq!(wf["8"]["inputs"]["strength_model"], 0.8);
        // CLIP should go through LoRA, not directly from checkpoint
        assert_eq!(wf["2"]["inputs"]["clip"], serde_json::json!(["8", 1]));
    }

    #[test]
    fn sampler_mapping() {
        assert_eq!(map_sampler("Euler a"), "euler_ancestral");
        assert_eq!(map_sampler("DPM++ 2M"), "dpmpp_2m");
        assert_eq!(map_sampler("unknown_sampler"), "euler");
    }
}
