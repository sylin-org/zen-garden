//! Flow executor — runs multi-step action DAGs (ORCH-0030 §7).
//!
//! A flow is a sequence of actions with inter-step references. Each
//! step's payload can contain `{{step_id.field.path}}` placeholders
//! that are resolved at execution time from completed upstream steps.
//!
//! # Example flow body:
//!
//! ```json
//! {
//!   "actions": [
//!     {
//!       "id": "transcribe",
//!       "action": "audio.transcribe",
//!       "payload": { "audio.source": { "media_id": "abc123" } }
//!     },
//!     {
//!       "id": "summarize",
//!       "action": "text.chat",
//!       "payload": {
//!         "text.prompt.user": "Summarize:\n{{transcribe.text.response}}"
//!       }
//!     }
//!   ]
//! }
//! ```
//!
//! The executor walks the steps in declaration order (linear for now;
//! DAG topological sort is a future enhancement). Each step's payload
//! is rendered by substituting `{{step_id.field}}` placeholders with
//! values from completed upstream results.
//!
//! # Events
//!
//! The executor publishes step-level events on the bus:
//!
//! - `jobs.{job_id}.step.{step_id}.state` — accepted | running | completed | failed
//! - `jobs.{job_id}.step.{step_id}.result` — terminal payload (on success)
//! - `jobs.{job_id}.state` — running (on start) | completed (all done) | failed (any step fails)
//! - `jobs.{job_id}.result` — aggregated results from all steps

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::domain::errors::ErrorCode;
use crate::domain::events::EventBus;
use crate::domain::ids::{CorrelationId, RequestId};
use crate::domain::request::Action;
use crate::services::dispatcher::Dispatcher;

/// A parsed flow definition with N steps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowDefinition {
    pub steps: Vec<FlowStep>,
}

/// One step in a flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowStep {
    /// Unique identifier within the flow, used in placeholder references.
    pub id: String,
    /// The action to execute (dotted form, e.g., "audio.transcribe").
    pub action: String,
    /// The payload for this step. May contain `{{step_id.field}}`
    /// placeholders.
    #[serde(default)]
    pub payload: Value,
    /// Top-level selectors (model, provider, etc.) for this step.
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
}

/// Result of executing a flow.
#[derive(Debug, Serialize)]
pub struct FlowResult {
    pub job_id: String,
    pub status: FlowStatus,
    pub steps: Vec<StepResult>,
}

#[derive(Debug, Serialize)]
pub struct StepResult {
    pub id: String,
    pub action: String,
    pub status: FlowStatus,
    pub result: Option<Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FlowStatus {
    Completed,
    Failed,
}

/// Errors from flow parsing.
#[derive(Debug, thiserror::Error)]
pub enum FlowError {
    #[error("flow requires at least one step in `actions` array")]
    Empty,
    #[error("duplicate step id `{0}`")]
    DuplicateStepId(String),
    #[error("step `{step_id}` has invalid action `{action}`: {reason}")]
    InvalidAction {
        step_id: String,
        action: String,
        reason: String,
    },
    #[error("step `{step_id}` references unknown step `{referenced}` in placeholder")]
    UnknownReference {
        step_id: String,
        referenced: String,
    },
    #[error("step `{step_id}` references step `{referenced}` which comes later in the flow (forward reference)")]
    ForwardReference {
        step_id: String,
        referenced: String,
    },
}

/// Parse the `actions` array from a `/v1/do` flow body.
pub fn parse_flow(body: &Value) -> Result<FlowDefinition, FlowError> {
    let actions = body
        .get("actions")
        .and_then(|v| v.as_array())
        .ok_or(FlowError::Empty)?;

    if actions.is_empty() {
        return Err(FlowError::Empty);
    }

    let mut steps = Vec::with_capacity(actions.len());
    let mut seen_ids = std::collections::HashSet::new();

    for (idx, action_val) in actions.iter().enumerate() {
        let id = action_val
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| "")
            .to_string();

        if id.is_empty() {
            return Err(FlowError::InvalidAction {
                step_id: format!("step_{idx}"),
                action: String::new(),
                reason: "each step requires an `id` field".into(),
            });
        }

        if !seen_ids.insert(id.clone()) {
            return Err(FlowError::DuplicateStepId(id));
        }

        let action_str = action_val
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Validate the action parses
        if let Err(e) = Action::parse_dotted(action_str) {
            return Err(FlowError::InvalidAction {
                step_id: id,
                action: action_str.to_string(),
                reason: e.to_string(),
            });
        }

        let payload = action_val.get("payload").cloned().unwrap_or(json!({}));

        steps.push(FlowStep {
            id,
            action: action_str.to_string(),
            payload,
            model: action_val.get("model").and_then(|v| v.as_str()).map(String::from),
            provider: action_val.get("provider").and_then(|v| v.as_str()).map(String::from),
        });
    }

