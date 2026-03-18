// Offerings API - Human Layer
//
// Purpose: Simplified, beginner-friendly API for managing offerings
// Target audience: 90% of users - scripters, beginners, simple automation
// Philosophy: Hide Docker complexity, provide safety rails, optimize for common case

use crate::api::responses::ApiResponse;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use garden_common::offerings::{
    OfferingFqn, OfferingSearchResponse, OfferingSearchResult, TaxonomyDictionary,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::infra::embedded::EmbeddedManifests;
use crate::{bad_request, error_response, internal, unavailable, AppState};

/// Query parameters for filtering offerings
#[derive(Debug, Deserialize)]
pub struct OfferingsQuery {
    /// Filter by state: available, installing, installed
    #[serde(default)]
    state: Option<String>,
}

/// Simplified offering view for human layer
#[derive(Debug, Serialize)]
pub struct OfferingView {
    pub name: String,
    pub state: String,
    pub category: String,
    pub description: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    pub image: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<CompatibilityView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uptime: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CompatibilityView {
    pub decision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// GET /api/v1/offerings
/// List all offerings (available + installed), optionally filtered by state
pub async fn list_offerings_v1(
    State(state): State<AppState>,
    Query(query): Query<OfferingsQuery>,
) -> Result<
    (StatusCode, Json<ApiResponse<Vec<OfferingView>>>),
    (StatusCode, Json<garden_common::api_utils::ApiErrorResponse>),
> {
    // Get installed services from unified offerings registry
    let offerings_guard = state.offerings.read().await;
    let installed: HashMap<String, &garden_common::Offering> = offerings_guard
        .iter()
        .map(|o| (o.name.to_string(), o))
        .collect();

    // Get available offerings from index (may still be building)
    let idx_guard = state.offerings_index.read().await;
    let offerings_index = idx_guard.as_ref();
    let catalog_building = offerings_index.is_none();

    let mut offerings: Vec<OfferingView> = Vec::new();

    // Add installed offerings with runtime details
    if query.state.as_deref() != Some("available") {
        for offering in offerings_guard.iter() {
            let name_str = offering.name.to_string();
            let image = state
                .platform
                .docker
                .get_service_image(&name_str)
                .await
                .unwrap_or_else(|_| "<unknown>".to_string());
            let uptime = state
                .platform
                .docker
                .get_service_uptime(&name_str)
                .await
                .ok()
                .filter(|&s| s > 0)
                .map(garden_common::format_uptime);
            offerings.push(OfferingView {
                name: name_str,
                state: "installed".to_string(),
                category: offering.offering.clone(),
                description: format!("{} service", offering.offering),
                tags: vec![],
                image,
                compatibility: None,
                health: Some(simplify_health(&offering.status)),
                uptime,
            });
        }
    }

    // Add available offerings (not yet installed) - only if catalog loaded
    if query.state.as_deref() != Some("installed") {
        if let Some(offerings_index) = offerings_index {
            for offering in &offerings_index.offerings {
                if !installed.contains_key(&offering.name) {
                    offerings.push(OfferingView {
                        name: offering.name.clone(),
                        state: "available".to_string(),
                        category: offering.category.clone(),
                        description: offering.description.clone(),
                        tags: offering.tags.clone(),
                        image: offering.image.clone(),
                        compatibility: Some(CompatibilityView {
                            decision: offering.compatibility.decision.to_string(),
                            reason: offering.compatibility.reason.clone(),
                        }),
                        health: None,
                        uptime: None,
                    });
                }
            }
        }
    }

    let suggestions = if catalog_building {
        Some(vec![
            "Catalog still building - available offerings may be incomplete".to_string(),
        ])
    } else {
        None
    };

    Ok((
        StatusCode::OK,
        Json(ApiResponse {
            data: offerings,
            suggestions,
        }),
    ))
}

/// GET /api/v1/offerings/:name
/// Get details about a specific offering
pub async fn get_offering_v1(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<
    (StatusCode, Json<ApiResponse<serde_json::Value>>),
    (StatusCode, Json<garden_common::api_utils::ApiErrorResponse>),
> {
    let offering_fqn = OfferingFqn::parse(&name).map_err(|e| {
        bad_request(
            "INVALID_OFFERING_NAME",
            format!("Invalid offering name '{}': {}", name, e),
        )
    })?;
    let service_name = offering_fqn.fqn();
    let offering_type = offering_fqn.offering.clone();

    // Check if installed
    let offerings_guard = state.offerings.read().await;
    if let Some(offering) = offerings_guard
        .iter()
        .find(|o| o.name.fqn() == service_name)
    {
        return Ok((
            StatusCode::OK,
            Json(ApiResponse::new(serde_json::json!({
                "name": offering.name,
                "state": "installed",
                "category": offering.offering,
                "health": simplify_health(&offering.status),
                "version": offering.version,
            }))),
        ));
    }

    // Check if available
    let idx_guard = state.offerings_index.read().await;
    let offerings_index = idx_guard
        .as_ref()
        .ok_or_else(|| unavailable("INDEX_UNAVAILABLE", "Offerings catalog not yet loaded"))?;

    if let Some(offering) = offerings_index
        .offerings
        .iter()
        .find(|o| o.name == offering_type)
    {
        return Ok((
            StatusCode::OK,
            Json(ApiResponse::new(serde_json::json!({
                "name": offering.name,
                "state": "available",
                "category": offering.category,
                "description": offering.description,
                "tags": offering.tags,
                "compatibility": {
                    "decision": offering.compatibility.decision.to_string(),
                    "reason": offering.compatibility.reason,
                },
            }))),
        ));
    }

    // Not found
    let mut details = HashMap::new();
    details.insert("name".to_string(), serde_json::json!(name));
    Err(error_response(
        StatusCode::NOT_FOUND,
        garden_common::constants::OFFERING_NOT_FOUND,
        format!("Offering '{}' not found in catalog", name),
        Some(details),
    ))
}

/// GET /api/v1/offerings/:name/manifest
/// Get raw YAML manifest for an offering
pub async fn get_offering_manifest_v1(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<(StatusCode, String), (StatusCode, Json<garden_common::api_utils::ApiErrorResponse>)> {
    let offering_fqn = OfferingFqn::parse(&name).map_err(|e| {
        bad_request(
            "INVALID_OFFERING_NAME",
            format!("Invalid offering name '{}': {}", name, e),
        )
    })?;
    let offering_type = offering_fqn.offering;

    match state.manifest_registry.sw.get(&offering_type) {
        Some(entry) => {
            let yaml = entry
                .managed
                .as_ref()
                .map(|m| m.snippet_yaml.clone())
                .unwrap_or_default();
            Ok((StatusCode::OK, yaml))
        }
        None => {
            let mut details = HashMap::new();
            details.insert("name".to_string(), serde_json::json!(offering_type));
            Err(error_response(
                StatusCode::NOT_FOUND,
                garden_common::constants::TEMPLATE_NOT_FOUND,
                format!("Manifest for '{}' not found", offering_type),
                Some(details),
            ))
        }
    }
}

/// POST /api/v1/offerings
/// Plant an offering (simplified installation)
#[derive(Debug, Deserialize)]
pub struct PlantOfferingRequest {
    pub name: String,
    // Future: config field for environment overrides
}

pub async fn plant_offering_v1(
    State(_state): State<AppState>,
    Json(payload): Json<PlantOfferingRequest>,
) -> Result<
    (StatusCode, Json<serde_json::Value>),
    (StatusCode, Json<garden_common::api_utils::ApiErrorResponse>),
> {
    // Forward to services API with simplified configuration
    // TODO: Transform simplified config to full service creation request
    tracing::info!(offering = %payload.name, "Planting offering (simplified)");

    let mut details = HashMap::new();
    details.insert("offering".to_string(), serde_json::json!(payload.name));
    Err(error_response(
        StatusCode::NOT_IMPLEMENTED,
        "NOT_IMPLEMENTED",
        "Offering planting not yet implemented - use POST /api/v1/services for now".to_string(),
        Some(details),
    ))
}

/// DELETE /api/v1/offerings/:name
/// Take away an offering (uninstall)
pub async fn take_away_offering_v1(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<
    (StatusCode, Json<serde_json::Value>),
    (StatusCode, Json<garden_common::api_utils::ApiErrorResponse>),
> {
    use axum::http::HeaderMap;
    // Forward to services delete
    let result =
        crate::api::v1::services::delete_service_v1(State(state), Path(name), HeaderMap::new())
            .await;
    match result {
        Ok(Json(response)) => Ok((StatusCode::OK, Json(serde_json::json!(response.data)))),
        Err((status, error)) => Err((status, error)),
    }
}

/// POST /api/v1/offerings:heal
/// Heal the garden by discovering and adopting orphaned containers
#[derive(Debug, Deserialize)]
pub struct HealRequest {
    #[serde(default)]
    pub drop_invalid: bool,
}

pub async fn heal_garden_v1(
    State(state): State<AppState>,
    Json(payload): Json<HealRequest>,
) -> Result<
    (StatusCode, Json<serde_json::Value>),
    (StatusCode, Json<garden_common::api_utils::ApiErrorResponse>),
> {
    // Forward to services reconcile (same operation, zen terminology)
    crate::api::v1::services::reconcile_inventory_v1(
        State(state),
        Json(crate::api::v1::services::ReconcileRequest {
            drop_invalid: payload.drop_invalid,
        }),
    )
    .await
}

/// POST /api/v1/offerings:refresh
/// Refresh the offerings catalog from disk
pub async fn refresh_catalog_v1(
    State(state): State<AppState>,
) -> Result<
    (StatusCode, Json<serde_json::Value>),
    (StatusCode, Json<garden_common::api_utils::ApiErrorResponse>),
> {
    // Rebuild offerings index
    crate::ensure_offerings_index(&state, true, &crate::infra::persistence::OsOfferingsCache)
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Failed to rebuild offerings catalog");
            let mut details = HashMap::new();
            details.insert("error".to_string(), serde_json::json!(format!("{}", e)));
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                garden_common::constants::INTERNAL_ERROR,
                "Failed to rebuild offerings catalog".to_string(),
                Some(details),
            )
        })?;

    let idx_guard = state.offerings_index.read().await;
    let idx = idx_guard.as_ref().ok_or_else(|| {
        internal(
            garden_common::constants::INTERNAL_ERROR,
            "Offerings catalog unavailable after rebuild",
        )
    })?;

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "refreshed",
            "count": idx.offerings.len(),
            "fingerprint": idx.fingerprint,
            "generated_at": idx.generated_at,
        })),
    ))
}

