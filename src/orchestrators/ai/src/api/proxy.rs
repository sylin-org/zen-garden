//! Ollama-compatible proxy handler (port 21434).
//!
//! Accepts all Ollama client traffic, routes inference requests to the
//! optimal instance via `routing::select_instance()`, and merges discovery
//! endpoints (`/api/tags`, `/api/ps`) across all healthy instances.
//!
//! Block 3 scope: no moniker resolution, no extension API, no NDJSON
//! tee for streaming metrics. Streaming responses are forwarded as-is;
//! metrics are extracted from non-streaming responses only.

use crate::app_state::AppState;
use crate::domain::routing;
use crate::domain::types::{Capability, InferenceDefaults, InstanceHealth, MetricEvent, RoutingError};
use crate::offerings::ollama::client::OllamaClient;
use crate::offerings::ollama::types::OllamaInferenceFinal;

use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use std::sync::atomic::Ordering;

// ── Proxy State ────────────────────────────────────────────────

/// Shared state for the Ollama proxy server.
///
/// Wraps `AppState` with the Ollama-specific HTTP client.
/// Each offering's proxy port carries its own protocol-specific state.
#[derive(Clone)]
pub struct ProxyState {
    pub app: AppState,
    pub client: OllamaClient,
}

// ── Main Handler ───────────────────────────────────────────────

/// Catch-all proxy handler for all Ollama API paths on port 21434.
pub async fn proxy_handler(
    State(state): State<ProxyState>,
    req: Request,
) -> Result<Response, StatusCode> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let headers = req.headers().clone();

    // Read body — needed to extract model name for routing.
    // No hard size limit: Ollama inference bodies are small JSON,
    // but pull/create can stream. We cap at 50 MB for safety.
    let body_bytes = axum::body::to_bytes(req.into_body(), 50 * 1024 * 1024)
        .await
        .map_err(|_| StatusCode::PAYLOAD_TOO_LARGE)?;

    match (method.clone(), path.as_str()) {
        // ── Root health probe (clients expect "Ollama is running") ──
        (Method::GET, "/") | (Method::HEAD, "/") => {
            let body = if method == Method::HEAD {
                Body::empty()
            } else {
                Body::from("Ollama is running")
            };
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "text/plain; charset=utf-8")
                .body(body)
                .expect("constant headers"))
        }

        // ── Version (orchestrator's own) ──
        (Method::GET, "/api/version") => {
            let body = serde_json::json!({"version": env!("CARGO_PKG_VERSION")});
            Ok(axum::Json(body).into_response())
        }

        // ── Merged discovery endpoints ──
        (Method::GET, "/api/tags") => proxy_merged_tags(&state).await,
        (Method::GET, "/api/ps") => proxy_merged_ps(&state).await,

        // ── Inference endpoints (routed) ──
        (Method::POST, "/api/generate" | "/api/chat" | "/api/embed" | "/api/embeddings") => {
            proxy_inference(&state, &path, &headers, body_bytes).await
        }

        // ── Show (with catalog fallback) ──
        (Method::POST, "/api/show") => {
            proxy_show(&state, &path, method, &headers, body_bytes).await
        }

        // ── Model management (routed to any healthy instance) ──
        (Method::POST, "/api/pull")
        | (Method::DELETE, "/api/delete")
        | (Method::POST, "/api/copy")
        | (Method::POST, "/api/create") => {
            proxy_routed(&state, &path, method, &headers, body_bytes).await
        }

        // ── Blob endpoints ──
        _ if path.starts_with("/api/blobs/")
            && matches!(method, Method::HEAD | Method::POST) =>
        {
            proxy_routed(&state, &path, method, &headers, body_bytes).await
        }

        // ── Fallback ──
        _ => Err(StatusCode::NOT_FOUND),
    }
}

// ── Inference Proxy ────────────────────────────────────────────

