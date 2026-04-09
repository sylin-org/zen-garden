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

/// `GET /v1/catalog/{path}` — full schema for one registration.
///
/// The `{path}` is a dotted registration path like `text.chat` or
/// `image.generate.sample-tron`. Returns the full field list with
/// widget hints, constraints, and media specs — everything needed to
/// render a Try It form.
pub async fn get_catalog_detail(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> Response {
    let directory = &state.capability_directory;

    // Try as a base primitive first (e.g., "text.chat" → Primitive::TextChat)
    if let Ok(primitive) = path.parse::<Primitive>() {
        let providers = directory.providers_for_primitive(primitive).await;
        if providers.is_empty() {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": "registration_not_found",
                    "message": format!("No provider registered for '{path}'"),
                    "path": path,
                })),
            )
                .into_response();
        }

        // Find the first enabled provider's capability for this primitive
        // to extract the full parameter schema.
        let mut detail = json!({
            "path": path,
            "kind": "primitive",
            "primitive": primitive,
            "display_name": primitive.summary(),
            "providers": providers,
        });

        // Attach field schema from the first provider that declares parameters.
        for provider in &providers {
            if let Some(cap) = directory.capability(provider, primitive).await {
                if !cap.parameters.is_empty() {
                    detail["fields"] = serde_json::to_value(&cap.parameters)
                        .unwrap_or_default();
                }
                if !cap.media_inputs.is_empty() {
                    detail["media_inputs"] = serde_json::to_value(&cap.media_inputs)
                        .unwrap_or_default();
                }
                break;
            }
        }

        return Json(detail).into_response();
    }

    // Try as a skill registration (e.g., "image.generate.sample-tron")
    // Parse: everything up to the last dot is the primitive, the rest is the skill id.
    if let Some(dot_pos) = path.rfind('.') {
        let primitive_str = &path[..dot_pos];
        let skill_id = &path[dot_pos + 1..];

        if let Ok(primitive) = primitive_str.parse::<Primitive>() {
            let providers = directory
                .providers_for_skill(primitive, skill_id)
                .await;

            if !providers.is_empty() {
                let provider = &providers[0];
                let skill = directory.skill(provider, skill_id).await;

                let mut detail = json!({
                    "path": path,
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
                }

                // Also attach media inputs from the parent capability.
                if let Some(cap) = directory.capability(provider, primitive).await {
                    if !cap.media_inputs.is_empty() {
                        detail["media_inputs"] = serde_json::to_value(&cap.media_inputs)
                            .unwrap_or_default();
                    }
                }

                return Json(detail).into_response();
            }
        }
    }

    // Nothing matched.
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "error": "registration_not_found",
            "message": format!("No registration found for path '{path}'"),
            "path": path,
        })),
    )
        .into_response()
}