// Helper functions

fn simplify_health(status: &garden_common::OfferingStatus) -> String {
    use garden_common::{constants, OfferingStatus};
    match status {
        OfferingStatus::Running => constants::HEALTH_HEALTHY.to_string(),
        OfferingStatus::Stopped | OfferingStatus::Unknown => {
            constants::HEALTH_UNHEALTHY.to_string()
        }
        OfferingStatus::Maintenance | OfferingStatus::Degraded => {
            constants::HEALTH_DEGRADED.to_string()
        }
        OfferingStatus::Installing => constants::HEALTH_INSTALLING.to_string(),
    }
}

// ============================================================================
// Image Inspection (OFFER-0006 — image-direct)
// ============================================================================

/// Query parameters for image inspection
#[derive(Debug, Deserialize)]
pub struct InspectQuery {
    /// Docker image reference (e.g., "nginx:latest", "mongo:7")
    pub image: String,
}

/// GET /api/v1/stone/offerings/inspect?image={ref}
/// Inspect a Docker image without deploying. Returns OCI metadata and
/// curated collision advisory.
pub async fn inspect_image_v1(
    State(state): State<AppState>,
    Query(query): Query<InspectQuery>,
) -> Result<
    (StatusCode, Json<serde_json::Value>),
    (StatusCode, Json<garden_common::api_utils::ApiErrorResponse>),
