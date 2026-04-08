//! Ingress handlers: `POST /v1/do` and `POST /v1/{modality}/{leaf}[/{skill}]`.
//!
//! Both paths construct the same `OrchestratorRequest` and hand it to
//! the shared dispatcher.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response, Sse};
use axum::Json;
use chrono::Utc;
use futures_util::StreamExt;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::app_state::AppState;
use crate::domain::errors::ErrorCode;
use crate::domain::ids::{CorrelationId, ProviderName, RequestId};
use crate::domain::idempotency::CachedResponse;
use crate::domain::jobs::JobSink;
use crate::domain::keys;
use crate::domain::moniker::Moniker;
use crate::domain::output::Output;
use crate::domain::primitive::Primitive;
use crate::domain::provider::ProviderOutcome;
use crate::domain::request::{Action, OrchestratorRequest};
use crate::domain::selectors::{Constraints, Selectors};
use crate::services::dispatcher::DispatchResult;

use super::envelopes::{Meta, SuccessEnvelope};
use super::errors::quick_error_response;

// ── Handlers ──────────────────────────────────────────────────

/// `POST /v1/do` — the universal dispatcher.
pub async fn post_do(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let body_value: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return quick_error_response(
                ErrorCode::ValidationFailed,
                format!("Request body is not valid JSON: {e}"),
            );
        }
    };

    let action_dotted = match body_value.get("action").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return quick_error_response(
                ErrorCode::ValidationFailed,
                "`action` field is required in POST /v1/do body.",
            );
        }
    };

    let action = match Action::parse_dotted(&action_dotted) {
        Ok(a) => a,
        Err(e) => {
            return quick_error_response(
                ErrorCode::ValidationFailed,
                format!("Invalid action `{action_dotted}`: {e}"),
            );
        }
    };

    execute(state, action, body_value, headers).await
}

/// `POST /v1/{modality}/{leaf}` — hierarchical sugar.
pub async fn post_primitive(
    State(state): State<AppState>,
    Path((modality, leaf)): Path<(String, String)>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let primitive = match Primitive::from_segments(&modality, &leaf) {
        Ok(p) => p,
        Err(_) => {
            return quick_error_response(
                ErrorCode::NotFound,
                format!("Unknown primitive `{modality}.{leaf}`."),
            );
        }
    };
    let action = Action::bare(primitive);
    let body_value = parse_body(&body);
    let body_value = match body_value {
        Ok(v) => v,
        Err(e) => return e,
    };
    execute(state, action, body_value, headers).await
}

/// `POST /v1/{modality}/{leaf}/{skill}` — skill-scoped hierarchical sugar.
pub async fn post_skill(
    State(state): State<AppState>,
    Path((modality, leaf, skill)): Path<(String, String, String)>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let primitive = match Primitive::from_segments(&modality, &leaf) {
        Ok(p) => p,
        Err(_) => {
            return quick_error_response(
                ErrorCode::NotFound,
                format!("Unknown primitive `{modality}.{leaf}`."),
            );
        }
    };
    let moniker = match Moniker::new(skill.clone()) {
        Ok(m) => m,
        Err(e) => {
            return quick_error_response(
                ErrorCode::ValidationFailed,
                format!("Invalid skill moniker `{skill}`: {e}"),
            );
        }
    };
    let action = Action::skill(primitive, moniker);
    let body_value = parse_body(&body);
    let body_value = match body_value {
        Ok(v) => v,
        Err(e) => return e,
    };
    execute(state, action, body_value, headers).await
}

/// `OPTIONS /v1/do` — preflight.
pub async fn options_do() -> Response {
    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header("allow", "POST, OPTIONS")
        .body(Body::empty())
        .expect("static headers")
}

fn parse_body(body: &axum::body::Bytes) -> Result<Value, Response> {
    if body.is_empty() {
        return Ok(Value::Object(Default::default()));
    }
    serde_json::from_slice(body).map_err(|e| {
        quick_error_response(
            ErrorCode::ValidationFailed,
            format!("Request body is not valid JSON: {e}"),
        )
    })
}