/// Route an inference request to the best instance and forward the response.
///
/// 1. Extract model name from body JSON.
/// 2. Snapshot state, call `routing::select_instance()`.
/// 3. Increment queue depth, forward, decrement on completion.
/// 4. Non-streaming: extract metrics from response JSON.
/// 5. Streaming: forward NDJSON as-is (metrics extraction deferred to follow-up).
async fn proxy_inference(
    state: &ProxyState,
    path: &str,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let model = extract_model(&body).ok_or(StatusCode::BAD_REQUEST)?;

    let mut body_json: serde_json::Value =
        serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);

    let stream_disabled = body_json.get("stream").and_then(|v| v.as_bool()) == Some(false);

    // Infer capability from path
    let capability = capability_from_path(path, &body_json);

    // Merge per-capability inference defaults (only for fields the client didn't set).
    if let Some(cap) = capability {
        let config = state.app.config.read().await;
        if let Some(defaults) = config.defaults.get(cap.as_str()) {
            if let Some(obj) = body_json.as_object_mut() {
                merge_inference_defaults(obj, defaults);
            }
        }
    }

    // Route — snapshot state, no locks held during routing
    let decision = {
        let mut instances = state.app.instances.read().await.clone();
        let models = state.app.models.read().await.clone();
        let tiers = state.app.tiers.read().await.clone();
        let gpu_matrix = {
            let run = state.app.benchmark_run.read().await;
            run.gpu_matrix.clone()
        };

        // Patch live queue depths from atomics
        {
            let depths = state.app.queue_depths.read().await;
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

        let recent_demand = {
            let metrics = state.app.metrics.read().await;
            metrics.demand_shares(300)
        };

        routing::select_instance(
            &model,
            &instances,
            &models,
            &tiers,
            64,
            fitness_ref,
            &recent_demand,
            capability,
        )
    };

    let decision = match decision {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(model = %model, error = %e, "routing failed");
            let status = match &e {
                RoutingError::ModelNotFound(_) => StatusCode::NOT_FOUND,
                RoutingError::ModelBlocked(_) => StatusCode::CONFLICT,
                _ => StatusCode::SERVICE_UNAVAILABLE,
            };
            let body = serde_json::json!({"error": e.to_string()});
            return Ok((status, axum::Json(body)).into_response());
        }
    };

    let target = &decision.target_endpoint;
    let stone_name = decision.stone_name.clone();

    tracing::debug!(
        model = %model,
        target = %target,
        stone = %stone_name,
        tier = %decision.tier_label,
        overflow = decision.was_overflow,
        "routing request"
    );

    // Increment queue depth
    let counter = state.app.queue_counter(target).await;
    counter.fetch_add(1, Ordering::Relaxed);

    // Re-serialize body if defaults were merged (body_json may have been mutated).
    let forward_body = Bytes::from(
        serde_json::to_vec(&body_json).unwrap_or_else(|_| body.to_vec()),
    );

    // Forward to target instance
    let result = state
        .client
        .forward_request(target, path, Method::POST, forward_body, headers.clone())
        .await;

    let response = match result {
        Ok(r) => r,
        Err(e) => {
            counter.fetch_sub(1, Ordering::Relaxed);
            state
                .app
                .set_instance_health(
                    target,
                    InstanceHealth::Unhealthy {
                        since: std::time::Instant::now(),
                        reason: e.to_string(),
                    },
                )
                .await;
            let _ = state.app.metrics_tx.send(MetricEvent::Error {
                stone: stone_name,
                model: Some(model),
                status_code: None,
                reason: Some(format!("connection error: {e}")),
            });
            let body = serde_json::json!({"error": format!("upstream error: {e}")});
            return Ok((StatusCode::BAD_GATEWAY, axum::Json(body)).into_response());
        }
    };

    let status = response.status();

    // Propagate non-success upstream status
    if !status.is_success() {
        counter.fetch_sub(1, Ordering::Relaxed);
        let axum_status =
            StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let text = response.text().await.unwrap_or_default();
        let _ = state.app.metrics_tx.send(MetricEvent::Error {
            stone: stone_name,
            model: Some(model),
            status_code: Some(status.as_u16()),
            reason: if text.is_empty() {
                None
            } else {
                Some(text.clone())
            },
        });
        return Ok((axum_status, text).into_response());
    }

    if stream_disabled {
        // Non-streaming: buffer response, extract metrics
        let response_bytes = response
            .bytes()
            .await
            .map_err(|_| StatusCode::BAD_GATEWAY)?;
        counter.fetch_sub(1, Ordering::Relaxed);

        if let Ok(final_obj) = serde_json::from_slice::<OllamaInferenceFinal>(&response_bytes) {
            let _ = state.app.metrics_tx.send(MetricEvent::Request {
                stone: stone_name,
                model,
                capability: capability.unwrap_or(Capability::Chat),
                tokens_in: final_obj.prompt_eval_count,
                tokens_out: final_obj.eval_count,
                duration_ns: final_obj.total_duration,
                eval_duration_ns: final_obj.eval_duration,
            });
        }

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/json")
            .body(Body::from(response_bytes))
            .expect("constant headers"))
    } else {
        // Streaming: forward NDJSON stream as-is.
        // Block 3 simplification: no NDJSON tee for streaming metrics.
        // Queue depth is decremented when the stream ends.
        let stream = response.bytes_stream();

        let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(64);

        tokio::spawn(async move {
            use futures_util::StreamExt;
            futures_util::pin_mut!(stream);

            loop {
                tokio::select! {
                    chunk_opt = stream.next() => {
                        match chunk_opt {
                            Some(Ok(chunk)) => {
                                if tx.send(Ok(chunk)).await.is_err() {
                                    break; // client disconnected
                                }
                            }
                            Some(Err(e)) => {
                                tracing::warn!(error = %e, "upstream stream error");
                                let _ = tx.send(Err(std::io::Error::other(e))).await;
                                break;
                            }
                            None => break, // upstream finished
                        }
                    }
                    _ = tx.closed() => {
                        tracing::debug!("client disconnected, dropping upstream");
                        break;
                    }
                }
            }

            counter.fetch_sub(1, Ordering::Relaxed);
        });

        let body_stream = tokio_stream::wrappers::ReceiverStream::new(rx);

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/x-ndjson")
            .header("transfer-encoding", "chunked")
            .body(Body::from_stream(body_stream))
            .expect("constant headers"))
    }
}