> {
    let image_ref = query.image.trim();
    if image_ref.is_empty() {
        return Err(bad_request(
            "INVALID_IMAGE_REF",
            "Image reference cannot be empty",
        ));
    }

    let inspection = crate::infra::image_inspect::inspect_image(&state.platform.docker, image_ref)
        .await
        .map_err(|e| {
            bad_request(
                "IMAGE_INSPECT_FAILED",
                format!("Failed to inspect image '{}': {}", image_ref, e),
            )
        })?;

    // Check for curated alternative
    let curated = {
        let idx_guard = state.offerings_index.read().await;
        idx_guard.as_ref().and_then(|idx| {
            crate::domain::offering_resolution::check_curated_collision(image_ref, &idx.offerings)
        })
    };

    let curated_json = curated.map(|alt| {
        serde_json::json!({
            "offering_name": alt.offering_name,
            "description": alt.description,
            "has_compatibility": alt.has_compatibility,
            "has_guidance": alt.has_guidance,
            "has_health_check": alt.has_health_check,
        })
    });

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "image": inspection.image_ref,
            "exposed_ports": inspection.exposed_ports,
            "volumes": inspection.volumes,
            "environment": inspection.environment.iter()
                .filter(|e| !e.starts_with("PATH="))
                .collect::<Vec<_>>(),
            "command": inspection.command,
            "labels": inspection.labels,
            "healthcheck": inspection.healthcheck.as_ref().map(|h| serde_json::json!({
                "test": h.test,
                "interval_ns": h.interval_ns,
                "timeout_ns": h.timeout_ns,
                "retries": h.retries,
            })),
            "architecture": inspection.architecture,
            "curated_alternative": curated_json,
        })),
    ))
}

