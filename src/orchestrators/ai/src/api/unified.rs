//! Unified API handlers — `/v1/...` on port 7190.
//!
//! Our stable, OpenAI-shaped spec. Version-locked to us — if OpenAI changes
//! their API, only the OpenAI adapter changes, not our clients.
//!
//! Each handler: parse canonical request → resolve model → route → dispatch
//! to the correct adapter → serialize canonical response.

use axum::body::Body;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use bytes::Bytes;
use std::sync::atomic::Ordering;

use crate::app_state::AppState;
use crate::catalog::inference::*;
use crate::domain::routing;
use crate::domain::types::{Capability, RoutingDecision, RoutingError};

// ── Chat Completions ────────────────────────────────────────────

/// `POST /v1/chat/completions`
pub async fn chat_completions(
    State(state): State<AppState>,
    Json(mut req): Json<InferenceRequest>,
) -> Response {
    // Resolve model name (capability name, MFQN, recommended:, or plain)
    let (resolved, header) = resolve_model(&req.model, &state).await;
    req.model = resolved.clone();

    // Infer capability from request
    let capability = infer_chat_capability(&req);

    // Route
    let decision = match route_model(&resolved, &state, Some(capability)).await {
        Ok(d) => d,
        Err(resp) => return resp,
    };

    // Look up provider
    let adapter = match state.providers.get(decision.offering_kind).cloned() {
        Some(a) => a,
        None => {
            tracing::warn!(
                kind = %decision.offering_kind,
                "no provider registered"
            );
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                &format!("No inference adapter registered for {}", decision.offering_kind),
            );
        }
    };

    // Build adapter context
    let ctx = build_context(&decision, &state).await;

    // Queue depth management
    let counter = state.queue_counter(&decision.target_endpoint).await;
    counter.fetch_add(1, Ordering::Relaxed);

    if req.stream {
        // ── Streaming ───────────────────────────────────────────
        let result = adapter.infer_stream(&ctx, req).await;

        match result {
            Ok(chunk_stream) => {
                // Convert BoxStream<Result<InferenceChunk>> → SSE byte stream
                use futures_util::StreamExt;

                let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(64);

                tokio::spawn(async move {
                    futures_util::pin_mut!(chunk_stream);
                    loop {
                        tokio::select! {
                            chunk_opt = chunk_stream.next() => {
                                match chunk_opt {
                                    Some(Ok(chunk)) => {
                                        let json = serde_json::to_string(&chunk).unwrap_or_default();
                                        let sse = format!("data: {json}\n\n");
                                        if tx.send(Ok(Bytes::from(sse))).await.is_err() {
                                            break; // client disconnected
                                        }
                                    }
                                    Some(Err(e)) => {
                                        tracing::warn!(error = %e, "adapter stream error");
                                        let _ = tx.send(Err(std::io::Error::other(e))).await;
                                        break;
                                    }
                                    None => break, // stream ended
                                }
                            }
                            _ = tx.closed() => {
                                tracing::debug!("client disconnected from SSE stream");
                                break;
                            }
                        }
                    }
                    // Send [DONE] sentinel
                    let _ = tx.send(Ok(Bytes::from("data: [DONE]\n\n"))).await;
                    counter.fetch_sub(1, Ordering::Relaxed);
                });

                let body_stream = tokio_stream::wrappers::ReceiverStream::new(rx);

                let mut builder = Response::builder()
                    .status(StatusCode::OK)
                    .header("content-type", "text/event-stream")
                    .header("cache-control", "no-cache");
                if let Some(ref h) = header {
                    builder = builder.header("x-zen-resolved-model", h.as_str());
                }
                builder
                    .body(Body::from_stream(body_stream))
                    .expect("constant headers")
            }
            Err(e) => {
                counter.fetch_sub(1, Ordering::Relaxed);
                error_response(StatusCode::BAD_GATEWAY, &e.to_string())
            }
        }
    } else {
        // ── Non-streaming ───────────────────────────────────────
        let result = adapter.infer(&ctx, req).await;
        counter.fetch_sub(1, Ordering::Relaxed);

        match result {
            Ok(resp) => {
                let mut builder = Response::builder()
                    .status(StatusCode::OK)
                    .header("content-type", "application/json");
                if let Some(ref h) = header {
                    builder = builder.header("x-zen-resolved-model", h.as_str());
                }
                let body = serde_json::to_vec(&resp).unwrap_or_default();
                builder.body(Body::from(body)).expect("constant headers")
            }
            Err(e) => error_response(StatusCode::BAD_GATEWAY, &e.to_string()),
        }
    }
}

