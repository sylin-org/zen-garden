//! ComfyUI UI-format → API-format workflow converter.
//!
//! ComfyUI has two JSON formats:
//! - **UI format**: exported by "Save" button (`nodes`, `links`, `groups`)
//! - **API format**: consumed by `/prompt` endpoint (`{ "1": { class_type, inputs } }`)
//!
//! CivitAI workflow downloads use UI format. Our execution engine needs API format.
//! This module converts between them.
//!
//! The conversion challenge: `widgets_values` is a positional array — mapping
//! positions to input names requires knowing each node type's widget order.
//! We use a static lookup table for built-in nodes. Unknown nodes preserve
//! link connections (named in UI format) but skip widget values.

use anyhow::{Context, Result};
use std::collections::HashMap;

// ── Public API ───────────────────────────────────────────────

/// Convert a ComfyUI UI-format workflow to API format.
///
/// Returns `None` if the input is already API format or not a valid UI workflow.
/// Returns `Some(api_workflow)` on successful conversion.
pub fn convert(ui_workflow: &serde_json::Value) -> Result<serde_json::Value> {
    let nodes = ui_workflow.get("nodes")
        .and_then(|v| v.as_array())
        .context("UI workflow missing 'nodes' array")?;

    let links_raw = ui_workflow.get("links")
        .and_then(|v| v.as_array())
        .context("UI workflow missing 'links' array")?;

    // Step 1: Build link lookup — link_id → (from_node_id, from_slot)
    let links = parse_links(links_raw);

    // Step 2: Convert each node
    let mut api = serde_json::Map::new();
    let mut converted = 0u32;
    let mut skipped_widgets = 0u32;

    for node in nodes {
        let id = match node.get("id").and_then(|v| v.as_u64()) {
            Some(id) => id.to_string(),
            None => continue,
        };

        let class_type = match node.get("type").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => continue,
        };

        // Skip special UI-only nodes
        if is_ui_only_node(class_type) {
            continue;
        }

        let mut inputs = serde_json::Map::new();

        // Step 2a: Link-based inputs (connected wires)
        // These are named in the UI format: { name, type, link }
        if let Some(node_inputs) = node.get("inputs").and_then(|v| v.as_array()) {
            for input in node_inputs {
                let name = match input.get("name").and_then(|v| v.as_str()) {
                    Some(n) => n,
                    None => continue,
                };
                let link_id = match input.get("link").and_then(|v| v.as_u64()) {
                    Some(id) => id,
                    None => continue, // unconnected slot
                };
                if let Some(&(from_id, from_slot)) = links.get(&link_id) {
                    inputs.insert(
                        name.to_string(),
                        serde_json::json!([from_id.to_string(), from_slot]),
                    );
                }
            }
        }

        // Step 2b: Widget-based inputs (from widgets_values)
        if let Some(widgets) = node.get("widgets_values").and_then(|v| v.as_array()) {
            if let Some(widget_names) = get_widget_names(class_type) {
                let mut wi = 0; // widget_values index
                for name in widget_names {
                    if wi >= widgets.len() {
                        break;
                    }
                    match name {
                        WidgetSlot::Input(input_name) => {
                            inputs.insert(input_name.to_string(), widgets[wi].clone());
                            wi += 1;
                        }
                        WidgetSlot::Skip => {
                            // control_after_generate or other UI-only widget — skip the value
                            wi += 1;
                        }
                    }
                }
            } else {
                // Unknown node type — can't map widget values without definitions
                skipped_widgets += 1;
                tracing::debug!(
                    class_type,
                    widget_count = widgets.len(),
                    "ui_to_api: unknown node type — widget values not mapped"
                );
            }
        }

        api.insert(id, serde_json::json!({
            "class_type": class_type,
            "inputs": serde_json::Value::Object(inputs),
        }));
        converted += 1;
    }

    tracing::info!(
        nodes = converted,
        skipped_widgets,
        links = links.len(),
        "ui_to_api: conversion complete"
    );

    Ok(serde_json::Value::Object(api))
}