// ============================================================================
// Offering Search (moved from Rake - Moss does all search logic)
// ============================================================================

/// Query parameters for search endpoint
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    /// Free-form search query (e.g., "nosql database", "vector store")
    pub q: String,
    /// Optional hardware preferences (comma-separated, e.g., "ssd,nvme")
    #[serde(default)]
    pub prefer: Option<String>,
    /// Maximum results to return (default: 5)
    #[serde(default)]
    pub limit: Option<usize>,
}

/// GET /api/v1/offerings/search?q={query}&prefer={prefs}&limit={n}
/// Search offerings using taxonomy dictionary and relevance scoring.
/// All search logic runs server-side; Rake is a thin client.
pub async fn search_offerings_v1(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<
    (StatusCode, Json<ApiResponse<OfferingSearchResponse>>),
    (StatusCode, Json<garden_common::api_utils::ApiErrorResponse>),
> {
    let dict = load_taxonomy_dictionary();
    let tokens = normalize_tokens(&query.q, &dict);

    if tokens.is_empty() {
        let mut details = HashMap::new();
        details.insert("query".to_string(), serde_json::json!(query.q));
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            garden_common::constants::INVALID_REQUEST,
            "Search query is empty after normalization".to_string(),
            Some(details),
        ));
    }

    // Parse prefer preferences (reserved for future stone hardware preference scoring)
    let _prefer: Vec<String> = query
        .prefer
        .as_ref()
        .map(|p| {
            p.split(',')
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let limit = query.limit.unwrap_or(5).min(50); // Cap at 50

    // Get offerings from index
    let idx_guard = state.offerings_index.read().await;
    let offerings_index = idx_guard
        .as_ref()
        .ok_or_else(|| unavailable("INDEX_UNAVAILABLE", "Offerings catalog not yet loaded"))?;

    let total_offerings = offerings_index.offerings.len();

    // Score and rank offerings
    let mut ranked: Vec<(i32, &crate::domain::offerings::CompiledOffering)> = offerings_index
        .offerings
        .iter()
        .filter(|o| o.compatibility.decision.as_str() != garden_common::constants::COMPAT_FAIL)
        .map(|o| {
            let score = offering_relevance_score(&tokens, o);
            (score, o)
        })
        .filter(|(s, _)| *s > 0)
        .collect();

    ranked.sort_by(|(sa, a), (sb, b)| sb.cmp(sa).then_with(|| a.name.cmp(&b.name)));

    // Convert to response format
    let results: Vec<OfferingSearchResult> = ranked
        .into_iter()
        .take(limit)
        .map(|(score, o)| OfferingSearchResult {
            name: o.name.clone(),
            category: o.category.clone(),
            description: o.description.clone(),
            tags: o.tags.clone(),
            image: o.image.clone(),
            score,
            compatibility: o.compatibility.decision.clone(),
            compatibility_reason: o.compatibility.reason.clone(),
        })
        .collect();

    Ok((
        StatusCode::OK,
        Json(ApiResponse::new(OfferingSearchResponse {
            query: query.q,
            tokens,
            results,
            total_offerings,
        })),
    ))
}

/// Load taxonomy dictionary from embedded manifests or filesystem.
fn load_taxonomy_dictionary() -> TaxonomyDictionary {
    // Try filesystem first (overlay pattern)
    let data_dir = std::path::PathBuf::from(garden_common::constants::paths::data_dir());
    let fs_path = data_dir.join("manifests").join("taxonomy.dictionary.yaml");

    if fs_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&fs_path) {
            if let Ok(dict) = serde_yaml::from_str::<TaxonomyDictionary>(&content) {
                tracing::debug!("Loaded taxonomy dictionary from filesystem: {:?}", fs_path);
                return dict;
            }
        }
    }

    // Fall back to embedded
    if let Some(content) = EmbeddedManifests::get_string("taxonomy.dictionary.yaml") {
        if let Ok(dict) = serde_yaml::from_str::<TaxonomyDictionary>(&content) {
            tracing::debug!("Loaded taxonomy dictionary from embedded assets");
            return dict;
        }
    }

    tracing::warn!("Failed to load taxonomy dictionary, using empty");
    TaxonomyDictionary::default()
}