// ── Embeddings ──────────────────────────────────────────────────

/// `POST /v1/embeddings`
pub async fn embeddings(
    State(state): State<AppState>,
    Json(mut req): Json<EmbedRequest>,
) -> Response {
    let (resolved, _) = resolve_model(&req.model, &state).await;
    req.model = resolved.clone();

    let decision = match route_model(&resolved, &state, Some(Capability::Embed)).await {
        Ok(d) => d,
        Err(resp) => return resp,
    };

    let adapter = match state.providers.get(decision.offering_kind).cloned() {
        Some(a) => a,
        None => {
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                &format!("No inference adapter registered for {}", decision.offering_kind),
            );
        }
    };

    let ctx = build_context(&decision, &state).await;

    match adapter.embed(&ctx, req).await {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => error_response(StatusCode::BAD_GATEWAY, &e.to_string()),
    }
}

// ── Audio Speech (TTS) ──────────────────────────────────────────

/// `POST /v1/audio/speech`
pub async fn speech(
    State(state): State<AppState>,
    Json(mut req): Json<SpeechRequest>,
) -> Response {
    let (resolved, _) = resolve_model(&req.model, &state).await;
    req.model = resolved.clone();

    let decision = match route_model(&resolved, &state, Some(Capability::Speech)).await {
        Ok(d) => d,
        Err(resp) => return resp,
    };

    let adapter = match state.providers.get(decision.offering_kind).cloned() {
        Some(a) => a,
        None => {
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                &format!("No inference adapter registered for {}", decision.offering_kind),
            );
        }
    };

    let ctx = build_context(&decision, &state).await;

    match adapter.speak(&ctx, req).await {
        Ok(resp) => {
            let content_type = resp.content_type;
            match resp.audio {
                SpeechAudio::Complete(bytes) => Response::builder()
                    .status(StatusCode::OK)
                    .header("content-type", &content_type)
                    .body(Body::from(bytes))
                    .expect("constant headers"),
                SpeechAudio::Stream(stream) => {
                    use futures_util::TryStreamExt;
                    let mapped =
                        stream.map_err(|e| std::io::Error::other(e.to_string()));
                    Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", &content_type)
                        .header("transfer-encoding", "chunked")
                        .body(Body::from_stream(mapped))
                        .expect("constant headers")
                }
            }
        }
        Err(e) => error_response(StatusCode::BAD_GATEWAY, &e.to_string()),
    }
}

// ── Audio Transcriptions (STT) ──────────────────────────────────

/// `POST /v1/audio/transcriptions`
pub async fn transcriptions(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> Response {
    // Parse multipart form data
    let mut model = String::new();
    let mut audio = Vec::new();
    let mut filename = String::from("audio.wav");
    let mut language = None;
    let mut response_format = None;

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(_) => {
                return error_response(StatusCode::BAD_REQUEST, "Invalid multipart form data");
            }
        };

        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "model" => {
                model = match field.text().await {
                    Ok(t) => t,
                    Err(_) => {
                        return error_response(
                            StatusCode::BAD_REQUEST,
                            "Invalid multipart form data",
                        );
                    }
                };
            }
            "file" => {
                if let Some(fname) = field.file_name() {
                    filename = fname.to_string();
                }
                audio = match field.bytes().await {
                    Ok(b) => b.to_vec(),
                    Err(_) => {
                        return error_response(
                            StatusCode::BAD_REQUEST,
                            "Invalid multipart form data",
                        );
                    }
                };
            }
            "language" => {
                language = match field.text().await {
                    Ok(t) => Some(t),
                    Err(_) => {
                        return error_response(
                            StatusCode::BAD_REQUEST,
                            "Invalid multipart form data",
                        );
                    }
                };
            }
            "response_format" => {
                response_format = match field.text().await {
                    Ok(t) => Some(t),
                    Err(_) => {
                        return error_response(
                            StatusCode::BAD_REQUEST,
                            "Invalid multipart form data",
                        );
                    }
                };
            }
            _ => {} // skip unknown fields
        }
    }

    if model.is_empty() || audio.is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "model and file are required");
    }

    let (resolved, _) = resolve_model(&model, &state).await;

    let decision = match route_model(&resolved, &state, Some(Capability::Transcribe)).await {
        Ok(d) => d,
        Err(resp) => return resp,
    };

    let adapter = match state.providers.get(decision.offering_kind).cloned() {
        Some(a) => a,
        None => {
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                &format!("No inference adapter registered for {}", decision.offering_kind),
            );
        }
    };

    let ctx = build_context(&decision, &state).await;

    let req = TranscribeRequest {
        model: resolved,
        audio,
        filename,
        language,
        response_format,
    };

    match adapter.transcribe(&ctx, req).await {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => error_response(StatusCode::BAD_GATEWAY, &e.to_string()),
    }
}