// ── Merged Discovery ───────────────────────────────────────────

/// Merge `/api/tags` from all healthy instances into a unified response.
async fn proxy_merged_tags(state: &ProxyState) -> Result<Response, StatusCode> {
    let instances = state.app.instances.read().await;
    let models = state.app.models.read().await;

    let mut merged: std::collections::HashMap<String, serde_json::Value> =
        std::collections::HashMap::new();

    for inst in instances.values() {
        if !inst.is_routable() {
            continue;
        }
        for model_name in &inst.models_available {
            if merged.contains_key(model_name) {
                continue;
            }
            if let Some(info) = models.get(model_name) {
                merged.insert(
                    model_name.clone(),
                    serde_json::json!({
                        "name": info.name,
                        "model": info.name,
                        "size": info.size_disk,
                        "details": {
                            "family": info.family,
                            "families": info.families,
                            "parameter_size": info.parameter_size,
                            "quantization_level": info.quantization_level,
                            "format": info.format,
                        }
                    }),
                );
            } else {
                merged.insert(
                    model_name.clone(),
                    serde_json::json!({"name": model_name, "model": model_name}),
                );
            }
        }
    }

    let body = serde_json::json!({"models": merged.values().collect::<Vec<_>>()});
    Ok(axum::Json(body).into_response())
}