/// Normalize search tokens using taxonomy dictionary.
/// Splits query, lowercases, and maps synonyms (e.g., "nosql" → "mongodb").
fn normalize_tokens(raw: &str, dict: &TaxonomyDictionary) -> Vec<String> {
    raw.split([',', ' ', '\t', '\n', '\r'])
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .map(|t| dict.map.get(&t).cloned().unwrap_or(t))
        .collect()
}

/// Check if a token matches a category (with intent mapping).
fn token_matches_category(token: &str, category: &str) -> bool {
    let token = token.to_lowercase();
    let category = category.to_lowercase();

    match token.as_str() {
        // User intent → canonical category
        "database" => matches!(category.as_str(), "data" | "cache" | "search" | "vector"),
        "vector" => category == "vector",
        "messaging" => category == "messaging",
        "observability" => category == "observability",
        "secrets" => category == "secrets",
        "cache" => category == "cache",
        "search" => category == "search",
        // Direct category match
        _ => token == category,
    }
}

/// Calculate relevance score for an offering against search tokens.
fn offering_relevance_score(
    tokens: &[String],
    offering: &crate::domain::offerings::CompiledOffering,
) -> i32 {
    let name_lc = offering.name.to_lowercase();
    let desc_lc = offering.description.to_lowercase();
    let tags_lc: HashSet<String> = offering.tags.iter().map(|t| t.to_lowercase()).collect();

    let mut score = 0i32;
    for token in tokens {
        let t = token.as_str();
        if token_matches_category(t, &offering.category) {
            score += 10;
        }
        if tags_lc.contains(t) {
            score += 6;
        }
        if name_lc == t {
            score += 8;
        } else if name_lc.contains(t) {
            score += 2;
        }
        if desc_lc.contains(t) {
            score += 1;
        }
    }
    score
}