    // Validate inter-step references: only backward references allowed.
    for (idx, step) in steps.iter().enumerate() {
        let refs = extract_placeholders(&step.payload);
        for (ref_step, _ref_field) in &refs {
            if !seen_ids.contains(ref_step.as_str()) {
                return Err(FlowError::UnknownReference {
                    step_id: step.id.clone(),
                    referenced: ref_step.clone(),
                });
            }
            // Check it's not a forward reference
            let ref_idx = steps.iter().position(|s| s.id == *ref_step);
            if let Some(ri) = ref_idx {
                if ri >= idx {
                    return Err(FlowError::ForwardReference {
                        step_id: step.id.clone(),
                        referenced: ref_step.clone(),
                    });
                }
            }
        }
    }

    Ok(FlowDefinition { steps })
}

/// Extract `{{step_id.field.path}}` placeholders from a JSON value.
/// Returns `(step_id, field_path)` pairs.
fn extract_placeholders(value: &Value) -> Vec<(String, String)> {
    let mut result = Vec::new();
    collect_placeholders(value, &mut result);
    result
}

fn collect_placeholders(value: &Value, out: &mut Vec<(String, String)>) {
    match value {
        Value::String(s) => {
            // Find all {{step_id.field.path}} patterns
            let mut start = 0;
            while let Some(open) = s[start..].find("{{") {
                let abs_open = start + open + 2;
                if let Some(close) = s[abs_open..].find("}}") {
                    let abs_close = abs_open + close;
                    let reference = &s[abs_open..abs_close];
                    if let Some(dot) = reference.find('.') {
                        let step_id = reference[..dot].to_string();
                        let field = reference[dot + 1..].to_string();
                        out.push((step_id, field));
                    }
                    start = abs_close + 2;
                } else {
                    break;
                }
            }
        }
        Value::Array(arr) => {
            for v in arr {
                collect_placeholders(v, out);
            }
        }
        Value::Object(obj) => {
            for (_, v) in obj {
                collect_placeholders(v, out);
            }
        }
        _ => {}
    }
}

