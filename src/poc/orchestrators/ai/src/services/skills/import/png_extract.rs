//! Extract ComfyUI workflow from PNG tEXt/zTXt/iTXt chunks.
//!
//! ComfyUI writes two chunks:
//! - `prompt`: API format (execution graph) — what we need
//! - `workflow`: Editor format (positions, links) — informational
//!
//! Some images also have `parameters`: A1111-compatible text.

use anyhow::{Context, Result};

/// Extracted data from a PNG file.
#[derive(Debug)]
pub struct PngExtraction {
    /// ComfyUI API-format workflow (from `prompt` chunk).
    pub workflow: Option<serde_json::Value>,
    /// A1111-format parameters text (from `parameters` chunk).
    pub parameters_text: Option<String>,
}

/// Extract workflow metadata from PNG bytes.
/// Returns Ok with potentially empty fields — the caller decides what's required.
pub fn extract(png_bytes: &[u8]) -> Result<PngExtraction> {
    let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    let reader = decoder.read_info().context("invalid PNG")?;
    let info = reader.info();

    let mut workflow = None;
    let mut parameters_text = None;

    // tEXt chunks (uncompressed)
    for chunk in &info.uncompressed_latin1_text {
        match chunk.keyword.as_str() {
            "prompt" if workflow.is_none() => {
                workflow = try_parse_json(&chunk.text);
            }
            "parameters" if parameters_text.is_none() => {
                parameters_text = Some(chunk.text.clone());
            }
            _ => {}
        }
    }

    // zTXt chunks (compressed)
    for chunk in &info.compressed_latin1_text {
        let text = chunk.get_text().unwrap_or_default();
        match chunk.keyword.as_str() {
            "prompt" if workflow.is_none() => {
                workflow = try_parse_json(&text);
            }
            "parameters" if parameters_text.is_none() => {
                parameters_text = Some(text);
            }
            _ => {}
        }
    }

    // iTXt chunks (international text)
    for chunk in &info.utf8_text {
        let text = chunk.get_text().unwrap_or_default();
        match chunk.keyword.as_str() {
            "prompt" if workflow.is_none() => {
                workflow = try_parse_json(&text);
            }
            "parameters" if parameters_text.is_none() => {
                parameters_text = Some(text);
            }
            _ => {}
        }
    }

    Ok(PngExtraction { workflow, parameters_text })
}

fn try_parse_json(text: &str) -> Option<serde_json::Value> {
    serde_json::from_str(text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::skills::import::input_detect;

    #[test]
    fn png_magic_detection() {
        assert!(input_detect::is_png_bytes(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]));
        assert!(!input_detect::is_png_bytes(b"not png"));
    }
}