async fn execute(
    state: AppState,
    action: Action,
    body: Value,
    headers: HeaderMap,
) -> Response {
    let received_at = Utc::now();
    let correlation_id = correlation_from_headers(&headers);
    let request_id = RequestId::generate();

    // Extract top-level selectors from the body.
    let mut selectors = Selectors::default();
    if let Some(obj) = body.as_object() {
        if let Some(p) = obj.get("provider").and_then(|v| v.as_str()) {
            selectors.provider = Some(ProviderName::new(p));
        }
        if let Some(m) = obj.get("model").and_then(|v| v.as_str()) {
            selectors.model = Some(m.to_string());
        }
        if let Some(s) = obj.get("skill").and_then(|v| v.as_str()) {
            if let Ok(moniker) = Moniker::new(s) {
                selectors.skill = Some(moniker);
            }
        }
        // Skill-meta variant selector (ORCH-0029): picks among the
        // skill's declared workflow variants.
        if let Some(v) = obj.get("variant").and_then(|x| x.as_str()) {
            selectors.variant = Some(v.to_string());
        }
    }

    let idempotency_key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let constraints = Constraints {
        zone: crate::domain::selectors::ZoneConstraint::Any,
        execution: None,
        idempotency_key,
    };

    let cancel = CancellationToken::new();
    let span = tracing::info_span!(
        "request",
        action = action.dotted(),
        correlation_id = correlation_id.as_str(),
        request_id = request_id.as_str(),
    );

    let raw = crate::domain::request::RawRequest {
        id: request_id.clone(),
        correlation_id: correlation_id.clone(),
        received_at,
        action: action.clone(),
        payload: body,
        selectors,
        constraints,
        cancel,
        span,
    };

    let dispatcher = state.dispatcher.clone();
    match dispatcher.dispatch(raw).await {
        Ok(DispatchResult::Fresh(outcome, completed_request)) => {
            deliver_fresh(outcome, completed_request).await
        }
        Ok(DispatchResult::Cached(record, completed_request)) => {
            deliver_cached(record, completed_request, received_at).await
        }
        Err(err) => {
            let meta = build_meta(
                &correlation_id,
                &request_id,
                action.dotted(),
                None,
                None,
                "sync",
                received_at,
            );
            super::errors::error_response(err, meta)
        }
    }
}

async fn deliver_fresh(
    outcome: ProviderOutcome,
    request: OrchestratorRequest,
) -> Response {
    let received_at = request.received_at;
    let action_dotted = request.action.dotted();
    // The job-sink bound to the request is the dispatcher's handle;
    // terminal transitions are owned by the dispatcher + provider, so
    // handlers never touch the job store directly here.
    let job_id = request.context.job_sink.job_id().clone();
    match outcome {
        ProviderOutcome::Sync(output) => {
            let meta = build_meta(
                &request.correlation_id,
                &request.id,
                action_dotted,
                request.resolved_provider.as_ref(),
                request.resolved_model.as_ref().map(|m| m.short_name.clone()),
                "sync",
                received_at,
            );
            let envelope = SuccessEnvelope::from_output(&output, meta);
            (StatusCode::OK, Json(envelope)).into_response()
        }
        ProviderOutcome::Async(output) => {
            let meta = build_meta(
                &request.correlation_id,
                &request.id,
                action_dotted,
                request.resolved_provider.as_ref(),
                request.resolved_model.as_ref().map(|m| m.short_name.clone()),
                "async",
                received_at,
            );
            let envelope = SuccessEnvelope::from_output(&output, meta);
            (StatusCode::ACCEPTED, Json(envelope)).into_response()
        }
        ProviderOutcome::Streaming { initial, stream } => {
            let meta = build_meta(
                &request.correlation_id,
                &request.id,
                action_dotted.clone(),
                request.resolved_provider.as_ref(),
                request.resolved_model.as_ref().map(|m| m.short_name.clone()),
                "stream",
                received_at,
            );
            let initial_payload = json!({
                "output": initial.to_nested(),
                "_meta": meta,
            });
            // The streaming adapter needs a JobSink handle to mark
            // the job terminal when the stream closes. Dispatcher has
            // already transitioned the job to Running and reserved
            // any referenced media.
            let sse_stream = async_stream_initial_then_deltas(
                initial_payload,
                stream,
                action_dotted,
                job_id,
                request.context.job_sink.clone(),
            );
            Sse::new(sse_stream).into_response()
        }
    }
}

