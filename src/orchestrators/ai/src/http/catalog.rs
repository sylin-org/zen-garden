//! `GET /v1/catalog` — navigation summary (ORCH-0030 R2 §R2.2.3).
//! `GET /v1/catalog/{path}` — full schema for one registration.
//!
//! Two views over the same store:
//!
//! - **`/v1/catalog`** returns the navigation view: path, display_name,
//!   and providers for every registration. Tiny, cacheable, drives a
//!   sidebar or command palette.
//! - **`/v1/catalog/{path}`** returns the full `RegistrationDescriptor`
//!   for one dotted path: all fields with types, widget hints,
//!   min/max/step, options, auto descriptors, and media specs. This is
//!   what drives the Try It form.
//!
//! The catalog builder pre-renders the navigation summary; the detail
//! view is assembled on-demand from the `CapabilityDirectory`.
//!
//! The previous `/v1/catalog/events` SSE stream has been retired in
//! favor of the unified `/v1/events?focus=catalog.*` bus per
//! ORCH-0030 §1.6.

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::app_state::AppState;
use crate::domain::primitive::Primitive;

/// `GET /v1/catalog` — navigation summary.
pub async fn get_catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let docs = state.catalog.snapshot();
    let etag = format!("\"{}\"", docs.directory_version);

    if let Some(inm) = headers.get(header::IF_NONE_MATCH) {
        if let Ok(raw) = inm.to_str() {
            if raw == etag {
                return (StatusCode::NOT_MODIFIED).into_response();
            }
        }
    }

    let body = Json(docs.catalog.as_ref().clone());
    let mut response = body.into_response();
    if let Ok(v) = HeaderValue::from_str(&etag) {
        response.headers_mut().insert(header::ETAG, v);
    }
    response
}

/// `GET /v1/catalog/{modality}/{leaf}` — full schema for a base primitive.
///
/// Mirrors the dispatch URL grammar: `/v1/catalog/text/chat` returns the
/// form schema for `text.chat`. Returns the full field list with widget
/// hints, constraints, defaults, and media specs — everything needed to
/// render a Try It form.
pub async fn get_catalog_primitive(
    State(state): State<AppState>,
    Path((modality, leaf)): Path<(String, String)>,
) -> Response {
    let directory = &state.capability_directory;
    let dotted = format!("{modality}.{leaf}");

    let primitive = match dotted.parse::<Primitive>() {
        Ok(p) => p,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": "registration_not_found",
                    "message": format!("Unknown primitive '{dotted}'"),
                    "path": dotted,
                })),
            )
                .into_response();
        }
    };

    let providers = directory.providers_for_primitive(primitive).await;
    if providers.is_empty() {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "registration_not_found",
                "message": format!("No provider registered for '{dotted}'"),
                "path": dotted,
            })),
        )
            .into_response();
    }

    let mut detail = json!({
        "path": dotted,
        "kind": "primitive",
        "primitive": primitive,
        "display_name": primitive.summary(),
        "providers": providers,
    });

    // Collect fields, media_inputs, and examples from providers.
    // Use the first non-empty value for each — a primitive may be
    // served by multiple providers with different detail levels.
    for provider in &providers {
        if let Some(cap) = directory.capability(provider, primitive).await {
            if detail.get("fields").is_none() && !cap.parameters.is_empty() {
                detail["fields"] = serde_json::to_value(&cap.parameters)
                    .unwrap_or_default();
            }
            if detail.get("media_inputs").is_none() && !cap.media_inputs.is_empty() {
                detail["media_inputs"] = serde_json::to_value(&cap.media_inputs)
                    .unwrap_or_default();
            }
            if detail.get("examples").is_none() && !cap.examples.is_empty() {
                detail["examples"] = serde_json::to_value(&cap.examples)
                    .unwrap_or_default();
            }
        }
    }

    Json(detail).into_response()
}

/// `GET /v1/catalog/{modality}/{leaf}/{skill}` — full schema for a skill.
///
/// Mirrors the dispatch URL grammar: `/v1/catalog/image/generate/flux-butterfly`
/// returns the form schema for skill `flux-butterfly` under `image.generate`.
pub async fn get_catalog_skill(
    State(state): State<AppState>,
    Path((modality, leaf, skill_id)): Path<(String, String, String)>,
) -> Response {
    let directory = &state.capability_directory;
    let primitive_dotted = format!("{modality}.{leaf}");
    let fqn = format!("{primitive_dotted}.{skill_id}");

    let primitive = match primitive_dotted.parse::<Primitive>() {
        Ok(p) => p,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": "registration_not_found",
                    "message": format!("Unknown primitive '{primitive_dotted}'"),
                    "path": fqn,
                })),
            )
                .into_response();
        }
    };

    let providers = directory.providers_for_skill(primitive, &skill_id).await;
    if providers.is_empty() {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "registration_not_found",
                "message": format!("No provider registered for '{fqn}'"),
                "path": fqn,
            })),
        )
            .into_response();
    }

    let provider = &providers[0];
    let skill = directory.skill(provider, &skill_id).await;

    let mut detail = json!({
        "path": fqn,
        "kind": "skill",
        "primitive": primitive,
        "skill_id": skill_id,
        "providers": providers,
    });

    if let Some(s) = &skill {
        detail["display_name"] = json!(s.display.name);
        if let Some(desc) = &s.display.description {
            detail["description"] = json!(desc);
        }
        if !s.display.tags.is_empty() {
            detail["tags"] = json!(s.display.tags);
        }
        if let Some(img) = &s.display.preview_image {
            detail["preview_image"] = json!(img);
        }
        detail["fields"] = serde_json::to_value(&s.parameters)
            .unwrap_or_default();
        if !s.examples.is_empty() {
            detail["examples"] = serde_json::to_value(&s.examples)
                .unwrap_or_default();
        }
    }

    // Attach media inputs and examples from the parent capability.
    if let Some(cap) = directory.capability(provider, primitive).await {
        if !cap.media_inputs.is_empty() {
            detail["media_inputs"] = serde_json::to_value(&cap.media_inputs)
                .unwrap_or_default();
        }
        // Fall back to capability examples if the skill has none.
        if skill.as_ref().is_some_and(|s| s.examples.is_empty()) && !cap.examples.is_empty() {
            detail["examples"] = serde_json::to_value(&cap.examples)
                .unwrap_or_default();
        }
    }

    Json(detail).into_response()
}