/// Resolve placeholders in a step's payload using completed step results.
fn resolve_payload(
    payload: &Value,
    completed: &HashMap<String, Value>,
) -> Value {
    match payload {
        Value::String(s) => {
            let mut result = s.clone();
            // Replace all {{step_id.field.path}} with actual values.
            // Walk in reverse to handle overlapping replacements correctly.
            let placeholders = extract_placeholders(payload);
            for (step_id, field_path) in placeholders.iter().rev() {
                let placeholder = format!("{{{{{step_id}.{field_path}}}}}");
                if let Some(step_result) = completed.get(step_id) {
                    // Navigate the dotted field path into the result
                    let pointer = format!("/{}", field_path.replace('.', "/"));
                    let replacement = step_result
                        .pointer(&pointer)
                        .map(|v| match v {
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                        .unwrap_or_default();
                    result = result.replace(&placeholder, &replacement);
                }
            }
            Value::String(result)
        }
        Value::Array(arr) => {
            Value::Array(arr.iter().map(|v| resolve_payload(v, completed)).collect())
        }
        Value::Object(obj) => {
            Value::Object(
                obj.iter()
                    .map(|(k, v)| (k.clone(), resolve_payload(v, completed)))
                    .collect(),
            )
        }
        other => other.clone(),
    }
}

/// Execute a flow against the dispatcher.
///
/// Runs each step in sequence. On success, the result is stored and
/// available for placeholder resolution in subsequent steps. On
/// failure of any step, the flow terminates and remaining steps are
/// not executed.
pub async fn execute_flow(
    flow: FlowDefinition,
    dispatcher: Arc<Dispatcher>,
    events: Arc<EventBus>,
    job_id: String,
    correlation_id: CorrelationId,
) -> FlowResult {
    let mut completed_outputs: HashMap<String, Value> = HashMap::new();
    let mut step_results: Vec<StepResult> = Vec::new();

    events.publish(
        &format!("jobs.{job_id}.state"),
        &json!({"state": "running", "steps": flow.steps.len()}),
    );

    for step in &flow.steps {
        events.publish(
            &format!("jobs.{job_id}.step.{}.state", step.id),
            &json!({"state": "running", "action": step.action}),
        );

        // Resolve placeholders from completed upstream steps.
        let resolved_payload = resolve_payload(&step.payload, &completed_outputs);

        // Build the raw request for this step.
        let action = match Action::parse_dotted(&step.action) {
            Ok(a) => a,
            Err(e) => {
                let err_msg = format!("invalid action: {e}");
                events.publish(
                    &format!("jobs.{job_id}.step.{}.state", step.id),
                    &json!({"state": "failed", "error": err_msg}),
                );
                step_results.push(StepResult {
                    id: step.id.clone(),
                    action: step.action.clone(),
                    status: FlowStatus::Failed,
                    result: None,
                    error: Some(err_msg),
                });
                break;
            }
        };

        let request_id = RequestId::generate();
        let cancel = tokio_util::sync::CancellationToken::new();
        let span = tracing::info_span!(
            "flow_step",
            job_id = %job_id,
            step_id = %step.id,
            action = %step.action,
        );

        let mut selectors = crate::domain::selectors::Selectors::default();
        if let Some(m) = &step.model {
            selectors.model = Some(m.clone());
        }
        if let Some(p) = &step.provider {
            selectors.provider = Some(crate::domain::ids::ProviderName::new(p));
        }

        let raw = crate::domain::request::RawRequest {
            id: request_id.clone(),
            correlation_id: correlation_id.clone(),
            received_at: chrono::Utc::now(),
            action,
            payload: resolved_payload.clone(),
            selectors,
            constraints: crate::domain::selectors::Constraints {
                zone: crate::domain::selectors::ZoneConstraint::Any,
                execution: None,
                idempotency_key: None,
            },
            cancel,
            span,
        };

        match dispatcher.dispatch(raw).await {
            Ok(crate::services::dispatcher::DispatchResult::Fresh(outcome, _req)) => {
                match outcome {
                    crate::domain::provider::ProviderOutcome::Sync(output) => {
                        let output_value = serde_json::to_value(&output).unwrap_or(json!({}));
                        completed_outputs.insert(step.id.clone(), output_value.clone());

                        events.publish(
                            &format!("jobs.{job_id}.step.{}.state", step.id),
                            &json!({"state": "completed"}),
                        );
                        events.publish(
                            &format!("jobs.{job_id}.step.{}.result", step.id),
                            &output_value,
                        );

                        step_results.push(StepResult {
                            id: step.id.clone(),
                            action: step.action.clone(),
                            status: FlowStatus::Completed,
                            result: Some(output_value),
                            error: None,
                        });
                    }
                    // Async and streaming outcomes are not yet supported in flows.
                    // A future commit can add support for waiting on async jobs
                    // and collecting streaming results.
                    other => {
                        let err_msg = format!(
                            "flow steps only support sync outcomes for now; got {:?}",
                            std::mem::discriminant(&other)
                        );
                        events.publish(
                            &format!("jobs.{job_id}.step.{}.state", step.id),
                            &json!({"state": "failed", "error": err_msg}),
                        );
                        step_results.push(StepResult {
                            id: step.id.clone(),
                            action: step.action.clone(),
                            status: FlowStatus::Failed,
                            result: None,
                            error: Some(err_msg),
                        });
                        break;
                    }
                }
            }
            Ok(crate::services::dispatcher::DispatchResult::Cached(record, _req)) => {
                let output_value = match &record.response {
                    crate::domain::idempotency::CachedResponse::Sync { output } => {
                        serde_json::to_value(output).unwrap_or(json!({}))
                    }
                    crate::domain::idempotency::CachedResponse::AsyncJob { job_id: jid } => {
                        json!({"job_id": jid.as_str()})
                    }
                };
                completed_outputs.insert(step.id.clone(), output_value.clone());
                events.publish(
                    &format!("jobs.{job_id}.step.{}.state", step.id),
                    &json!({"state": "completed", "cached": true}),
                );
                step_results.push(StepResult {
                    id: step.id.clone(),
                    action: step.action.clone(),
                    status: FlowStatus::Completed,
                    result: Some(output_value),
                    error: None,
                });
            }
            Err(err) => {
                let err_msg = format!("{err}");
                events.publish(
                    &format!("jobs.{job_id}.step.{}.state", step.id),
                    &json!({"state": "failed", "error": err_msg}),
                );
                step_results.push(StepResult {
                    id: step.id.clone(),
                    action: step.action.clone(),
                    status: FlowStatus::Failed,
                    result: None,
                    error: Some(err_msg),
                });
                break;
            }
        }
    }

    let all_completed = step_results.iter().all(|s| s.status == FlowStatus::Completed)
        && step_results.len() == flow.steps.len();
    let status = if all_completed {
        FlowStatus::Completed
    } else {
        FlowStatus::Failed
    };

    events.publish(
        &format!("jobs.{job_id}.state"),
        &json!({"state": if all_completed { "completed" } else { "failed" }}),
    );

    if all_completed {
        let aggregated: Value = json!(step_results
            .iter()
            .filter_map(|s| s.result.as_ref().map(|r| (s.id.clone(), r.clone())))
            .collect::<serde_json::Map<String, Value>>());
        events.publish(&format!("jobs.{job_id}.result"), &aggregated);
    }

    FlowResult {
        job_id,
        status,
        steps: step_results,
    }
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_flow_single_step() {
        let body = json!({
            "actions": [{
                "id": "step1",
                "action": "text.chat",
                "payload": { "text.prompt.user": "hello" }
            }]
        });
        let flow = parse_flow(&body).unwrap();
        assert_eq!(flow.steps.len(), 1);
        assert_eq!(flow.steps[0].id, "step1");
        assert_eq!(flow.steps[0].action, "text.chat");
    }

    #[test]
    fn parse_flow_two_steps_with_reference() {
        let body = json!({
            "actions": [
                {
                    "id": "transcribe",
                    "action": "audio.transcribe",
                    "payload": {}
                },
                {
                    "id": "summarize",
                    "action": "text.chat",
                    "payload": {
                        "text.prompt.user": "Summarize:\n{{transcribe.text.response}}"
                    }
                }
            ]
        });
        let flow = parse_flow(&body).unwrap();
        assert_eq!(flow.steps.len(), 2);
    }

    #[test]
    fn parse_flow_rejects_forward_reference() {
        let body = json!({
            "actions": [
                {
                    "id": "step1",
                    "action": "text.chat",
                    "payload": {
                        "text.prompt.user": "{{step2.text.response}}"
                    }
                },
                {
                    "id": "step2",
                    "action": "text.chat",
                    "payload": {}
                }
            ]
        });
        let err = parse_flow(&body).unwrap_err();
        assert!(matches!(err, FlowError::ForwardReference { .. }));
    }

    #[test]
    fn parse_flow_rejects_unknown_reference() {
        let body = json!({
            "actions": [
                {
                    "id": "step1",
                    "action": "text.chat",
                    "payload": {
                        "text.prompt.user": "{{nonexistent.field}}"
                    }
                }
            ]
        });
        let err = parse_flow(&body).unwrap_err();
        assert!(matches!(err, FlowError::UnknownReference { .. }));
    }

    #[test]
    fn parse_flow_rejects_duplicate_ids() {
        let body = json!({
            "actions": [
                { "id": "dup", "action": "text.chat", "payload": {} },
                { "id": "dup", "action": "text.embed", "payload": {} }
            ]
        });
        let err = parse_flow(&body).unwrap_err();
        assert!(matches!(err, FlowError::DuplicateStepId(_)));
    }

    #[test]
    fn parse_flow_rejects_empty() {
        let body = json!({ "actions": [] });
        let err = parse_flow(&body).unwrap_err();
        assert!(matches!(err, FlowError::Empty));
    }

    #[test]
    fn parse_flow_rejects_invalid_action() {
        let body = json!({
            "actions": [{ "id": "s", "action": "not.a.primitive", "payload": {} }]
        });
        let err = parse_flow(&body).unwrap_err();
        assert!(matches!(err, FlowError::InvalidAction { .. }));
    }

    #[test]
    fn extract_placeholders_finds_references() {
        let val = json!({
            "text.prompt.user": "Summarize:\n{{transcribe.text.response}}\nAlso: {{step2.output.field}}"
        });
        let refs = extract_placeholders(&val);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0], ("transcribe".into(), "text.response".into()));
        assert_eq!(refs[1], ("step2".into(), "output.field".into()));
    }

    #[test]
    fn extract_placeholders_handles_nested() {
        let val = json!({
            "outer": {
                "inner": "prefix {{step1.a.b}} suffix"
            },
            "list": ["{{step2.x}}", "no-ref"]
        });
        let refs = extract_placeholders(&val);
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn resolve_payload_substitutes_values() {
        let payload = json!({
            "text.prompt.user": "Summary of: {{transcribe.text.response}}"
        });
        let mut completed = HashMap::new();
        completed.insert(
            "transcribe".to_string(),
            json!({ "text": { "response": "Hello world transcription" } }),
        );
        let resolved = resolve_payload(&payload, &completed);
        assert_eq!(
            resolved["text.prompt.user"],
            "Summary of: Hello world transcription"
        );
    }

    #[test]
    fn resolve_payload_handles_missing_reference() {
        let payload = json!({ "x": "{{missing.field}}" });
        let completed = HashMap::new();
        let resolved = resolve_payload(&payload, &completed);
        // Missing reference resolves to empty string
        assert_eq!(resolved["x"], "{{missing.field}}");
    }
}
