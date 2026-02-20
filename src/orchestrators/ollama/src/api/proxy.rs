//! Ollama-compatible proxy endpoints.
//!
//! The core of the router: accepts Ollama API requests on :11434,
//! extracts the model name, routes to the optimal instance, and
//! streams the response back. NDJSON passthrough with metrics extraction.

use crate::app_state::AppState;
use crate::domain::routing;
use crate::domain::types::{
    AutoPullMode, JobKind, JobStatus, MetricEvent, OllamaInferenceFinal, RoutingError,
};
use crate::infra::ollama_client::OllamaClient;
use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use futures_util::StreamExt;
use std::sync::atomic::Ordering;

/// Shared proxy state.
#[derive(Clone)]
pub struct ProxyState {
    pub app: AppState,
    pub client: OllamaClient,
}

/// Proxy handler for all Ollama API paths.
///
/// Routes: POST /api/generate, /api/chat, /api/embed, /api/embeddings
/// Also handles: GET /api/tags, /api/ps, /api/version, POST /api/show
pub async fn proxy_handler(
    State(state): State<ProxyState>,
    req: Request,
) -> Result<Response, StatusCode> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let headers = req.headers().clone();

    // Read the full body
    let body_bytes = axum::body::to_bytes(req.into_body(), 50 * 1024 * 1024) // 50MB limit
        .await
        .map_err(|_| StatusCode::PAYLOAD_TOO_LARGE)?;

    // Dispatch based on path
    match (method.clone(), path.as_str()) {
        // ── Root health probe (Ollama clients expect "Ollama is running") ──
        (Method::GET, "/") => Ok(Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/plain; charset=utf-8")
            .body(Body::from("Ollama is running"))
            .unwrap()),
        (Method::HEAD, "/") => Ok(Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/plain; charset=utf-8")
            .body(Body::empty())
            .unwrap()),

        // ── Inference endpoints (routed) ──
        (Method::POST, "/api/generate" | "/api/chat" | "/api/embed" | "/api/embeddings") => {
            proxy_inference(&state, &path, &headers, body_bytes).await
        }

        // ── Merged discovery endpoints ──
        (Method::GET, "/api/tags") => proxy_merged_tags(&state).await,
        (Method::GET, "/api/ps") => proxy_merged_ps(&state).await,

        // ── Pass-through endpoints (routed to specific instance) ──
        (Method::POST, "/api/show") => {
            proxy_routed(&state, &path, method, &headers, body_bytes).await
        }
        (Method::POST, "/api/pull") => {
            proxy_routed(&state, &path, method, &headers, body_bytes).await
        }
        (Method::DELETE, "/api/delete") => {
            proxy_routed(&state, &path, method, &headers, body_bytes).await
        }
        (Method::POST, "/api/copy") => {
            proxy_routed(&state, &path, method, &headers, body_bytes).await
        }
        (Method::POST, "/api/create") => {
            proxy_routed(&state, &path, method, &headers, body_bytes).await
        }

        // ── Version (router's own) ──
        (Method::GET, "/api/version") => {
            let body = serde_json::json!({"version": env!("CARGO_PKG_VERSION")});
            Ok(axum::Json(body).into_response())
        }

        // ── Blob endpoints (proxied to any healthy instance) ──
        _ if path.starts_with("/api/blobs/") && matches!(method, Method::HEAD | Method::POST) => {
            proxy_blob(&state, &path, method, &headers, body_bytes).await
        }

        // ── Fallback ──
        _ => Err(StatusCode::NOT_FOUND),
    }
}

