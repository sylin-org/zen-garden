//! Greenhouse API — manifest authoring endpoints for the Portrait SPA.
//!
//! Thin API layer for Phase 3 (OFFER-0006). Reuses Phase 2 validation
//! and Phase 1 inspection; no duplicated logic.
//!
//! Endpoints:
//! - `GET  /api/v1/stone/greenhouse/containers` — running offerings for picker
//! - `POST /api/v1/stone/greenhouse/validate`   — real-time manifest validation
//! - `POST /api/v1/stone/greenhouse/generate`    — generate manifest from inspection

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

use crate::{error_response, AppState};
use garden_common::api_utils::ApiErrorResponse;
use garden_common::manifests::{generate, validation};

// ============================================================================
// DTOs
// ============================================================================

/// A running offering entry for the container picker UI.
#[derive(Debug, Serialize)]
pub struct ContainerEntry {
    pub name: String,
    pub image: String,
    pub status: String,
    pub ports: Vec<String>,
}

/// Request body for `POST /greenhouse/validate`.
#[derive(Debug, Deserialize)]
pub struct ValidateRequest {
    pub snippet_yaml: String,
    #[serde(default)]
    pub frontmatter_json: Option<String>,
}

/// Response body for `POST /greenhouse/validate`.
#[derive(Debug, Serialize)]
pub struct ValidateResponse {
    pub valid: bool,
    pub findings: Vec<validation::ValidationFinding>,
}

/// Request body for `POST /greenhouse/generate`.
#[derive(Debug, Deserialize)]
pub struct GenerateRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    pub inspection: serde_json::Value,
}

/// Response body for `POST /greenhouse/generate`.
#[derive(Debug, Serialize)]
pub struct GenerateResponse {
    pub name: String,
    pub snippet_yaml: String,
    pub frontmatter_json: String,
    pub compatibility_yaml: String,
    pub guidance_md: String,
}

// ============================================================================
// Handlers
// ============================================================================

/// `GET /api/v1/stone/greenhouse/containers`
///
/// Lists managed offerings that are currently running, for the "pick a
/// container" source selector in the Greenhouse UI.
pub async fn list_containers_v1(
    State(state): State<AppState>,
) -> Result<Json<Vec<ContainerEntry>>, (StatusCode, Json<ApiErrorResponse>)> {
    let offerings = state.offerings.read().await;

    let entries: Vec<ContainerEntry> = offerings
        .iter()
        .filter(|o| o.is_managed())
        .map(|o| {
            let ports: Vec<String> = {
                let mut ps = Vec::new();
                if o.location.port > 0 {
                    ps.push(format!("{}:{}", o.location.port, o.location.port));
                }
                for (name, &host_port) in &o.location.port_map {
                    ps.push(format!("{host_port} ({name})"));
                }
                ps
            };

            ContainerEntry {
                name: o.name.to_string(),
                image: o.offering.clone(),
                status: o.status.to_string(),
                ports,
            }
        })
        .collect();

    Ok(Json(entries))
}

/// `POST /api/v1/stone/greenhouse/validate`
///
/// Validates a manifest snippet (and optional frontmatter) and returns
/// findings with severity levels. Used for real-time validation in the
/// Greenhouse form.
pub async fn validate_manifest_v1(
    Json(payload): Json<ValidateRequest>,
) -> Result<Json<ValidateResponse>, (StatusCode, Json<ApiErrorResponse>)> {
    let mut findings = validation::validate_snippet(&payload.snippet_yaml, "snippet.yaml");

    if let Some(ref fm) = payload.frontmatter_json {
        findings.extend(validation::validate_frontmatter(fm, "frontmatter.json"));
    }

    let valid = !findings
        .iter()
        .any(|f| f.severity == validation::Severity::Error);

    Ok(Json(ValidateResponse { valid, findings }))
}

/// `POST /api/v1/stone/greenhouse/generate`
///
/// Generates a full manifest file set from image inspection JSON.
/// The inspection payload is the same JSON returned by
/// `GET /api/v1/stone/offerings/inspect?image={ref}`.
pub async fn generate_manifest_v1(
    Json(payload): Json<GenerateRequest>,
) -> Result<Json<GenerateResponse>, (StatusCode, Json<ApiErrorResponse>)> {
    let result = generate::generate_from_inspection(
        payload.name.as_deref(),
        payload.category.as_deref(),
        &payload.inspection,
    )
    .map_err(|e| {
        error_response(
            StatusCode::BAD_REQUEST,
            "GENERATION_FAILED",
            format!("Failed to generate manifest: {e}"),
            None,
        )
    })?;

    Ok(Json(GenerateResponse {
        name: result.name,
        snippet_yaml: result.snippet_yaml,
        frontmatter_json: result.frontmatter_json,
        compatibility_yaml: result.compatibility_yaml,
        guidance_md: result.guidance_md,
    }))
}