/// Check if a JSON value is a ComfyUI UI-format workflow.
pub fn is_ui_format(value: &serde_json::Value) -> bool {
    value.get("nodes").and_then(|v| v.as_array()).is_some()
        && value.get("links").and_then(|v| v.as_array()).is_some()
}

// ── Link Resolution ──────────────────────────────────────────

/// Parse the `links` array into a lookup: link_id → (from_node_id, from_slot).
///
/// Each link entry: `[link_id, from_node, from_slot, to_node, to_slot, type_name]`
fn parse_links(links: &[serde_json::Value]) -> HashMap<u64, (u64, u64)> {
    let mut map = HashMap::new();
    for link in links {
        let arr = match link.as_array() {
            Some(a) if a.len() >= 5 => a,
            _ => continue,
        };
        let link_id = arr[0].as_u64().unwrap_or(0);
        let from_node = arr[1].as_u64().unwrap_or(0);
        let from_slot = arr[2].as_u64().unwrap_or(0);
        map.insert(link_id, (from_node, from_slot));
    }
    map
}

// ── Widget Name Mapping ──────────────────────────────────────

/// A slot in the widget_values array.
enum WidgetSlot {
    /// Maps to this API input name.
    Input(&'static str),
    /// Skip this value (UI-only widget like control_after_generate).
    Skip,
}

/// UI-only routing nodes that don't produce API entries.
fn is_ui_only_node(class_type: &str) -> bool {
    matches!(class_type,
        "PrimitiveNode" | "Reroute" | "Note"
        | "SetNode" | "GetNode"
        | "workflow/integer" | "workflow/float" | "workflow/string"
    )
}

/// Return the widget-value-to-input-name mapping for a known node type.
///
/// Widget values appear in `widgets_values` in the order listed here.
/// `WidgetSlot::Skip` marks UI-only values like `control_after_generate`.
///
/// Returns `None` for unknown node types (widget values can't be mapped).
fn get_widget_names(class_type: &str) -> Option<&'static [WidgetSlot]> {
    use WidgetSlot::{Input as I, Skip as S};