// ============================================================================
// Manifest Authoring (OFFER-0006 Phase 2)
// ============================================================================

/// Request body for test-deploying a manifest.
#[derive(Debug, Deserialize)]
pub struct ManifestTestRequest {
    /// Offering name for the test deployment.
    pub name: String,
    /// Raw snippet YAML content.
    pub snippet_yaml: String,
    /// Optional frontmatter JSON content.
    pub frontmatter_json: Option<String>,
    /// Optional compatibility YAML content.
    pub compatibility_yaml: Option<String>,
}

/// POST /api/v1/stone/manifests/test
///
/// Validate and test-deploy a manifest from raw content. Parses the snippet,
/// builds a temporary Offering, and deploys via the standard pipeline.
pub async fn test_manifest_v1(
    State(state): State<AppState>,
    Json(payload): Json<ManifestTestRequest>,
) -> Result<
    (StatusCode, Json<serde_json::Value>),
    (StatusCode, Json<garden_common::api_utils::ApiErrorResponse>),
> {
    use garden_common::manifests::validation;

    // Validate snippet content
    let findings = validation::validate_snippet(&payload.snippet_yaml, "snippet.yaml");
    let errors: Vec<_> = findings
        .iter()
        .filter(|f| f.severity == validation::Severity::Error)
        .collect();

    if !errors.is_empty() {
        let error_msgs: Vec<String> = errors
            .iter()
            .map(|e| format!("[{}] {}", e.code, e.message))
            .collect();
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "MANIFEST_VALIDATION_FAILED",
            format!(
                "Manifest has {} error(s): {}",
                errors.len(),
                error_msgs.join("; ")
            ),
            Some({
                let mut details = HashMap::new();
                details.insert(
                    "findings".to_string(),
                    serde_json::to_value(&findings).unwrap_or_default(),
                );
                details
            }),
        ));
    }

    // Validate frontmatter if provided
    if let Some(ref fm) = payload.frontmatter_json {
        let fm_findings = validation::validate_frontmatter(fm, "frontmatter.json");
        let fm_errors: Vec<_> = fm_findings
            .iter()
            .filter(|f| f.severity == validation::Severity::Error)
            .collect();
        if !fm_errors.is_empty() {
            let error_msgs: Vec<String> = fm_errors
                .iter()
                .map(|e| format!("[{}] {}", e.code, e.message))
                .collect();
            return Err(bad_request(
                "MANIFEST_VALIDATION_FAILED",
                format!(
                    "Frontmatter has {} error(s): {}",
                    fm_errors.len(),
                    error_msgs.join("; ")
                ),
            ));
        }
    }

    // Extract image reference from snippet YAML to deploy via image-direct
    let offering_name = payload.name.trim().to_string();
    if offering_name.is_empty() {
        return Err(bad_request(
            "INVALID_OFFERING_NAME",
            "Offering name cannot be empty",
        ));
    }

    // Parse snippet to extract the image field
    let snippet_value: serde_yaml::Value =
        serde_yaml::from_str(&payload.snippet_yaml).map_err(|e| {
            bad_request(
                "INVALID_SNIPPET",
                format!("Failed to parse snippet YAML: {}", e),
            )
        })?;

    let image_ref = snippet_value
        .get("image")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            bad_request(
                "MISSING_IMAGE",
                "Snippet YAML must contain an 'image' field",
            )
        })?
        .to_string();

    // Deploy via image-direct using the existing create pipeline.
    // The image-direct path handles container creation, port mapping, etc.
    let fqn_str = format!("image:{}", image_ref);
    let create_req = crate::api::responses::CreateServiceRequest { offering: fqn_str };

    let result = crate::api::v1::services::create_service_v1(
        axum::extract::State(state.clone()),
        axum::http::HeaderMap::new(),
        axum::Json(create_req),
    )
    .await;

    match result {
        Ok(Json(resp)) => Ok((
            StatusCode::OK,
            Json(serde_json::json!({
                "name": offering_name,
                "status": resp.data.status,
                "service": resp.data.service,
                "message": resp.data.message,
            })),
        )),
        Err((status, err)) => Err((status, err)),
    }
}

