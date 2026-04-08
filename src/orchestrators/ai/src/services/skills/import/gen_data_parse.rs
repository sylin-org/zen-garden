//! Parse A1111-format generation data text into structured parameters.
//!
//! Format:
//! ```text
//! {positive prompt}
//! Negative prompt: {negative prompt}
//! Steps: 30, CFG scale: 7, Sampler: Euler a, Seed: 12345, Model: name, ...
//! ```

use std::collections::HashMap;

/// Parsed generation parameters.
#[derive(Debug, Clone, Default)]
pub struct GenerationParams {
    pub prompt: String,
    pub negative_prompt: String,
    pub steps: Option<u32>,
    pub cfg_scale: Option<f64>,
    pub sampler: Option<String>,
    pub seed: Option<u64>,
    pub model: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub clip_skip: Option<u32>,
    /// All key-value pairs from the parameters line (for extensibility).
    pub extra: HashMap<String, String>,
}

/// Parse A1111-format generation data text.
pub fn parse(text: &str) -> GenerationParams {
    let mut params = GenerationParams::default();
    let text = text.trim();

    // Split into sections:
    // 1. Everything before "Negative prompt:" is the positive prompt
    // 2. Between "Negative prompt:" and the parameters line is the negative prompt
    // 3. The line starting with "Steps:" (or similar) is key-value pairs

    let (prompt_part, rest) = split_at_marker(text, "Negative prompt:");
    params.prompt = prompt_part.trim().to_string();

    let (negative_part, params_part) = if !rest.is_empty() {
        split_at_params_line(&rest)
    } else {
        // No negative prompt marker — check if there's a params line in the prompt part
        let (p, params_line) = split_at_params_line(&params.prompt);
        params.prompt = p.trim().to_string();
        (String::new(), params_line)
    };

    params.negative_prompt = negative_part.trim().to_string();

    // Parse key-value pairs from the parameters line
    if !params_part.is_empty() {
        let kvs = parse_kv_line(&params_part);
        params.extra = kvs.clone();

        params.steps = kvs.get("Steps").and_then(|v| v.parse().ok());
        params.cfg_scale = kvs.get("CFG scale").and_then(|v| v.parse().ok());
        params.sampler = kvs.get("Sampler").map(|v| v.to_string());
        params.seed = kvs.get("Seed").and_then(|v| v.parse().ok());
        params.model = kvs.get("Model").map(|v| v.to_string());
        params.clip_skip = kvs.get("Clip skip").and_then(|v| v.parse().ok());

        if let Some(w) = kvs.get("width").or(kvs.get("Size")) {
            if let Some((ww, hh)) = w.split_once('x') {
                params.width = ww.trim().parse().ok();
                params.height = hh.trim().parse().ok();
            } else {
                params.width = w.parse().ok();
            }
        }
        if params.width.is_none() {
            params.width = kvs.get("width").and_then(|v| v.parse().ok());
        }
        if params.height.is_none() {
            params.height = kvs.get("height").and_then(|v| v.parse().ok());
        }
    }

    params
}

/// Split text at the first occurrence of a marker.
fn split_at_marker(text: &str, marker: &str) -> (String, String) {
    if let Some(pos) = text.find(marker) {
        let before = text[..pos].to_string();
        let after = text[pos + marker.len()..].to_string();
        (before, after)
    } else {
        (text.to_string(), String::new())
    }
}

/// Split text at the parameters line (starts with "Steps:" or similar).
fn split_at_params_line(text: &str) -> (String, String) {
    // The params line typically starts with "Steps:" as the first key
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("Steps:") || trimmed.starts_with("steps:") {
            let before: String = text.lines().take(i).collect::<Vec<_>>().join("\n");
            let params: String = text.lines().skip(i).collect::<Vec<_>>().join(", ");
            return (before, params);
        }
    }
    (text.to_string(), String::new())
}

/// Parse "Key1: Value1, Key2: Value2, ..." into a map.
fn parse_kv_line(line: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();
    let mut current_key = String::new();
    let mut current_value = String::new();
    let mut in_value = false;

    for part in line.split(',') {
        let part = part.trim();
        if let Some((key, value)) = part.split_once(':') {
            // Save previous pair
            if in_value && !current_key.is_empty() {
                result.insert(current_key.trim().to_string(), current_value.trim().to_string());
            }
            current_key = key.trim().to_string();
            current_value = value.trim().to_string();
            in_value = true;
        } else if in_value {
            // Continuation of previous value (value contained a comma)
            current_value.push(',');
            current_value.push_str(part);
        }
    }

    // Save last pair
    if !current_key.is_empty() {
        result.insert(current_key.trim().to_string(), current_value.trim().to_string());
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_generation_data() {
        let text = "a beautiful landscape, mountains, sunset\n\
                     Negative prompt: ugly, blurry, watermark\n\
                     Steps: 30, CFG scale: 7, Sampler: Euler a, Seed: 773185375666026, Model: aMixIllustrious_aMix, width: 1536, height: 2304, Version: ComfyUI, Clip skip: 2";

        let p = parse(text);
        assert_eq!(p.prompt, "a beautiful landscape, mountains, sunset");
        assert_eq!(p.negative_prompt, "ugly, blurry, watermark");
        assert_eq!(p.steps, Some(30));
        assert_eq!(p.cfg_scale, Some(7.0));
        assert_eq!(p.sampler.as_deref(), Some("Euler a"));
        assert_eq!(p.seed, Some(773185375666026));
        assert_eq!(p.model.as_deref(), Some("aMixIllustrious_aMix"));
        assert_eq!(p.width, Some(1536));
        assert_eq!(p.height, Some(2304));
        assert_eq!(p.clip_skip, Some(2));
    }

    #[test]
    fn parse_prompt_only() {
        let text = "just a prompt with no other data";
        let p = parse(text);
        assert_eq!(p.prompt, "just a prompt with no other data");
        assert_eq!(p.negative_prompt, "");
        assert!(p.steps.is_none());
    }

    #[test]
    fn parse_with_size_field() {
        let text = "prompt\nNegative prompt: neg\nSteps: 20, Size: 512x768";
        let p = parse(text);
        assert_eq!(p.width, Some(512));
        assert_eq!(p.height, Some(768));
    }

    #[test]
    fn parse_multiline_prompt() {
        let text = "line one of prompt,\nline two of prompt\nNegative prompt: bad\nSteps: 25, CFG scale: 8";
        let p = parse(text);
        assert!(p.prompt.contains("line one"));
        assert!(p.prompt.contains("line two"));
        assert_eq!(p.steps, Some(25));
    }
}