/// Merge `/api/ps` from all healthy instances into a unified response.
async fn proxy_merged_ps(state: &ProxyState) -> Result<Response, StatusCode> {
    let instances = state.app.instances.read().await;
    let mut all_running = Vec::new();

    for inst in instances.values() {
        if !inst.is_routable() {
            continue;
        }
        for loaded in &inst.models_loaded {
            all_running.push(serde_json::json!({
                "name": loaded.name,
                "model": loaded.name,
                "size_vram": loaded.size_vram,
                "expires_at": loaded.expires_at,
                "stone": inst.stone.name,
            }));
        }
    }

    let body = serde_json::json!({"models": all_running});
    Ok(axum::Json(body).into_response())
}

// ── Show with Catalog Fallback ─────────────────────────────────

/// Proxy `/api/show` — tries upstream first, falls back to model catalog.
async fn proxy_show(
    state: &ProxyState,
    path: &str,
    method: Method,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let model_name = extract_model(&body);

    // Try upstream: route to an instance that has the model
    let target = if let Some(ref m) = model_name {
        let instances = state.app.instances.read().await;
        instances
            .values()
            .find(|i| i.is_routable() && i.models_available.iter().any(|name| name == m))
            .map(|i| i.endpoint.clone())
    } else {
        None
    };

    if let Some(ref endpoint) = target {
        let resp = state
            .client
            .forward_request(endpoint, path, method, body, headers.clone())
            .await;

        if let Ok(resp) = resp {
            if resp.status().is_success() {
                let bytes = resp.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
                return Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("content-type", "application/json")
                    .body(Body::from(bytes))
                    .expect("constant headers"));
            }
        }
        // Upstream failed or returned error — fall through to catalog
    }

    // ── Catalog fallback ──────────────────────────────────────
    let model_name = model_name.ok_or(StatusCode::BAD_REQUEST)?;
    let models = state.app.models.read().await;

    let info = models.get(&model_name).ok_or_else(|| {
        tracing::debug!(model = %model_name, "show: model not in catalog");
        StatusCode::NOT_FOUND
    })?;

    let mut model_info = serde_json::Map::new();
    if let Some(pc) = info.parameter_count {
        model_info.insert(
            "general.parameter_count".into(),
            serde_json::Value::Number(pc.into()),
        );
    }
    if let Some(ctx) = info.context_length {
        let arch = info.family.as_deref().unwrap_or("general");
        model_info.insert(
            format!("{arch}.context_length"),
            serde_json::Value::Number(ctx.into()),
        );
        model_info.insert(
            "general.context_length".into(),
            serde_json::Value::Number(ctx.into()),
        );
    }

    let body = serde_json::json!({
        "modelfile": "",
        "parameters": "",
        "template": "",
        "details": {
            "family": info.family,
            "families": info.families,
            "parameter_size": info.parameter_size,
            "quantization_level": info.quantization_level,
            "format": info.format,
        },
        "model_info": model_info,
        "capabilities": info.capabilities,
    });

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .header("x-zen-garden-source", "catalog")
        .body(Body::from(serde_json::to_vec(&body).unwrap_or_default()))
        .expect("constant headers"))
}

// ── Routed Management ──────────────────────────────────────────

/// Proxy a management request to any healthy instance that has the model,
/// or any healthy instance as fallback (for pull, blob, etc.).
async fn proxy_routed(
    state: &ProxyState,
    path: &str,
    method: Method,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let model = extract_model(&body).or_else(|| extract_field(&body, "source"));

    let target = if let Some(ref m) = model {
        let instances = state.app.instances.read().await;
        instances
            .values()
            .find(|i| i.is_routable() && i.models_available.iter().any(|name| name == m))
            .or_else(|| instances.values().find(|i| i.is_routable()))
            .map(|i| i.endpoint.clone())
    } else {
        let instances = state.app.instances.read().await;
        instances
            .values()
            .find(|i| i.is_routable())
            .map(|i| i.endpoint.clone())
    };

    let target = target.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let resp = state
        .client
        .forward_request(&target, path, method, body, headers.clone())
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    let status =
        StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if content_type.contains("ndjson") || content_type.contains("octet-stream") {
        let body = Body::from_stream(resp.bytes_stream());
        Ok(Response::builder()
            .status(status)
            .header("content-type", &content_type)
            .header("transfer-encoding", "chunked")
            .body(body)
            .expect("constant headers"))
    } else {
        let bytes = resp.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
        Ok(Response::builder()
            .status(status)
            .header("content-type", "application/json")
            .body(Body::from(bytes))
            .expect("constant headers"))
    }
}

