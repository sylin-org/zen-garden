//! Extract ComfyUI workflow metadata from PNG tEXt/zTXt chunks.
//!
//! ComfyUI writes two chunks into saved PNGs:
//! - `prompt`: API format — `{ "node_id": { "class_type": "...", "inputs": {...} } }`
//! - `workflow`: Editor format — node graph with positions, links, groups
//!
//! For skill creation, the `prompt` chunk is what we need (matches our parser input).

use anyhow::{Context, Result};

/// Extracted workflow data from a PNG file.
#[derive(Debug)]
pub struct PngWorkflowData {
    /// ComfyUI API format — the execution graph. This is what we use.
    pub prompt: Option<serde_json::Value>,
    /// ComfyUI editor format — node positions, links. Informational only.
    pub workflow: Option<serde_json::Value>,
}

/// Extract ComfyUI workflow metadata from PNG bytes.
///
/// Reads tEXt and zTXt chunks looking for "prompt" and "workflow" keywords.
pub fn extract_from_png(png_bytes: &[u8]) -> Result<PngWorkflowData> {
    let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    let reader = decoder.read_info().context("decode PNG header")?;
    let info = reader.info();

    let mut prompt = None;
    let mut workflow = None;

    // Check uncompressed tEXt chunks
    for chunk in &info.uncompressed_latin1_text {
        match chunk.keyword.as_str() {
            "prompt" => {
                if let Ok(val) = serde_json::from_str(&chunk.text) {
                    prompt = Some(val);
                }
            }
            "workflow" => {
                if let Ok(val) = serde_json::from_str(&chunk.text) {
                    workflow = Some(val);
                }
            }
            _ => {}
        }
    }

    // Check compressed zTXt chunks
    for chunk in &info.compressed_latin1_text {
        let text = chunk
            .get_text()
            .unwrap_or_default();

        match chunk.keyword.as_str() {
            "prompt" if prompt.is_none() => {
                if let Ok(val) = serde_json::from_str(&text) {
                    prompt = Some(val);
                }
            }
            "workflow" if workflow.is_none() => {
                if let Ok(val) = serde_json::from_str(&text) {
                    workflow = Some(val);
                }
            }
            _ => {}
        }
    }

    // Also check iTXt (international text) chunks — some tools use these
    for chunk in &info.utf8_text {
        let text = chunk.get_text().unwrap_or_default();
        match chunk.keyword.as_str() {
            "prompt" if prompt.is_none() => {
                if let Ok(val) = serde_json::from_str(&text) {
                    prompt = Some(val);
                }
            }
            "workflow" if workflow.is_none() => {
                if let Ok(val) = serde_json::from_str(&text) {
                    workflow = Some(val);
                }
            }
            _ => {}
        }
    }

    Ok(PngWorkflowData { prompt, workflow })
}

/// Check if raw bytes look like a PNG file (magic bytes).
pub fn is_png(data: &[u8]) -> bool {
    data.len() >= 8 && data[..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
}

/// Check if a JSON value looks like a ComfyUI API-format workflow.
/// Must be an object where at least one value has a "class_type" field.
pub fn is_comfyui_workflow(value: &serde_json::Value) -> bool {
    if let Some(obj) = value.as_object() {
        obj.values().any(|node| {
            node.get("class_type").is_some()
        })
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_png_detects_magic_bytes() {
        let png_header = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00];
        assert!(is_png(&png_header));
        assert!(!is_png(b"not a png"));
        assert!(!is_png(&[]));
    }

    #[test]
    fn is_comfyui_workflow_checks_class_type() {
        let valid = serde_json::json!({
            "1": { "class_type": "LoadImage", "inputs": {} },
            "2": { "class_type": "SaveImage", "inputs": {} }
        });
        assert!(is_comfyui_workflow(&valid));

        let invalid = serde_json::json!({ "foo": "bar" });
        assert!(!is_comfyui_workflow(&invalid));

        let array = serde_json::json!([1, 2, 3]);
        assert!(!is_comfyui_workflow(&array));
    }
}