/// GET /api/v1/stone/offerings/{name}/export
///
/// Export all manifest files for an offering as a JSON envelope.
/// Works for both curated offerings (from registry) and image-direct
/// offerings (synthesized from running container).
pub async fn export_offering_manifest_v1(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<
    (StatusCode, Json<serde_json::Value>),
    (StatusCode, Json<garden_common::api_utils::ApiErrorResponse>),
> {
    let offering_fqn = OfferingFqn::parse(&name).map_err(|e| {
        bad_request(
            "INVALID_OFFERING_NAME",
            format!("Invalid offering name '{}': {}", name, e),
        )
    })?;

    // Try curated manifest first
    if let Some(offering) = state.manifest_registry.sw.get(&offering_fqn.offering) {
        let snippet_yaml = offering
            .managed
            .as_ref()
            .map(|m| m.snippet_yaml.clone())
            .unwrap_or_default();

        let frontmatter_json = serde_json::to_string_pretty(&serde_json::json!({
            "name": offering.name,
            "description": offering.metadata.description,
            "category": offering.category,
            "tags": offering.metadata.tags,
        }))
        .unwrap_or_default();

        let compatibility_yaml = offering
            .compatibility
            .as_ref()
            .map(|c| serde_yaml::to_string(c).unwrap_or_default())
            .unwrap_or_default();

        let guidance_md = offering.guidance.clone().unwrap_or_default();

        return Ok((
            StatusCode::OK,
            Json(serde_json::json!({
                "name": offering.name,
                "category": offering.category,
                "snippet_yaml": snippet_yaml,
                "frontmatter_json": frontmatter_json,
                "compatibility_yaml": compatibility_yaml,
                "guidance_md": guidance_md,
            })),
        ));
    }

    // Try image-direct: inspect the running container and synthesize
    let offerings = state.offerings.read().await;
    let running = offerings
        .iter()
        .find(|o| o.name.fqn() == offering_fqn.fqn());

    if let Some(offering_entry) = running {
        // If it's an image-direct offering, use the image ref to inspect
        if let Some(image_ref) = &offering_entry.name.image_ref {
            match crate::infra::image_inspect::inspect_image(&state.platform.docker, image_ref)
                .await
            {
                Ok(inspection) => {
                    let inspection_json = serde_json::to_value(&inspection).unwrap_or_default();
                    match garden_common::manifests::generate::generate_from_inspection(
                        Some(&offering_fqn.offering),
                        None,
                        &inspection_json,
                    ) {
                        Ok(generated) => {
                            return Ok((
                                StatusCode::OK,
                                Json(serde_json::json!({
                                    "name": generated.name,
                                    "category": "custom",
                                    "snippet_yaml": generated.snippet_yaml,
                                    "frontmatter_json": generated.frontmatter_json,
                                    "compatibility_yaml": generated.compatibility_yaml,
                                    "guidance_md": generated.guidance_md,
                                })),
                            ));
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "Failed to generate manifest for export");
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, image = %image_ref, "Failed to inspect image for export");
                }
            }
        }
    }

    let mut details = HashMap::new();
    details.insert("name".to_string(), serde_json::json!(name));
    Err(error_response(
        StatusCode::NOT_FOUND,
        garden_common::constants::TEMPLATE_NOT_FOUND,
        format!(
            "Offering '{}' not found in registry or running services",
            name
        ),
        Some(details),
    ))
}