// ── Helpers ────────────────────────────────────────────────────

/// Extract the "model" field from a JSON body.
fn extract_model(body: &[u8]) -> Option<String> {
    extract_field(body, "model")
}

/// Extract a named string field from a JSON body.
fn extract_field(body: &[u8], field: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    value.get(field)?.as_str().map(|s| s.to_string())
}

/// Merge per-capability inference defaults into the request body.
/// Only injects fields the client did not already set.
///
/// Ollama uses `options.temperature`, `options.top_p`, `options.num_predict`
/// for inference parameters, so we set both top-level (OpenAI style) and
/// nested `options` (Ollama style) to cover both paths.
fn merge_inference_defaults(
    body: &mut serde_json::Map<String, serde_json::Value>,
    defaults: &InferenceDefaults,
) {
    // Top-level defaults (OpenAI compat).
    if let Some(temp) = defaults.temperature {
        body.entry("temperature")
            .or_insert(serde_json::json!(temp));
    }
    if let Some(max_tokens) = defaults.max_tokens {
        body.entry("max_tokens")
            .or_insert(serde_json::json!(max_tokens));
    }
    if let Some(top_p) = defaults.top_p {
        body.entry("top_p")
            .or_insert(serde_json::json!(top_p));
    }

    // Ollama-style nested options (temperature, top_p, num_predict).
    let options = body
        .entry("options")
        .or_insert_with(|| serde_json::json!({}));
    if let Some(opts) = options.as_object_mut() {
        if let Some(temp) = defaults.temperature {
            opts.entry("temperature")
                .or_insert(serde_json::json!(temp));
        }
        if let Some(max_tokens) = defaults.max_tokens {
            opts.entry("num_predict")
                .or_insert(serde_json::json!(max_tokens));
        }
        if let Some(top_p) = defaults.top_p {
            opts.entry("top_p")
                .or_insert(serde_json::json!(top_p));
        }
    }

    // Clean up empty options object to avoid confusing Ollama.
    if body
        .get("options")
        .and_then(|v| v.as_object())
        .is_some_and(|o| o.is_empty())
    {
        body.remove("options");
    }
}

/// Infer the `Capability` from the Ollama API path and body.
///
/// Returns `None` when the path does not map to a known capability,
/// which is fine — the routing layer treats `None` as "any capability".
fn capability_from_path(path: &str, body: &serde_json::Value) -> Option<Capability> {
    match path {
        "/api/embed" | "/api/embeddings" => Some(Capability::Embed),
        "/api/generate" => {
            // Vision if images are present
            if body.get("images").and_then(|v| v.as_array()).is_some_and(|a| !a.is_empty()) {
                Some(Capability::Vision)
            } else {
                Some(Capability::Generate)
            }
        }
        "/api/chat" => {
            // Tools if tools array is present
            if body.get("tools").and_then(|v| v.as_array()).is_some_and(|a| !a.is_empty()) {
                Some(Capability::Tools)
            } else {
                // Vision if any message has images
                let has_images = body
                    .get("messages")
                    .and_then(|v| v.as_array())
                    .is_some_and(|msgs| {
                        msgs.iter().any(|m| {
                            m.get("images")
                                .and_then(|v| v.as_array())
                                .is_some_and(|a| !a.is_empty())
                        })
                    });
                if has_images {
                    Some(Capability::Vision)
                } else {
                    Some(Capability::Chat)
                }
            }
        }
        _ => None,
    }
}
