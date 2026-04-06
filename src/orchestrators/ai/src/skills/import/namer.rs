//! Skill naming — use the garden's own chat model to generate meaningful
//! names and descriptions for imported skills (ORCH-0026).
//!
//! Sends generation context (prompt, negative, models, parameters) to
//! the orchestrator's Ollama proxy at localhost:21434. Text-only — no
//! image processing, no vision model dependency.
//!
//! Graceful degradation: if no chat model is available or the request
//! times out, returns None and the caller falls back to heuristic naming.

use reqwest::Client;
use std::time::Duration;

/// Naming timeout — don't block import for slow inference.
const NAMING_TIMEOUT: Duration = Duration::from_secs(30);

/// The Ollama proxy endpoint (inside the same container).
const OLLAMA_PROXY: &str = "http://localhost:21434";

/// Generated name and description for a skill.
#[derive(Debug)]
pub struct SkillNaming {
    pub name: String,
    pub description: String,
}

/// Generation context for the naming prompt.
pub struct NamingContext {
    pub prompt: String,
    pub negative_prompt: String,
    pub model_names: Vec<String>,
    pub steps: Option<u32>,
    pub cfg_scale: Option<f64>,
    pub sampler: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// Generate a skill name and description using the garden's chat model.
///
/// Returns `None` on any failure — naming is best-effort, never blocks import.
pub async fn generate_name(http: &Client, ctx: &NamingContext) -> Option<SkillNaming> {
    let prompt = build_prompt(ctx);

    let req = serde_json::json!({
        "model": "recommended:chat",
        "messages": [{"role": "user", "content": prompt}],
        "stream": false,
    });

    let resp = http
        .post(format!("{OLLAMA_PROXY}/api/chat"))
        .json(&req)
        .timeout(NAMING_TIMEOUT)
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        tracing::debug!(
            status = %resp.status(),
            "namer: chat model request failed"
        );
        return None;
    }

    let body: serde_json::Value = resp.json().await.ok()?;
    let content = body.get("message")?.get("content")?.as_str()?;

    let naming = parse_response(content)?;

    tracing::info!(
        name = %naming.name,
        description = %naming.description,
        model = body.get("model").and_then(|v| v.as_str()).unwrap_or("?"),
        "namer: generated skill name"
    );

    Some(naming)
}

/// Build the naming prompt from generation context.
fn build_prompt(ctx: &NamingContext) -> String {
    let mut parts = Vec::new();

    parts.push("You are naming an AI image generation skill for a dashboard.".to_string());
    parts.push(String::new());
    parts.push("Given these generation parameters, provide:".to_string());
    parts.push("1. A concise skill name (3-5 words, title case, no technical jargon)".to_string());
    parts.push("2. A one-sentence description of what this skill produces".to_string());
    parts.push(String::new());
    parts.push("The name should describe the visual style or subject, NOT the model name.".to_string());
    parts.push(String::new());

    if !ctx.prompt.is_empty() {
        // Truncate very long prompts
        let truncated = if ctx.prompt.len() > 300 {
            format!("{}...", &ctx.prompt[..300])
        } else {
            ctx.prompt.clone()
        };
        parts.push(format!("Prompt: {truncated}"));
    }

    if !ctx.negative_prompt.is_empty() {
        let truncated = if ctx.negative_prompt.len() > 200 {
            format!("{}...", &ctx.negative_prompt[..200])
        } else {
            ctx.negative_prompt.clone()
        };
        parts.push(format!("Negative: {truncated}"));
    }

    if !ctx.model_names.is_empty() {
        parts.push(format!("Models: {}", ctx.model_names.join(", ")));
    }

    let mut params = Vec::new();
    if let Some(steps) = ctx.steps {
        params.push(format!("{steps} steps"));
    }
    if let Some(cfg) = ctx.cfg_scale {
        params.push(format!("CFG {cfg}"));
    }
    if let Some(ref sampler) = ctx.sampler {
        params.push(sampler.clone());
    }
    if let (Some(w), Some(h)) = (ctx.width, ctx.height) {
        params.push(format!("{w}x{h}"));
    }
    if !params.is_empty() {
        parts.push(format!("Parameters: {}", params.join(", ")));
    }

    parts.push(String::new());
    parts.push(r#"Respond ONLY with JSON, no markdown, no explanation: {"name": "...", "description": "..."}"#.to_string());

    parts.join("\n")
}

/// Parse the model response, extracting name and description from JSON.
///
/// Handles common LLM response quirks: markdown code fences, extra text.
fn parse_response(content: &str) -> Option<SkillNaming> {
    let content = content.trim();

    // Strip markdown code fences if present
    let json_str = if content.starts_with("```") {
        content
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim()
    } else {
        content
    };

    // Try to find JSON object in the response
    let json_str = if let Some(start) = json_str.find('{') {
        if let Some(end) = json_str.rfind('}') {
            &json_str[start..=end]
        } else {
            json_str
        }
    } else {
        json_str
    };

    let parsed: serde_json::Value = serde_json::from_str(json_str).ok()?;

    let name = parsed.get("name")?.as_str()?.trim().to_string();
    let description = parsed.get("description")?.as_str()?.trim().to_string();

    // Validate: name should be reasonable
    if name.is_empty() || name.len() > 100 {
        return None;
    }
    if description.is_empty() || description.len() > 500 {
        return None;
    }

    Some(SkillNaming { name, description })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_clean_json() {
        let r = parse_response(r#"{"name": "Anime Cat Portrait", "description": "Generates anime-style portraits with cats."}"#).unwrap();
        assert_eq!(r.name, "Anime Cat Portrait");
        assert!(r.description.contains("anime"));
    }

    #[test]
    fn parse_with_code_fence() {
        let r = parse_response("```json\n{\"name\": \"Dark Fantasy\", \"description\": \"Moody art.\"}\n```").unwrap();
        assert_eq!(r.name, "Dark Fantasy");
    }

    #[test]
    fn parse_with_extra_text() {
        let r = parse_response("Here is the result:\n{\"name\": \"Test\", \"description\": \"Desc.\"}\nHope this helps!").unwrap();
        assert_eq!(r.name, "Test");
    }

    #[test]
    fn parse_empty_name_rejected() {
        assert!(parse_response(r#"{"name": "", "description": "Desc"}"#).is_none());
    }

    #[test]
    fn parse_garbage_rejected() {
        assert!(parse_response("not json at all").is_none());
    }

    #[test]
    fn prompt_building() {
        let ctx = NamingContext {
            prompt: "a cat sitting on a windowsill".into(),
            negative_prompt: "ugly, blurry".into(),
            model_names: vec!["dreamshaper (checkpoint)".into(), "detail_lora (lora)".into()],
            steps: Some(20),
            cfg_scale: Some(7.0),
            sampler: Some("Euler a".into()),
            width: Some(512),
            height: Some(768),
        };
        let prompt = build_prompt(&ctx);
        assert!(prompt.contains("a cat sitting"));
        assert!(prompt.contains("dreamshaper"));
        assert!(prompt.contains("20 steps"));
        assert!(prompt.contains("512x768"));
    }
}
