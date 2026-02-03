// Offerings API - Human Layer
// 
// Purpose: Simplified, beginner-friendly API for managing offerings
// Target audience: 90% of users - scripters, beginners, simple automation
// Philosophy: Hide Docker complexity, provide safety rails, optimize for common case

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use crate::api::responses::ApiResponse;
use garden_common::offerings::{
    OfferingSearchResponse, OfferingSearchResult, TaxonomyDictionary,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::{error_codes, error_response, AppState};
use crate::infra::embedded::EmbeddedManifests;

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
) -> Result<(StatusCode, Json<ApiResponse<Vec<OfferingView>>>), (StatusCode, Json<garden_common::api_utils::ApiErrorResponse>)> {
    // Get installed services from unified offerings registry
    let offerings_guard = state.offerings.read().await;
    let installed: HashMap<String, &garden_common::Offering> = offerings_guard
        .iter()
        .map(|o| (o.name.clone(), o))
        .collect();

    // Get available offerings from index (may still be building)
    let idx_guard = state.offerings_index.read().await;
    let offerings_index = idx_guard.as_ref();
    let catalog_building = offerings_index.is_none();

    let mut offerings: Vec<OfferingView> = Vec::new();

    // Add installed offerings with runtime details
    if query.state.as_deref() != Some("available") {
        for offering in offerings_guard.iter() {
            let image = state.docker.get_service_image(&offering.name).await.unwrap_or_else(|_| "<unknown>".to_string());
            offerings.push(OfferingView {
                name: offering.name.clone(),
                state: "installed".to_string(),
                category: offering.offering.clone(),
                description: format!("{} service", offering.offering),
                tags: vec![],
                image,
                compatibility: None,
                health: Some(simplify_health(&offering.status)),
                uptime: None, // TODO: Track uptime in Offering
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
        Some(vec!["Catalog still building - available offerings may be incomplete".to_string()])
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
) -> Result<(StatusCode, Json<ApiResponse<serde_json::Value>>), (StatusCode, Json<garden_common::api_utils::ApiErrorResponse>)> {
    // Check if installed
    let offerings_guard = state.offerings.read().await;
    if let Some(offering) = offerings_guard.iter().find(|o| o.name == name) {
        return Ok((
            StatusCode::OK,
            Json(ApiResponse {
                data: serde_json::json!({
                    "name": offering.name,
                    "state": "installed",
                    "category": offering.offering,
                    "health": simplify_health(&offering.status),
                    "version": offering.version,
                }),
                suggestions: None,
            }),
        ));
    }
    
    // Check if available
    let idx_guard = state.offerings_index.read().await;
    let offerings_index = idx_guard.as_ref().ok_or_else(|| {
        error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "INDEX_UNAVAILABLE",
            "Offerings catalog not yet loaded".to_string(),
            None,
        )
    })?;
    
    if let Some(offering) = offerings_index.offerings.iter().find(|o| o.name == name) {
        return Ok((
            StatusCode::OK,
            Json(ApiResponse {
                data: serde_json::json!({
                    "name": offering.name,
                    "state": "available",
                    "category": offering.category,
                    "description": offering.description,
                    "tags": offering.tags,
                    "compatibility": {
                        "decision": offering.compatibility.decision.to_string(),
                        "reason": offering.compatibility.reason,
                    },
                }),
                suggestions: None,
            }),
        ));
    }
    
    // Not found
    let mut details = HashMap::new();
    details.insert("name".to_string(), serde_json::json!(name));
    Err(error_response(
        StatusCode::NOT_FOUND,
        error_codes::OFFERING_NOT_FOUND,
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
    match state.manifest_registry.sw.get(&name) {
        Some(entry) => {
            let yaml = entry.managed.as_ref()
                .map(|m| m.snippet_yaml.clone())
                .unwrap_or_default();
            Ok((StatusCode::OK, yaml))
        }
        None => {
            let mut details = HashMap::new();
            details.insert("name".to_string(), serde_json::json!(name));
            Err(error_response(
                StatusCode::NOT_FOUND,
                error_codes::TEMPLATE_NOT_FOUND,
                format!("Manifest for '{}' not found", name),
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
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<garden_common::api_utils::ApiErrorResponse>)> {
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
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<garden_common::api_utils::ApiErrorResponse>)> {
    use axum::http::HeaderMap;
    // Forward to services delete
    let result = crate::api::v1::services::delete_service_v1(State(state), Path(name), HeaderMap::new()).await;
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
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<garden_common::api_utils::ApiErrorResponse>)> {
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
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<garden_common::api_utils::ApiErrorResponse>)> {
    // Rebuild offerings index
    crate::ensure_offerings_index(&state, true).await.map_err(|e| {
        tracing::error!(error = ?e, "Failed to rebuild offerings catalog");
        let mut details = HashMap::new();
        details.insert("error".to_string(), serde_json::json!(format!("{}", e)));
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            error_codes::INTERNAL_ERROR,
            "Failed to rebuild offerings catalog".to_string(),
            Some(details),
        )
    })?;
    
    let idx_guard = state.offerings_index.read().await;
    let idx = idx_guard.as_ref().ok_or_else(|| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            error_codes::INTERNAL_ERROR,
            "Offerings catalog unavailable after rebuild".to_string(),
            None,
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
    use garden_common::{OfferingStatus, constants};
    match status {
        OfferingStatus::Running => constants::HEALTH_HEALTHY.to_string(),
        OfferingStatus::Stopped | OfferingStatus::Unknown => constants::HEALTH_UNHEALTHY.to_string(),
        OfferingStatus::Maintenance | OfferingStatus::Degraded => constants::HEALTH_DEGRADED.to_string(),
        OfferingStatus::Installing => constants::HEALTH_INSTALLING.to_string(),
    }
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
) -> Result<(StatusCode, Json<ApiResponse<OfferingSearchResponse>>), (StatusCode, Json<garden_common::api_utils::ApiErrorResponse>)> {
    let dict = load_taxonomy_dictionary();
    let tokens = normalize_tokens(&query.q, &dict);
    
    if tokens.is_empty() {
        let mut details = HashMap::new();
        details.insert("query".to_string(), serde_json::json!(query.q));
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            error_codes::INVALID_REQUEST,
            "Search query is empty after normalization".to_string(),
            Some(details),
        ));
    }
    
    // Parse prefer preferences (reserved for future stone hardware preference scoring)
    let _prefer: Vec<String> = query
        .prefer
        .as_ref()
        .map(|p| p.split(',').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    
    let limit = query.limit.unwrap_or(5).min(50); // Cap at 50
    
    // Get offerings from index
    let idx_guard = state.offerings_index.read().await;
    let offerings_index = idx_guard.as_ref().ok_or_else(|| {
        error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "INDEX_UNAVAILABLE",
            "Offerings catalog not yet loaded".to_string(),
            None,
        )
    })?;
    
    let total_offerings = offerings_index.offerings.len();
    
    // Score and rank offerings
    let mut ranked: Vec<(i32, &crate::domain::offerings::CompiledOffering)> = offerings_index
        .offerings
        .iter()
        .filter(|o| o.compatibility.decision.as_str() != garden_common::COMPAT_FAIL)
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
        Json(ApiResponse {
            data: OfferingSearchResponse {
                query: query.q,
                tokens,
                results,
                total_offerings,
            },
            suggestions: None,
        }),
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
fn offering_relevance_score(tokens: &[String], offering: &crate::domain::offerings::CompiledOffering) -> i32 {
    let name_lc = offering.name.to_lowercase();
    let desc_lc = offering.description.to_lowercase();
    let tags_lc: HashSet<String> = offering
        .tags
        .iter()
        .map(|t| t.to_lowercase())
        .collect();

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