async fn deliver_cached(
    record: crate::domain::idempotency::IdempotencyRecord,
    request: OrchestratorRequest,
    received_at: chrono::DateTime<chrono::Utc>,
) -> Response {
    let meta = build_meta(
        &request.correlation_id,
        &request.id,
        request.action.dotted(),
        request.resolved_provider.as_ref(),
        request.resolved_model.as_ref().map(|m| m.short_name.clone()),
        "sync",
        received_at,
    )
    .mark_idempotent();
    match record.response {
        CachedResponse::Sync { output } => {
            let envelope = SuccessEnvelope::from_output(&output, meta);
            (StatusCode::OK, Json(envelope)).into_response()
        }
        CachedResponse::AsyncJob { job_id } => {
            let mut output = Output::new();
            output.set(&keys::job::ID, job_id.as_str());
            output.set(&keys::job::STATUS, keys::job::values::STATUS_RUNNING);
            let envelope = SuccessEnvelope::from_output(&output, meta);
            (StatusCode::ACCEPTED, Json(envelope)).into_response()
        }
    }
}

fn build_meta(
    correlation_id: &CorrelationId,
    request_id: &RequestId,
    action: String,
    provider: Option<&ProviderName>,
    model: Option<String>,
    mode: &'static str,
    received_at: chrono::DateTime<chrono::Utc>,
) -> Meta {
    Meta::build(
        correlation_id,
        request_id,
        action,
        provider,
        model,
        mode,
        received_at,
        None,
    )
}

/// Extract or synthesize a correlation id.
pub fn correlation_from_headers(headers: &HeaderMap) -> CorrelationId {
    if let Some(v) = headers.get("x-correlation-id") {
        if let Ok(s) = v.to_str() {
            return CorrelationId::from_string(s);
        }
    }
    if let Some(tp) = headers.get("traceparent") {
        if let Ok(s) = tp.to_str() {
            let parts: Vec<&str> = s.split('-').collect();
            if parts.len() >= 2 {
                return CorrelationId::from_string(parts[1]);
            }
        }
    }
    CorrelationId::generate()
}

/// Build an SSE stream that yields the initial envelope first, then
/// per-chunk deltas, then a final `done` event. Errors close the
/// stream with an `error` event. Terminal transitions go through the
/// `JobSink`, which the dispatcher already bound to this request.
fn async_stream_initial_then_deltas(
    initial: Value,
    mut stream: futures_util::stream::BoxStream<
        'static,
        Result<Output, crate::domain::provider::ProviderError>,
    >,
    action_dotted: String,
    _job_id: crate::domain::ids::JobId,
    job_sink: Arc<JobSink>,
) -> futures_util::stream::BoxStream<'static, Result<axum::response::sse::Event, std::convert::Infallible>>
{
    use axum::response::sse::Event;

    let s = async_stream::stream! {
        yield Ok(Event::default().event("initial").json_data(&initial).unwrap_or(Event::default().data("{}")));

        let mut aggregate = Output::new();
        while let Some(item) = stream.next().await {
            match item {
                Ok(delta) => {
                    let nested = delta.to_nested();
                    aggregate.merge(delta);
                    yield Ok(Event::default().event("delta").json_data(&nested).unwrap_or(Event::default().data("{}")));
                }
                Err(e) => {
                    let body = json!({
                        "error": {
                            "code": e.code().as_str(),
                            "message": e.message(),
                        },
                        "action": action_dotted,
                    });
                    yield Ok(Event::default().event("error").json_data(&body).unwrap_or(Event::default().data("{}")));
                    let _ = job_sink.fail(e).await;
                    return;
                }
            }
        }

        let done_body = json!({"status": "done", "action": action_dotted});
        yield Ok(Event::default().event("done").json_data(&done_body).unwrap_or(Event::default().data("{}")));
        let _ = job_sink.complete(aggregate).await;
    };
    Box::pin(s)
}

