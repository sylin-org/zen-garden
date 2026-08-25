//! Input type detection — classify raw input without performing any I/O.

/// Classified input type.
#[derive(Debug)]
pub enum InputType {
    /// CivitAI image page URL.
    CivitaiImage { image_id: u64 },
    /// CivitAI model page URL (may contain downloadable workflow).
    CivitaiModel { model_id: u64, version_id: Option<u64> },
    /// Direct URL to a PNG file.
    PngUrl { url: String },
    /// Direct URL to a non-PNG resource (try as JSON).
    GenericUrl { url: String },
    /// Raw ComfyUI API-format workflow JSON.
    WorkflowJson { json: serde_json::Value },
    /// A1111-format generation data text.
    GenerationText { text: String },
}

/// Classify the text input. Pure function — no I/O, no network calls.
pub fn classify(input: &str) -> Result<InputType, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("empty input".into());
    }

    // 1a. CivitAI image URL
    if let Some(image_id) = parse_civitai_image_id(input) {
        return Ok(InputType::CivitaiImage { image_id });
    }

    // 1b. CivitAI model URL
    if let Some((model_id, version_id)) = parse_civitai_model_id(input) {
        return Ok(InputType::CivitaiModel { model_id, version_id });
    }

    // 2. URL — check if PNG or generic
    if input.starts_with("http://") || input.starts_with("https://") {
        let lower = input.to_lowercase();
        if lower.ends_with(".png") || lower.contains(".png?") {
            return Ok(InputType::PngUrl { url: input.to_string() });
        }
        return Ok(InputType::GenericUrl { url: input.to_string() });
    }

    // 3. Raw JSON — try to parse
    if input.starts_with('{') {
        match serde_json::from_str::<serde_json::Value>(input) {
            Ok(json) => {
                if is_comfyui_workflow(&json) {
                    return Ok(InputType::WorkflowJson { json });
                }
                return Err("JSON parsed but does not look like a ComfyUI workflow (no class_type fields)".into());
            }
            Err(e) => return Err(format!("looks like JSON but failed to parse: {e}")),
        }
    }

    // 4. A1111-format generation data text
    if looks_like_generation_text(input) {
        return Ok(InputType::GenerationText { text: input.to_string() });
    }

    Err("unrecognized input — provide a CivitAI URL, PNG URL, workflow JSON, or generation data text".into())
}

/// Classify raw bytes.
pub fn classify_bytes(bytes: &[u8]) -> Result<InputType, String> {
    // PNG magic bytes
    if is_png_bytes(bytes) {
        // Can't return a URL — the bytes ARE the content.
        // The caller handles this as a special case.
        return Err("png_bytes".into()); // sentinel — caller checks is_png_bytes directly
    }

    // Try as UTF-8 text
    if let Ok(text) = std::str::from_utf8(bytes) {
        return classify(text);
    }

    Err("binary data that is not a PNG and not valid UTF-8".into())
}

// ── Helpers ───────────────────────────────────────────────────

/// Extract model ID (and optional version ID) from a CivitAI model URL.
/// Handles: civitai.com/models/2226355, civitai.com/models/2226355/slug-name,
/// civitai.com/models/2226355?modelVersionId=2506390
fn parse_civitai_model_id(input: &str) -> Option<(u64, Option<u64>)> {
    let pattern = "civitai.com/models/";
    let pos = input.find(pattern)?;
    let after = &input[pos + pattern.len()..];
    let id_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    let model_id = id_str.parse::<u64>().ok()?;

    // Check for ?modelVersionId= query param
    let version_id = input
        .find("modelVersionId=")
        .and_then(|p| {
            let after_param = &input[p + "modelVersionId=".len()..];
            let vid_str: String = after_param.chars().take_while(|c| c.is_ascii_digit()).collect();
            vid_str.parse::<u64>().ok()
        });

    Some((model_id, version_id))
}