    Some(match class_type {
        // ── Loaders ──────────────────────────────────────
        "CheckpointLoaderSimple" => &[I("ckpt_name")],
        "CheckpointLoader" => &[I("config_name"), I("ckpt_name")],
        "unCLIPCheckpointLoader" => &[I("ckpt_name")],
        "LoraLoader" => &[I("lora_name"), I("strength_model"), I("strength_clip")],
        "LoraLoaderModelOnly" => &[I("lora_name"), I("strength_model")],
        "VAELoader" => &[I("vae_name")],
        "CLIPLoader" => &[I("clip_name"), I("type")],
        "DualCLIPLoader" => &[I("clip_name1"), I("clip_name2"), I("type")],
        "ControlNetLoader" => &[I("control_net_name")],
        "UpscaleModelLoader" => &[I("model_name")],
        "UNETLoader" => &[I("unet_name"), I("weight_dtype")],
        "StyleModelLoader" => &[I("style_model_name")],
        "CLIPVisionLoader" => &[I("clip_name")],

        // ── Encoders ─────────────────────────────────────
        "CLIPTextEncode" => &[I("text")],
        "CLIPTextEncodeSDXL" => &[I("width"), I("height"), I("crop_w"), I("crop_h"),
                                   I("target_width"), I("target_height"), I("text_g"), I("text_l")],
        "CLIPTextEncodeSD3" => &[I("text"), I("clip_l"), I("clip_g"), I("t5xxl")],

        // ── Samplers ─────────────────────────────────────
        // KSampler: widgets = seed, CONTROL, steps, cfg, sampler_name, scheduler, denoise
        "KSampler" => &[I("seed"), S, I("steps"), I("cfg"), I("sampler_name"), I("scheduler"), I("denoise")],
        // KSamplerAdvanced: add_noise, noise_seed, CONTROL, steps, cfg, sampler_name, scheduler, start, end, return_noise
        "KSamplerAdvanced" => &[I("add_noise"), I("noise_seed"), S, I("steps"), I("cfg"),
                                 I("sampler_name"), I("scheduler"), I("start_at_step"),
                                 I("end_at_step"), I("return_with_leftover_noise")],
        "SamplerCustom" => &[I("add_noise"), I("noise_seed"), S, I("cfg"), I("positive"), I("negative")],

        // ── Latent ───────────────────────────────────────
        "EmptyLatentImage" => &[I("width"), I("height"), I("batch_size")],
        "EmptySD3LatentImage" => &[I("width"), I("height"), I("batch_size")],
        "LatentUpscale" => &[I("upscale_method"), I("width"), I("height"), I("crop")],
        "LatentUpscaleBy" => &[I("upscale_method"), I("scale_by")],
        "LatentComposite" => &[I("samples_to"), I("samples_from"), I("x"), I("y"), I("feather")],
        "LatentBlend" => &[I("blend_factor")],

        // ── Image ────────────────────────────────────────
        "LoadImage" => &[I("image")],
        "LoadImageMask" => &[I("image"), I("channel")],
        "SaveImage" => &[I("filename_prefix")],
        "PreviewImage" => &[],
        "ImageScale" => &[I("upscale_method"), I("width"), I("height"), I("crop")],
        "ImageScaleBy" => &[I("upscale_method"), I("scale_by")],
        "ImageUpscaleWithModel" => &[], // only link inputs
        "ImageInvert" => &[],
        "ImageBatch" => &[],
        "ImageSharpen" => &[I("sharpen_radius"), I("sigma"), I("alpha")],

        // ── VAE ──────────────────────────────────────────
        "VAEDecode" => &[],
        "VAEEncode" => &[],
        "VAEDecodeTiled" => &[I("tile_size")],
        "VAEEncodeTiled" => &[I("tile_size")],

        // ── Conditioning ─────────────────────────────────
        "ConditioningCombine" => &[],
        "ConditioningAverage" => &[I("conditioning_to_strength")],
        "ConditioningSetArea" => &[I("width"), I("height"), I("x"), I("y"), I("strength")],
        "ConditioningSetMask" => &[I("strength"), I("set_cond_area")],
        "ConditioningZeroOut" => &[],
        "ControlNetApply" => &[I("strength")],
        "ControlNetApplyAdvanced" => &[I("strength"), I("start_percent"), I("end_percent")],

        // ── Mask ─────────────────────────────────────────
        "SetLatentNoiseMask" => &[],
        "MaskToImage" => &[],
        "ImageToMask" => &[I("channel")],
        "SolidMask" => &[I("value"), I("width"), I("height")],

        // ── Audio ────────────────────────────────────────
        "SaveAudio" => &[I("filename_prefix")],
        "LoadAudio" => &[I("audio")],
        "PreviewAudio" => &[],

        // ── Model patches ────────────────────────────────
        "FreeU" | "FreeU_V2" => &[I("b1"), I("b2"), I("s1"), I("s2")],
        "ModelSamplingDiscrete" => &[I("sampling"), I("zsnr")],

        _ => return None,
    })
}

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_simple_workflow() {
        let ui = serde_json::json!({
            "nodes": [
                {
                    "id": 1,
                    "type": "CheckpointLoaderSimple",
                    "inputs": [],
                    "outputs": [
                        {"name": "MODEL", "links": [1]},
                        {"name": "CLIP", "links": [2, 3]},
                        {"name": "VAE", "links": [4]}
                    ],
                    "widgets_values": ["sd_v1-5.safetensors"]
                },
                {
                    "id": 2,
                    "type": "CLIPTextEncode",
                    "inputs": [{"name": "clip", "type": "CLIP", "link": 2}],
                    "widgets_values": ["a beautiful sunset"]
                },
                {
                    "id": 3,
                    "type": "CLIPTextEncode",
                    "inputs": [{"name": "clip", "type": "CLIP", "link": 3}],
                    "widgets_values": ["ugly, blurry"]
                },
                {
                    "id": 4,
                    "type": "KSampler",
                    "inputs": [
                        {"name": "model", "type": "MODEL", "link": 1},
                        {"name": "positive", "type": "CONDITIONING", "link": 5},
                        {"name": "negative", "type": "CONDITIONING", "link": 6},
                        {"name": "latent_image", "type": "LATENT", "link": 7}
                    ],
                    "widgets_values": [42, "fixed", 20, 7.0, "euler", "normal", 1.0]
                }
            ],
            "links": [
                [1, 1, 0, 4, 0, "MODEL"],
                [2, 1, 1, 2, 0, "CLIP"],
                [3, 1, 1, 3, 0, "CLIP"],
                [4, 1, 2, 6, 1, "VAE"],
                [5, 2, 0, 4, 1, "CONDITIONING"],
                [6, 3, 0, 4, 2, "CONDITIONING"],
                [7, 5, 0, 4, 3, "LATENT"]
            ],
            "last_node_id": 7,
            "last_link_id": 7
        });

        let api = convert(&ui).unwrap();

        // CheckpointLoaderSimple
        assert_eq!(api["1"]["class_type"], "CheckpointLoaderSimple");
        assert_eq!(api["1"]["inputs"]["ckpt_name"], "sd_v1-5.safetensors");

        // CLIPTextEncode (positive)
        assert_eq!(api["2"]["class_type"], "CLIPTextEncode");
        assert_eq!(api["2"]["inputs"]["text"], "a beautiful sunset");
        assert_eq!(api["2"]["inputs"]["clip"], serde_json::json!(["1", 1]));

        // KSampler — verify control_after_generate was skipped
        assert_eq!(api["4"]["class_type"], "KSampler");
        assert_eq!(api["4"]["inputs"]["seed"], 42);
        assert_eq!(api["4"]["inputs"]["steps"], 20);
        assert_eq!(api["4"]["inputs"]["cfg"], 7.0);
        assert_eq!(api["4"]["inputs"]["sampler_name"], "euler");
        assert_eq!(api["4"]["inputs"]["denoise"], 1.0);
        assert_eq!(api["4"]["inputs"]["model"], serde_json::json!(["1", 0]));
        assert!(api["4"]["inputs"].get("control_after_generate").is_none());
    }

    #[test]
    fn is_ui_format_detection() {
        let ui = serde_json::json!({"nodes": [], "links": []});
        assert!(is_ui_format(&ui));

        let api = serde_json::json!({"1": {"class_type": "Foo", "inputs": {}}});
        assert!(!is_ui_format(&api));
    }

    #[test]
    fn skip_ui_only_nodes() {
        let ui = serde_json::json!({
            "nodes": [
                {"id": 1, "type": "PrimitiveNode", "inputs": [], "widgets_values": [42]},
                {"id": 2, "type": "Note", "inputs": [], "widgets_values": ["reminder"]},
                {"id": 3, "type": "CheckpointLoaderSimple", "inputs": [], "widgets_values": ["model.safetensors"]}
            ],
            "links": []
        });

        let api = convert(&ui).unwrap();
        assert!(api.get("1").is_none(), "PrimitiveNode should be skipped");
        assert!(api.get("2").is_none(), "Note should be skipped");
        assert!(api.get("3").is_some(), "CheckpointLoaderSimple should be kept");
    }

    #[test]
    fn unknown_node_preserves_links() {
        let ui = serde_json::json!({
            "nodes": [
                {"id": 1, "type": "CheckpointLoaderSimple", "inputs": [], "widgets_values": ["m.safetensors"]},
                {
                    "id": 2, "type": "SomeCustomNode",
                    "inputs": [{"name": "model", "type": "MODEL", "link": 1}],
                    "widgets_values": ["unknown_value", 42]
                }
            ],
            "links": [[1, 1, 0, 2, 0, "MODEL"]]
        });

        let api = convert(&ui).unwrap();
        // Custom node: link preserved, widgets skipped
        assert_eq!(api["2"]["class_type"], "SomeCustomNode");
        assert_eq!(api["2"]["inputs"]["model"], serde_json::json!(["1", 0]));
        // Widget values NOT mapped (unknown node)
        assert!(api["2"]["inputs"].get("unknown_value").is_none());
    }
}