// ── Models ──────────────────────────────────────────────────────

/// `GET /v1/models` — merged model list from the directory.
pub async fn models(State(state): State<AppState>) -> Json<serde_json::Value> {
    let dir = state.directory_legacy.read().await;

    let data: Vec<serde_json::Value> = dir
        .entries()
        .values()
        .map(|entry| {
            let source = entry
                .instances
                .first()
                .map(|fqn| fqn.source.as_str())
                .unwrap_or("unknown");

            serde_json::json!({
                "id": entry.model,
                "object": "model",
                "owned_by": source,
                "capabilities": entry.capabilities.iter().map(|c| c.as_str()).collect::<Vec<_>>(),
            })
        })
        .collect();

    Json(serde_json::json!({
        "object": "list",
        "data": data,
    }))
}

/// `GET /v1/models/{model}/form?capability={cap}` — form schema for a model.
///
/// Returns a JSON Schema + UI Schema that the dashboard renders via RJSF.
/// The provider decides what parameters to expose for this model+capability.
pub async fn model_form(
    State(state): State<AppState>,
    axum::extract::Path(model_name): axum::extract::Path<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let capability_str = params.get("capability").map(|s| s.as_str()).unwrap_or("chat");

    // Parse capability
    let capability = match Capability::ALL.iter().find(|c| c.as_str() == capability_str) {
        Some(c) => *c,
        None => {
            return error_response(
                StatusCode::BAD_REQUEST,
                &format!("Unknown capability: {capability_str}"),
            );
        }
    };

    // Find the model in the directory to determine its provider
    let source = {
        let dir = state.directory_legacy.read().await;
        let entries = dir.find_by_model_name(&model_name);
        entries
            .first()
            .and_then(|e| e.instances.first())
            .map(|fqn| fqn.source.clone())
            .unwrap_or_default()
    };

    // Look up the offering kind from the source string
    let kind = match crate::domain::types::OfferingKind::from_str(&source) {
        Some(k) => k,
        None => {
            return error_response(
                StatusCode::NOT_FOUND,
                &format!("No provider found for model '{model_name}'"),
            );
        }
    };

    // Get the form schema from the provider
    let provider = match state.providers.get(kind) {
        Some(p) => p,
        None => {
            return error_response(
                StatusCode::NOT_FOUND,
                &format!("Provider '{source}' not registered"),
            );
        }
    };

    let form = provider.form_schema(&model_name, capability);

    let body = serde_json::json!({
        "model": model_name,
        "provider": source,
        "capability": capability_str,
        "schema": form.schema,
        "uiSchema": form.ui_schema,
    });

    Json(body).into_response()
}

// ── Helpers ─────────────────────────────────────────────────────

/// Resolve a model name — reuses logic from proxy.rs.
async fn resolve_model(raw: &str, state: &AppState) -> (String, Option<String>) {
    crate::api::proxy::resolve_model_field(raw, state).await
}

/// Snapshot state and call `select_instance()`.
async fn route_model(
    model: &str,
    state: &AppState,
    capability: Option<Capability>,
) -> Result<RoutingDecision, Response> {
    let mut instances = state.instances.read().await.clone();
    let directory = state.directory_legacy.read().await.clone();
    let tiers = state.tiers.read().await.clone();
    let gpu_matrix = state.benchmark_run.read().await.gpu_matrix.clone();

    // Patch live queue depths
    {
        let depths = state.queue_depths.read().await;
        for (ep, counter) in depths.iter() {
            if let Some(inst) = instances.get_mut(ep) {
                inst.queue_depth = counter.load(Ordering::Relaxed);
            }
        }
    }

    let fitness_ref = if gpu_matrix.entries.is_empty() {
        None
    } else {
        Some(&gpu_matrix)
    };

    let recent_demand = state.metrics.read().await.demand_shares(300);

    routing::select_instance(
        model,
        &instances,
        &directory,
        &tiers,
        64,
        fitness_ref,
        &recent_demand,
        capability,
    )
    .map_err(|e| {
        tracing::warn!(model = %model, error = %e, "routing failed");
        let status = match &e {
            RoutingError::ModelNotFound(_) => StatusCode::NOT_FOUND,
            RoutingError::ModelBlocked(_) => StatusCode::CONFLICT,
            _ => StatusCode::SERVICE_UNAVAILABLE,
        };
        error_response(status, &e.to_string())
    })
}

