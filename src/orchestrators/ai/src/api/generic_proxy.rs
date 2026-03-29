//! Generic proxy handler for non-Ollama offerings.
//!
//! Forwards requests to a healthy instance of the specified offering type.
//! Speaks the service's native protocol (pass-through). No metrics extraction
//! or moniker resolution — those are Ollama-specific.

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::domain::types::OfferingKind;
use crate::AppState;

/// State for a generic offering proxy — just the app state + which offering.
#[derive(Clone)]
pub struct GenericProxyState {
    pub app: AppState,
    pub kind: OfferingKind,
}

/// Pass-through proxy: pick a healthy instance, forward the request, return the response.
pub async fn proxy_handler(
    State(state): State<GenericProxyState>,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let query = req.uri().query().map(|q| format!("?{q}")).unwrap_or_default();
    let headers = req.headers().clone();

    // Find a healthy instance for this offering
    let target = {
        let instances = state.app.instances.read().await;
        instances
            .values()
            .find(|i| i.kind == state.kind && i.is_routable())
            .map(|i| i.endpoint.clone())
    };

    let target = match target {
        Some(t) => t,
        None => {
            return Ok(axum::Json(serde_json::json!({
                "error": format!("no healthy {} instances", state.kind)
            }))
            .into_response());
        }
    };

    // Build upstream URL
    let upstream_url = format!("{target}{path}{query}");

    // Forward the request
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut upstream = client.request(
        reqwest::Method::from_bytes(method.as_str().as_bytes()).unwrap_or(reqwest::Method::GET),
        &upstream_url,
    );

    // Forward relevant headers
    for (name, value) in headers.iter() {
        let name_str = name.as_str();
        if matches!(
            name_str,
            "content-type" | "accept" | "authorization" | "x-api-key"
        ) {
            if let Ok(v) = value.to_str() {
                upstream = upstream.header(name_str, v);
            }
        }
    }

    // Forward body (stream through, no buffering)
    let body_stream = req.into_body().into_data_stream();
    upstream = upstream.body(reqwest::Body::wrap_stream(body_stream));

    let resp = upstream.send().await.map_err(|e| {
        tracing::warn!(
            kind = %state.kind,
            target = %target,
            error = %e,
            "upstream connection failed"
        );
        StatusCode::BAD_GATEWAY
    })?;

    // Build response
    let status = StatusCode::from_u16(resp.status().as_u16())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    let mut builder = Response::builder().status(status);

    // Forward response headers
    for (name, value) in resp.headers().iter() {
        if let Ok(v) = value.to_str() {
            builder = builder.header(name.as_str(), v);
        }
    }

    // Stream the response body
    let body = Body::from_stream(resp.bytes_stream());

    builder
        .body(body)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}