/// Extract image ID from a CivitAI image URL.
fn parse_civitai_image_id(input: &str) -> Option<u64> {
    let patterns = ["civitai.com/images/"];
    for pattern in &patterns {
        if let Some(pos) = input.find(pattern) {
            let after = &input[pos + pattern.len()..];
            let id_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(id) = id_str.parse::<u64>() {
                return Some(id);
            }
        }
    }
    None
}

/// Check if a JSON value looks like a ComfyUI API-format workflow.
pub fn is_comfyui_workflow(value: &serde_json::Value) -> bool {
    if let Some(obj) = value.as_object() {
        obj.values().any(|node| node.get("class_type").is_some())
    } else {
        false
    }
}

/// Check if raw bytes start with PNG magic.
pub fn is_png_bytes(data: &[u8]) -> bool {
    data.len() >= 8 && data[..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
}

/// Heuristic: does this text look like A1111-format generation data?
fn looks_like_generation_text(text: &str) -> bool {
    // Must have at least one of these marker lines
    let markers = ["Negative prompt:", "Steps:", "Sampler:", "CFG scale:", "Model:"];
    markers.iter().any(|m| text.contains(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_civitai_url() {
        match classify("https://civitai.com/images/125682754").unwrap() {
            InputType::CivitaiImage { image_id } => assert_eq!(image_id, 125682754),
            other => panic!("expected CivitaiImage, got {:?}", other),
        }
    }

    #[test]
    fn classify_civitai_url_with_params() {
        match classify("https://civitai.com/images/123456?foo=bar").unwrap() {
            InputType::CivitaiImage { image_id } => assert_eq!(image_id, 123456),
            other => panic!("expected CivitaiImage, got {:?}", other),
        }
    }

    #[test]
    fn classify_civitai_model_url() {
        match classify("https://civitai.com/models/2226355/luneva-workflow").unwrap() {
            InputType::CivitaiModel { model_id, version_id } => {
                assert_eq!(model_id, 2226355);
                assert_eq!(version_id, None);
            }
            other => panic!("expected CivitaiModel, got {:?}", other),
        }
    }

    #[test]
    fn classify_civitai_model_url_with_version() {
        match classify("https://civitai.com/models/2226355?modelVersionId=2506390").unwrap() {
            InputType::CivitaiModel { model_id, version_id } => {
                assert_eq!(model_id, 2226355);
                assert_eq!(version_id, Some(2506390));
            }
            other => panic!("expected CivitaiModel, got {:?}", other),
        }
    }

    #[test]
    fn classify_png_url() {
        match classify("https://example.com/image.png").unwrap() {
            InputType::PngUrl { url } => assert_eq!(url, "https://example.com/image.png"),
            other => panic!("expected PngUrl, got {:?}", other),
        }
    }

    #[test]
    fn classify_generic_url() {
        match classify("https://example.com/api/workflow").unwrap() {
            InputType::GenericUrl { .. } => {}
            other => panic!("expected GenericUrl, got {:?}", other),
        }
    }

    #[test]
    fn classify_workflow_json() {
        let json = r#"{"1":{"class_type":"LoadImage","inputs":{"image":"test.png"}}}"#;
        match classify(json).unwrap() {
            InputType::WorkflowJson { .. } => {}
            other => panic!("expected WorkflowJson, got {:?}", other),
        }
    }

    #[test]
    fn classify_invalid_json() {
        assert!(classify("{not valid json").is_err());
    }

    #[test]
    fn classify_non_workflow_json() {
        assert!(classify(r#"{"foo": "bar"}"#).is_err());
    }

    #[test]
    fn classify_generation_text() {
        let text = "a beautiful landscape\nNegative prompt: ugly\nSteps: 20, CFG scale: 7, Sampler: Euler a, Seed: 123";
        match classify(text).unwrap() {
            InputType::GenerationText { .. } => {}
            other => panic!("expected GenerationText, got {:?}", other),
        }
    }

    #[test]
    fn classify_empty() {
        assert!(classify("").is_err());
    }

    #[test]
    fn is_png_magic() {
        assert!(is_png_bytes(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]));
        assert!(!is_png_bytes(b"not png"));
        assert!(!is_png_bytes(&[]));
    }
}