/// Build `ProviderContext` from a routing decision.
async fn build_context(decision: &RoutingDecision, state: &AppState) -> crate::catalog::ProviderContext {
    let api_key = if decision.offering_kind.is_cloud() {
        let store = state.cloud_store.read().await;
        store
            .all()
            .iter()
            .find(|p| {
                p.base_url == decision.target_endpoint && p.kind == decision.offering_kind
            })
            .map(|p| p.api_key.clone())
    } else {
        None
    };

    crate::catalog::ProviderContext {
        endpoint: decision.target_endpoint.clone(),
        model: Some(decision.model_name.clone()),
        api_key,
    }
}

/// Infer the capability from a chat completion request.
fn infer_chat_capability(req: &InferenceRequest) -> Capability {
    if req.tools.as_ref().is_some_and(|t| !t.is_empty()) {
        Capability::Tools
    } else if req
        .messages
        .iter()
        .any(|m| has_image_content(&m.content))
    {
        Capability::Vision
    } else {
        Capability::Chat
    }
}

/// Check if content contains image parts.
fn has_image_content(content: &Option<serde_json::Value>) -> bool {
    content
        .as_ref()
        .and_then(|c| c.as_array())
        .is_some_and(|parts| {
            parts
                .iter()
                .any(|p| p.get("type").and_then(|t| t.as_str()) == Some("image_url"))
        })
}

/// Build a structured JSON error response.
///
/// Always returns the same envelope so the frontend can parse it uniformly:
/// ```json
/// { "error": { "code": "...", "message": "...", "status": 502 } }
/// ```
fn error_response(status: StatusCode, message: &str) -> Response {
    // Classify the error from the message content
    let (code, clean_message) = classify_error(status, message);

    let body = serde_json::json!({
        "error": {
            "code": code,
            "message": clean_message,
            "status": status.as_u16(),
        }
    });
    (status, Json(body)).into_response()
}

/// Extract a human-readable error code and clean message from a raw adapter error.
fn classify_error(status: StatusCode, raw: &str) -> (&'static str, String) {
    let lower = raw.to_lowercase();

    // Rate limiting
    if lower.contains("429")
        || lower.contains("rate limit")
        || lower.contains("quota exceeded")
        || lower.contains("too many requests")
        || lower.contains("resource_exhausted")
    {
        return (
            "rate_limited",
            extract_inner_message(raw)
                .unwrap_or_else(|| "Rate limited — try again in a few seconds.".to_string()),
        );
    }

    // High demand / overloaded
    if lower.contains("503")
        || lower.contains("high demand")
        || lower.contains("unavailable")
        || lower.contains("overloaded")
    {
        return (
            "provider_overloaded",
            extract_inner_message(raw).unwrap_or_else(|| {
                "This model is currently experiencing high demand. Try a different model or retry later.".to_string()
            }),
        );
    }

    // Auth failures
    if lower.contains("401") || lower.contains("unauthorized") || lower.contains("invalid.*key") {
        return (
            "auth_failed",
            "API key is invalid or expired. Check your cloud provider configuration.".to_string(),
        );
    }

    // Model not found
    if status == StatusCode::NOT_FOUND || lower.contains("not found") {
        return (
            "model_not_found",
            extract_inner_message(raw)
                .unwrap_or_else(|| "Model not found in any available provider.".to_string()),
        );
    }

    // Not supported
    if lower.contains("not supported") {
        return (
            "not_supported",
            extract_inner_message(raw)
                .unwrap_or_else(|| "This capability is not supported by the selected provider.".to_string()),
        );
    }

    // Fallback: try to extract inner message, else use raw (truncated)
    let message = extract_inner_message(raw).unwrap_or_else(|| {
        if raw.len() > 200 {
            format!("{}...", &raw[..200])
        } else {
            raw.to_string()
        }
    });

    ("upstream_error", message)
}

/// Try to extract a clean message from nested provider error JSON.
///
/// Handles patterns like:
/// - `"Gemini ... HTTP 503: {\"error\":{\"message\":\"...\"}}"` (Google)
/// - Raw JSON `{"error": {"message": "..."}}`
fn extract_inner_message(raw: &str) -> Option<String> {
    // Try to find JSON in the string and extract error.message
    if let Some(brace_start) = raw.find('{') {
        let json_part = &raw[brace_start..];
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_part) {
            if let Some(msg) = parsed
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
            {
                // Truncate very long messages
                let clean = if msg.len() > 300 {
                    format!("{}...", &msg[..300])
                } else {
                    msg.to_string()
                };
                return Some(clean);
            }
        }
    }

    None
}
