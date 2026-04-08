//! AI-assisted skill naming (ORCH-0029 Phase 3).
//!
//! Uses the orchestrator's own `text.chat` primitive with
//! `selectors.model = "recommended:chat"` to generate a human-
//! friendly name + description for an imported skill.
//!
//! **Rewired from the prior system**: the old implementation HTTP-
//! POSTed to `localhost:21434/api/chat` (the Ollama proxy). This
//! version calls [`Dispatcher::dispatch`] directly. The orchestrator
//! IS the chat router now, so we avoid the extra network hop and
//! benefit from the same error handling, recommendation engine, and
//! per-primitive dispatch flow any other chat caller gets.
//!
//! Naming is **best-effort**: any failure returns `None` and the
//! caller falls back to heuristic naming.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio_util::sync::CancellationToken;
use tracing::Span;

use crate::domain::ids::{CorrelationId, RequestId};
use crate::domain::keys;
use crate::domain::primitive::Primitive;
use crate::domain::request::{Action, RawRequest};
use crate::domain::selectors::{Constraints, Selectors};
use crate::services::dispatcher::{DispatchResult, Dispatcher};

/// Naming timeout — don't block imports on slow inference. The
/// dispatcher's own timeout applies too; this is an upper bound.
const NAMING_TIMEOUT: Duration = Duration::from_secs(30);

/// Generation context the namer feeds to the chat model.
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

/// Returned name + description pair.
#[derive(Debug, Clone)]
pub struct SkillNaming {
    pub name: String,
    pub description: String,
}

/// Generate a skill name + description via the garden's own chat
/// model. Returns `None` on any failure.
pub async fn generate_name(
    dispatcher: &Arc<Dispatcher>,
    ctx: &NamingContext,
) -> Option<SkillNaming> {
    let prompt = build_prompt(ctx);

    let payload = serde_json::json!({
        "text": {
            "prompt": { "user": prompt },
            "sampling": { "temperature": 0.3 },
            "tokens": { "max": 256 },
        }
    });

    let raw = RawRequest {
        id: RequestId::generate(),
        correlation_id: CorrelationId::generate(),
        received_at: Utc::now(),
        action: Action::bare(Primitive::TextChat),
        payload,
        selectors: Selectors {
            model: Some("recommended:chat".to_string()),
            ..Default::default()
        },
        constraints: Constraints::default(),
        cancel: CancellationToken::new(),
        span: Span::current(),
    };

    // Apply our wall-clock cap on top of whatever the dispatcher's
    // own timeout is — the namer must NEVER block import for long.
    let result = tokio::time::timeout(NAMING_TIMEOUT, dispatcher.dispatch(raw))
        .await
        .ok()?
        .ok()?;

    let output = match result {
        DispatchResult::Fresh(outcome, _) => extract_sync_output(outcome)?,
        DispatchResult::Cached(record, _) => extract_cached_output(record)?,
    };

    let response_text = output
        .get(&keys::text::RESPONSE)
        .and_then(|v| v.as_str())
        .map(String::from)?;

    let naming = parse_response(&response_text)?;
    tracing::info!(
        name = %naming.name,
        description = %naming.description,
        "namer: generated skill name via internal dispatcher"
    );
    Some(naming)
}

fn extract_sync_output(
    outcome: crate::domain::provider::ProviderOutcome,
) -> Option<crate::domain::output::Output> {
    match outcome {
        crate::domain::provider::ProviderOutcome::Sync(out) => Some(out),
        // Async and Streaming outcomes aren't useful for naming — the
        // caller is expected to await the response inline.
        _ => None,
    }
}

fn extract_cached_output(
    record: crate::domain::idempotency::IdempotencyRecord,
) -> Option<crate::domain::output::Output> {
    match record.response {
        crate::domain::idempotency::CachedResponse::Sync { output } => Some(output),
        _ => None,
    }
}

/// Build the naming prompt from generation context.
fn build_prompt(ctx: &NamingContext) -> String {
    let mut parts: Vec<String> = Vec::new();

    parts.push("You are naming an AI image generation skill for a dashboard.".into());
    parts.push(String::new());
    parts.push("Given these generation parameters, provide:".into());
    parts.push("1. A concise skill name (3-5 words, title case, no technical jargon)".into());
    parts.push("2. A one-sentence description of what this skill produces".into());
    parts.push(String::new());
    parts.push(
        "The name should describe the visual style or subject, NOT the model name.".into(),
    );
    parts.push(String::new());

    if !ctx.prompt.is_empty() {
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
    parts.push(
        r#"Respond ONLY with JSON, no markdown, no explanation: {"name": "...", "description": "..."}"#
            .into(),
    );

    parts.join("\n")
}

/// Parse the model response, handling common LLM quirks (code
/// fences, extra narrative text around the JSON blob).
fn parse_response(content: &str) -> Option<SkillNaming> {
    let content = content.trim();

    // Strip markdown code fences.
    let json_str = if content.starts_with("```") {
        content
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim()
    } else {
        content
    };

    // Find the first JSON object in the response.
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
        let r = parse_response(
            r#"{"name": "Anime Cat Portrait", "description": "Generates anime-style portraits with cats."}"#,
        )
        .unwrap();
        assert_eq!(r.name, "Anime Cat Portrait");
        assert!(r.description.contains("anime"));
    }

    #[test]
    fn parse_with_code_fence() {
        let r = parse_response(
            "```json\n{\"name\": \"Dark Fantasy\", \"description\": \"Moody art.\"}\n```",
        )
        .unwrap();
        assert_eq!(r.name, "Dark Fantasy");
    }

    #[test]
    fn parse_with_extra_text() {
        let r = parse_response(
            "Here is the result:\n{\"name\": \"Test\", \"description\": \"Desc.\"}\nHope this helps!",
        )
        .unwrap();
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
            model_names: vec![
                "dreamshaper (checkpoint)".into(),
                "detail_lora (lora)".into(),
            ],
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
