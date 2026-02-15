//! Garden stones endpoints — aggregated topology with enrichment

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use garden_common::api_utils::ApiErrorResponse;
use serde::Serialize;
use serde_json::Value;

use crate::api::responses::error_response;
use crate::domain::topology::StoneEnrichment;
use crate::AppState;

/// Combined stone view: TopologyEntry + enrichment data
#[derive(Serialize)]
struct StoneView {
    // From TopologyEntry
    stone_id: String,
    stone_name: String,
    endpoint: String,
    moss_version: String,
    health: String,
    status: String,
    discovered_at: String,
    last_seen: String,
    tags: Vec<String>,
    /// Deterministic HSL color for visual identity
    color: String,
    services: Vec<ServiceView>,
    // From enrichment
    #[serde(skip_serializing_if = "Option::is_none")]
    resources: Option<crate::domain::topology::EnrichedResources>,
    #[serde(skip_serializing_if = "Option::is_none")]
    capabilities: Option<Value>,
    offerings: Vec<crate::domain::topology::EnrichedOffering>,
    seed_banks: Vec<crate::domain::topology::EnrichedSeedBank>,
    companions: Vec<crate::domain::topology::EnrichedCompanion>,
}

#[derive(Serialize)]
struct ServiceView {
    offering_id: String,
    name: String,
    offering: String,
    category: String,
    status: String,
}

/// GET /api/v1/garden/stones — all stones with enrichment
pub async fn get_stones(State(state): State<AppState>) -> Json<Value> {
    let topology = state.topology.read().await;

    let stones: Vec<StoneView> = topology
        .stones
        .iter()
        .map(|(key, entry)| {
            let enrichment = topology.enrichment.get(key);
            build_stone_view(entry, enrichment)
        })
        .collect();

    Json(serde_json::to_value(&stones).unwrap())
}

/// GET /api/v1/garden/stones/:stone_id — single stone detail
pub async fn get_stone_detail(
    State(state): State<AppState>,
    Path(stone_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<ApiErrorResponse>)> {
    let topology = state.topology.read().await;

    // Search by stone_id or stone_name
    let found = topology.stones.iter().find(|(key, entry)| {
        key.as_str() == stone_id || entry.stone_id == stone_id || entry.stone_name == stone_id
    });

    match found {
        Some((key, entry)) => {
            let enrichment = topology.enrichment.get(key);
            let view = build_stone_view(entry, enrichment);
            Ok(Json(serde_json::to_value(&view).unwrap()))
        }
        None => Err(error_response(
            StatusCode::NOT_FOUND,
            "STONE_NOT_FOUND",
            format!("No stone found with id or name '{}'", stone_id),
        )),
    }
}

/// GET /api/v1/garden/topology — legacy compatibility, returns LanternTopology
pub async fn get_topology(State(state): State<AppState>) -> Json<Value> {
    let topology = state.topology.read().await;
    let lantern_topo = topology.to_lantern_topology();
    Json(serde_json::to_value(lantern_topo).unwrap())
}

fn build_stone_view(
    entry: &garden_common::types::topology::TopologyEntry,
    enrichment: Option<&StoneEnrichment>,
) -> StoneView {
    let empty = StoneEnrichment::default();
    let enrich = enrichment.unwrap_or(&empty);

    // Use enriched color from portrait, or derive from stone_id
    let color = enrich
        .color
        .clone()
        .unwrap_or_else(|| garden_common::utils::derive_stone_color(&entry.stone_id));

    StoneView {
        stone_id: entry.stone_id.clone(),
        stone_name: entry.stone_name.clone(),
        endpoint: entry.address.http_base(),
        moss_version: entry.moss_version.clone(),
        health: entry.health.clone(),
        status: format!("{:?}", entry.status).to_lowercase(),
        discovered_at: entry.discovered_at.to_rfc3339(),
        last_seen: entry.last_seen.to_rfc3339(),
        tags: entry.tags.clone(),
        color,
        services: entry
            .services
            .iter()
            .map(|svc| ServiceView {
                offering_id: svc.offering_id.clone(),
                name: svc.name.clone(),
                offering: svc.offering.clone(),
                category: svc.category.clone(),
                status: svc.status.clone(),
            })
            .collect(),
        resources: enrich.resources.clone(),
        capabilities: entry
            .capabilities
            .as_ref()
            .and_then(|c| serde_json::to_value(c).ok()),
        offerings: enrich.offerings.clone(),
        seed_banks: enrich.seed_banks.clone(),
        companions: enrich.companions.clone(),
    }
}
