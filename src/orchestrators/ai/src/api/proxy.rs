//! Unified proxy handler — dispatches to offering adapters via capability routing.
//!
//! The proxy handler:
//! 1. Extracts model name and capability from the request.
//! 2. Calls the routing engine to select an instance.
//! 3. Dispatches to the offering adapter's `proxy()` method.
//! 4. Forwards the response (streaming or complete) to the client.

use std::sync::atomic::Ordering;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;

use crate::app_state::AppState;
use crate::catalog::ProxyRequest;
use crate::domain::routing;
use crate::domain::types::{Capability, MetricEvent};

/// Main proxy handler — all inference requests come through here.
pub async fn proxy_handler(
    State(state): State<AppState>,
    method: Method,
    headers: HeaderMap,
    uri: axum::http::Uri,
    body: Bytes,
) -> Response {
    let path = uri.path().to_string();

    // ── Infer capability from path ──────────────────────────────
    let capability = capability_from_path(&path);

    // ── Extract model from body ─────────────────────────────────
    //
    // JSON-body requests (Chat, Embed, Speak, Translate, Rerank):
    //   model is in the JSON body's "model" field.
    //
    // Multipart requests (Transcribe via whisper.cpp/Speaches):
    //   model may be absent — these services load a single model at
    //   startup. Use a sentinel "whisper" so routing can match.
    //
    // ComfyUI workflow requests (Imagine, Edit, Render):
    //   model is inside the workflow JSON. For raw forwarding, use
    //   a sentinel "comfyui" and route by capability.
    let model = {
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
        let from_json = parsed
            .get("model")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();

        if !from_json.is_empty() {
            // Resolve recommended:* monikers (ORCH-0011).
            if let Some(cap_name) = from_json.strip_prefix("recommended:") {
                let recommended = state.recommended_models.read().await;
                match recommended.get(cap_name) {
                    Some(resolved) => resolved.clone(),
                    None => {
                        return (
                            StatusCode::NOT_FOUND,
                            format!("no recommended model for capability '{cap_name}'"),
                        )
                            .into_response();
                    }
                }
            } else {
                from_json
            }
        } else {
            // For multipart/capability-only requests, use a sentinel based
            // on the capability so the routing engine can still match.
            match capability {
                Some(Capability::Transcribe) => "whisper".to_string(),
                Some(Capability::Imagine | Capability::Edit | Capability::Render) => {
                    "comfyui".to_string()
                }
                _ => {
                    return (StatusCode::BAD_REQUEST, "missing 'model' field").into_response();
                }
            }
        }
    };

    // ── Route ───────────────────────────────────────────────────
    let mut instances = state.instances.read().await.clone();
    let models = state.models.read().await.clone();
    let tiers = state.tiers.read().await.clone();

    // Patch live queue-depth counters into the cloned snapshot.
    // Without this, routing always sees queue_depth=0 and the
    // max_queue saturation guard is functionally dead.
    {
        let depths = state.queue_depths.read().await;
        for (ep, counter) in depths.iter() {
            if let Some(inst) = instances.get_mut(ep) {
                inst.queue_depth = counter.load(Ordering::Relaxed);
            }
        }
    }
    let benchmark = state.benchmark_run.read().await;
    let fitness = &benchmark.gpu_matrix;
    let demand = state.metrics.read().await.demand_shares(3600);

    let decision = match routing::select_instance(
        &model,
        capability,
        &instances,
        &models,
        &tiers,
        64, // max_queue
        Some(fitness),
        &demand,
    ) {
        Ok(d) => d,
        Err(e) => {
            return (StatusCode::NOT_FOUND, e.to_string()).into_response();
        }
    };

    // ── Lease management ──────────────────────────────────────────
    if decision.lease_acquired {
        let now = std::time::Instant::now();
        let mut leases = state.leases.write().await;
        leases.acquire(&decision.endpoint, &decision.model, now);
    }

    // ── Increment queue depth ───────────────────────────────────
    let counter = state.queue_counter(&decision.endpoint).await;
    counter.fetch_add(1, Ordering::Relaxed);

    // ── Dispatch to offering adapter ────────────────────────────
    let offering = match state.catalog.get(decision.kind) {
        Some(o) => o.clone(),
        None => {
            counter.fetch_sub(1, Ordering::Relaxed);
            return (StatusCode::INTERNAL_SERVER_ERROR, "offering not registered")
                .into_response();
        }
    };

    let proxy_req = ProxyRequest {
        method: method.clone(),
        path: path.clone(),
        headers: headers.clone(),
        body,
    };

    let result = offering
        .proxy(&decision.endpoint, capability.unwrap_or(Capability::Chat), proxy_req)
        .await;

    counter.fetch_sub(1, Ordering::Relaxed);

    match result {
        Ok(proxy_resp) => {
            // Emit metric event (fire-and-forget).
            let _ = state.metrics_tx.send(MetricEvent::Request {
                stone: decision.stone.name.clone(),
                model: decision.model.clone(),
                capability: capability.unwrap_or(Capability::Chat),
                tokens_in: 0,  // Populated by metrics processor from NDJSON parsing.
                tokens_out: 0,
                duration_ns: 0,
                eval_duration_ns: 0,
            });

            build_response(proxy_resp)
        }
        Err(e) => {
            let _ = state.metrics_tx.send(MetricEvent::Error {
                stone: decision.stone.name.clone(),
                model: Some(decision.model.clone()),
                status_code: Some(502),
                reason: Some(e.to_string()),
            });
            (StatusCode::BAD_GATEWAY, e.to_string()).into_response()
        }
    }
}

/// Build an Axum response from a ProxyResponse.
fn build_response(proxy: crate::catalog::ProxyResponse) -> Response {
    let status = StatusCode::from_u16(proxy.status).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut builder = Response::builder().status(status);

    for (key, value) in &proxy.headers {
        builder = builder.header(key.as_str(), value.as_str());
    }

    let build_result = match proxy.body {
        crate::catalog::ProxyBody::Complete(bytes) => builder.body(Body::from(bytes)),
        crate::catalog::ProxyBody::Stream(stream) => {
            builder.body(Body::from_stream(stream))
        }
    };

    match build_result {
        Ok(resp) => resp,
        Err(e) => {
            tracing::warn!(error = %e, "failed to build proxy response");
            Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(Body::from("proxy response build error"))
                .expect("static error response")
        }
    }
}

/// Infer capability from the request path.
fn capability_from_path(path: &str) -> Option<Capability> {
    match path {
        "/api/generate" | "/api/chat" => Some(Capability::Chat),
        "/api/embed" | "/api/embeddings" => Some(Capability::Embed),
        "/api/imagine" => Some(Capability::Imagine),
        "/api/edit" => Some(Capability::Edit),
        "/api/render" => Some(Capability::Render),
        "/api/transcribe" => Some(Capability::Transcribe),
        "/api/speak" => Some(Capability::Speak),
        "/api/rerank" => Some(Capability::Rerank),
        "/api/translate" => Some(Capability::Translate),
        _ => None,
    }
}