/// Proxy an inference request with VRAM-aware routing and NDJSON metrics extraction.
async fn proxy_inference(
    state: &ProxyState,
    path: &str,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Response, StatusCode> {
    // Extract model name from request body
    let model = extract_model(&body).ok_or(StatusCode::BAD_REQUEST)?;

    // Check if streaming is explicitly disabled
    let stream_disabled = serde_json::from_slice::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("stream")?.as_bool())
        == Some(false);

    // Route to best instance (snapshot state, no locks held during routing)
    let decision = {
        let mut instances = state.app.instances.read().await.clone();
        let models = state.app.models.read().await.clone();
        let tiers = state.app.tiers.read().await.clone();
        let gpu_matrix = {
            let run = state.app.benchmark_run.read().await;
            run.gpu_matrix.clone()
        };

        // Patch live queue depths from atomics (brief lock, then drop)
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
        routing::select_instance(&model, &instances, &models, &tiers, 64, fitness_ref)
    };

    let decision = match decision {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(model = %model, error = %e, "routing failed");

            // On-demand pull: if model is unknown and mode is OnDemand,
            // spawn a background job to check feasibility and pull.
            if let RoutingError::ModelNotFound(ref missing_model) = e {
                let mode = {
                    let config = state.app.config.read().await;
                    config.features.auto_pull_mode
                };
                if mode == AutoPullMode::OnDemand {
                    let app = state.app.clone();
                    let client = state.client.clone();
                    let model_name = missing_model.clone();
                    tokio::spawn(async move {
                        on_demand_pull_job(app, client, model_name).await;
                    });
                }
            }

            let body = serde_json::json!({"error": e.to_string()});
            return Ok((StatusCode::SERVICE_UNAVAILABLE, axum::Json(body)).into_response());
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

    // Forward request to target instance
    let result = state
        .client
        .forward_request(target, path, Method::POST, body, headers.clone())
        .await;

    let response = match result {
        Ok(r) => r,
        Err(e) => {
            counter.fetch_sub(1, Ordering::Relaxed);
            // Error-based inference: if we can't connect, mark unhealthy
            state
                .app
                .set_instance_health(
                    target,
                    crate::domain::types::InstanceHealth::Unhealthy {
                        since: std::time::Instant::now(),
                        reason: e.to_string(),
                    },
                )
                .await;
            let _ = state.app.metrics_tx.send(MetricEvent::Error {
                stone: stone_name.clone(),
            });
            let body = serde_json::json!({"error": format!("upstream error: {e}")});
            return Ok((StatusCode::BAD_GATEWAY, axum::Json(body)).into_response());
        }
    };

    let status = response.status();

    // Error-based inference: 404 = model not found (deleted outside router)
    if status == reqwest::StatusCode::NOT_FOUND {
        counter.fetch_sub(1, Ordering::Relaxed);
        tracing::warn!(model = %model, target = %target, "model not found — removed outside router?");
        // Remove from registry (snapshot then drop lock before mutation)
        {
            let reg = state.app.instances.read().await;
            if let Some(inst) = reg.get(target) {
                let avail: Vec<String> = inst
                    .models_available
                    .iter()
                    .filter(|m| m.as_str() != model)
                    .cloned()
                    .collect();
                let loaded = inst
                    .models_loaded
                    .iter()
                    .filter(|m| m.name != model)
                    .cloned()
                    .collect();
                drop(reg);
                state
                    .app
                    .update_instance_models(target, avail, loaded)
                    .await;
            }
        }
        let _ = state.app.metrics_tx.send(MetricEvent::Error {
            stone: stone_name.clone(),
        });
        let body = serde_json::json!({"error": format!("model '{model}' not found")});
        return Ok((StatusCode::NOT_FOUND, axum::Json(body)).into_response());
    }

    // Propagate non-OK status
    if !status.is_success() {
        counter.fetch_sub(1, Ordering::Relaxed);
        let _ = state.app.metrics_tx.send(MetricEvent::Error {
            stone: stone_name.clone(),
        });
        let status_code =
            StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let text = response.text().await.unwrap_or_default();
        return Ok((status_code, text).into_response());
    }

    if stream_disabled {
        // Non-streaming: read full response, extract metrics, forward
        let response_bytes = response
            .bytes()
            .await
            .map_err(|_| StatusCode::BAD_GATEWAY)?;
        counter.fetch_sub(1, Ordering::Relaxed);

        // Extract metrics from the response
        if let Ok(final_obj) = serde_json::from_slice::<OllamaInferenceFinal>(&response_bytes) {
            let _ = state.app.metrics_tx.send(MetricEvent::Request {
                stone: stone_name.clone(),
                model: model.clone(),
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
            .unwrap())
    } else {
        // Streaming: pass through NDJSON, inspect each line for metrics
        let app = state.app.clone();
        let stone_for_metrics = stone_name.clone();
        let model_for_metrics = model.clone();

        let upstream = response.bytes_stream();

        let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(64);

        // Spawn a task to tee the stream: forward chunks AND inspect for final object
        tokio::spawn(async move {
            let mut line_buf = Vec::new();
            futures_util::pin_mut!(upstream);

            while let Some(chunk_result) = upstream.next().await {
                match chunk_result {
                    Ok(chunk) => {
                        // Forward chunk to client
                        if tx.send(Ok(chunk.clone())).await.is_err() {
                            break; // client disconnected
                        }

                        // Buffer for NDJSON line parsing
                        line_buf.extend_from_slice(&chunk);
                        while let Some(pos) = line_buf.iter().position(|&b| b == b'\n') {
                            let line = &line_buf[..pos];
                            if !line.is_empty() {
                                if let Ok(obj) =
                                    serde_json::from_slice::<OllamaInferenceFinal>(line)
                                {
                                    if obj.done {
                                        let _ = app.metrics_tx.send(MetricEvent::Request {
                                            stone: stone_for_metrics.clone(),
                                            model: model_for_metrics.clone(),
                                            tokens_in: obj.prompt_eval_count,
                                            tokens_out: obj.eval_count,
                                            duration_ns: obj.total_duration,
                                            eval_duration_ns: obj.eval_duration,
                                        });
                                    }
                                }
                            }
                            line_buf.drain(..=pos);
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "upstream stream error");
                        let _ = tx.send(Err(std::io::Error::other(e))).await;
                        break;
                    }
                }
            }

            // Decrement queue depth when stream ends
            counter.fetch_sub(1, Ordering::Relaxed);
        });

        let body_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        let body = Body::from_stream(body_stream);

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/x-ndjson")
            .header("transfer-encoding", "chunked")
            .body(body)
            .unwrap())
    }
}

/// Proxy a request that needs routing by model name (show, pull, delete, copy, create).
async fn proxy_routed(
    state: &ProxyState,
    path: &str,
    method: Method,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let model = extract_model(&body).or_else(|| extract_field(&body, "source"));

    // For model management, route to the first healthy instance that has it
    // (or any healthy instance for pull)
    let target = if let Some(ref m) = model {
        let instances = state.app.instances.read().await;
        instances
            .values()
            .find(|i| i.health.is_routable() && i.models_available.iter().any(|name| name == m))
            .or_else(|| instances.values().find(|i| i.health.is_routable()))
            .map(|i| i.endpoint.clone())
    } else {
        let instances = state.app.instances.read().await;
        instances
            .values()
            .find(|i| i.health.is_routable())
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

    // Check if response is streaming (pull, create)
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if content_type.contains("ndjson") || content_type.contains("octet-stream") {
        // Stream through
        let body = Body::from_stream(resp.bytes_stream());
        Ok(Response::builder()
            .status(status)
            .header("content-type", &content_type)
            .header("transfer-encoding", "chunked")
            .body(body)
            .unwrap())
    } else {
        let bytes = resp.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
        Ok(Response::builder()
            .status(status)
            .header("content-type", "application/json")
            .body(Body::from(bytes))
            .unwrap())
    }
}

/// Merge `/api/tags` from all instances into a unified response.
async fn proxy_merged_tags(state: &ProxyState) -> Result<Response, StatusCode> {
    let instances = state.app.instances.read().await;
    let models = state.app.models.read().await;

    // Deduplicate models by name — show the richest metadata
    let mut merged: std::collections::HashMap<String, serde_json::Value> =
        std::collections::HashMap::new();

    for inst in instances.values() {
        if !inst.health.is_routable() {
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

    let body = serde_json::json!({
        "models": merged.values().collect::<Vec<_>>()
    });
    Ok(axum::Json(body).into_response())
}

/// Merge `/api/ps` from all instances into a unified response.
async fn proxy_merged_ps(state: &ProxyState) -> Result<Response, StatusCode> {
    let instances = state.app.instances.read().await;
    let mut all_running = Vec::new();

    for inst in instances.values() {
        if !inst.health.is_routable() {
            continue;
        }
        for loaded in &inst.models_loaded {
            all_running.push(serde_json::json!({
                "name": loaded.name,
                "model": loaded.name,
                "size_vram": loaded.size_vram,
                "expires_at": loaded.expires_at,
                "stone": inst.stone_name,
            }));
        }
    }

    let body = serde_json::json!({"models": all_running});
    Ok(axum::Json(body).into_response())
}

/// Background job: attempt to pull an unknown model to all healthy instances.
///
/// Spawned when a request arrives for a model that doesn't exist anywhere
/// and `auto_pull_mode` is `OnDemand`. The caller still gets a 404, but
/// the model will be available for the next request if the pull succeeds.
async fn on_demand_pull_job(app: AppState, client: OllamaClient, model: String) {
    tracing::info!(model = %model, "on-demand pull: starting background job");

    let job_id = app
        .create_job(JobKind::OnDemandPull {
            model: model.clone(),
        })
        .await;
    app.update_job(&job_id, JobStatus::Running, None).await;

    // Select healthy targets
    let targets: Vec<String> = {
        let instances = app.instances.read().await;
        instances
            .values()
            .filter(|i| i.health.is_routable())
            .map(|i| i.endpoint.clone())
            .collect()
    };

    if targets.is_empty() {
        app.fail_job(&job_id, "no healthy instances available")
            .await;
        return;
    }

    let mut any_success = false;
    for target in &targets {
        app.update_job(
            &job_id,
            JobStatus::Running,
            Some(format!("pulling to {target}")),
        )
        .await;

        match client.pull_model(target, &model).await {
            Ok(mut stream) => {
                let mut last_status = String::new();
                while let Some(chunk) = stream.next().await {
                    if let Ok(bytes) = chunk {
                        if let Ok(progress) = serde_json::from_slice::<
                            crate::domain::types::OllamaPullProgress,
                        >(&bytes)
                        {
                            last_status = progress.status;
                        }
                    }
                }
                if last_status == "success" {
                    tracing::info!(model = %model, target = %target, "on-demand pull succeeded");
                    any_success = true;

                    // Re-profile the instance to pick up the new model
                    if let Ok((avail, loaded, infos, _)) = client.full_profile(target).await {
                        app.update_instance_models(target, avail, loaded).await;
                        for info in infos {
                            app.upsert_model(info).await;
                        }
                    }
                } else {
                    tracing::warn!(model = %model, target = %target, status = %last_status, "on-demand pull did not succeed");
                }
            }
            Err(e) => {
                tracing::warn!(model = %model, target = %target, error = %e, "on-demand pull error");
            }
        }
    }

    if any_success {
        app.complete_job(&job_id).await;
        app.emit_event("models.updated", "{}").await;
    } else {
        app.fail_job(&job_id, "pull failed on all instances").await;
    }
}

/// Proxy blob check/upload (`HEAD /api/blobs/:digest`, `POST /api/blobs/:digest`).
///
/// Blobs are content-addressed, so any healthy instance will do.
/// HEAD returns 200/404 (exists?), POST uploads the layer data.
async fn proxy_blob(
    state: &ProxyState,
    path: &str,
    method: Method,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let target = {
        let instances = state.app.instances.read().await;
        instances
            .values()
            .find(|i| i.health.is_routable())
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
    let bytes = resp.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;

    Ok(Response::builder()
        .status(status)
        .body(Body::from(bytes))
        .unwrap())
}

/// Extract the "model" field from a JSON body.
fn extract_model(body: &[u8]) -> Option<String> {
    extract_field(body, "model")
}

/// Extract a named string field from a JSON body.
fn extract_field(body: &[u8], field: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    value.get(field)?.as_str().map(|s| s.to_string())
}
