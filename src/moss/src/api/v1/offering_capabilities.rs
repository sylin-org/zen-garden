//! Offering capabilities API endpoints
//!
//! Manifest-driven capability discovery for offerings.
//! Supports listing, adding, and removing capabilities like models, extensions, etc.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use garden_common::{
    api_utils::ApiErrorResponse,
    CapabilityCollection, OfferingMode, ServiceInfo, ServiceStatus, Ports,
};
use serde::{Deserialize, Serialize};

use crate::api::responses::ApiResponse;
use crate::domain::CapabilityExecutor;
use crate::infra::manifests::get_capability_manifest;
use crate::{error_response, AppState};

/// Response for capability listing
#[derive(Debug, Serialize)]
pub struct CapabilitiesResponse {
    /// Offering name
    pub offering: String,

    /// The offering mode (managed/adopted)
    pub mode: OfferingMode,

    /// Capability collections (one per type)
    pub capabilities: Vec<CapabilityCollection>,
}

/// Query parameters for capabilities endpoint
#[derive(Debug, Deserialize)]
pub struct CapabilitiesQuery {
    /// Force refresh (ignore cache)
    #[serde(default)]
    pub refresh: bool,
}

/// GET /api/v1/stone/offerings/:name/capabilities
///
/// List capabilities for an offering using manifest-defined commands.
/// Returns rich capability information including metadata.
///
/// # Path Parameters
/// - `name`: Offering name (e.g., "ollama")
///
/// # Query Parameters
/// - `refresh`: Force fresh discovery (default: false)
///
/// # Response
/// ```json
/// {
///   "data": {
///     "offering": "ollama",
///     "mode": "adopted",
///     "capabilities": [{
///       "type": "model",
///       "display": { "singular": "model", "plural": "models" },
///       "items": [
///         { "name": "llama2:7b", "size": "3.6 GB", "size_bytes": 3826793472 }
///       ],
///       "discovered_at": "2026-02-02T14:30:00Z"
///     }]
///   }
/// }
/// ```
pub async fn list_offering_capabilities_v1(
    State(state): State<AppState>,
    Path(offering_name): Path<String>,
) -> Result<Json<ApiResponse<CapabilitiesResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    // Try to find in managed registry first
    let (service, mode) = {
        let registry = state.registry.read().await;
        if let Some(svc) = registry
            .iter()
            .find(|s| s.offering.to_lowercase() == offering_name.to_lowercase())
            .cloned()
        {
            let mode = determine_offering_mode(&state, &svc.name).await;
            (svc, mode)
        } else {
            drop(registry);

            // Not in managed registry, check adopted offerings
            let adopted = state.adopted_offerings.read().await;
            if let Some(adopted_svc) = adopted
                .iter()
                .find(|a| a.offering.to_lowercase() == offering_name.to_lowercase())
            {
                // Convert AdoptedOfferingInfo to ServiceInfo for capability executor
                // Use location.port, falling back to default port from manifest
                let port = if adopted_svc.location.port > 0 {
                    adopted_svc.location.port
                } else {
                    // Get default port from manifest if available
                    get_default_port_for_offering(&adopted_svc.offering)
                };

                let service = ServiceInfo {
                    offering_id: String::new(),
                    name: adopted_svc.name.clone(),
                    offering: adopted_svc.offering.clone(),
                    version: adopted_svc.version.clone().unwrap_or_default(),
                    status: ServiceStatus::Running,
                    health: adopted_svc.health.clone(),
                    ports: Ports {
                        native: port,
                        agnostic: None,
                    },
                    resources: None,
                    job_id: None,
                    sub_capabilities: Vec::new(),
                    guidance: None,
                };
                (service, OfferingMode::Adopted)
            } else {
                drop(adopted);
                return Err(error_response(
                    StatusCode::NOT_FOUND,
                    "OFFERING_NOT_FOUND",
                    format!("Offering '{}' is not running on this stone. Use 'rake list' or 'rake adopted' to see offerings.", offering_name),
                    None,
                ));
            }
        }
    };

    // Get capability manifest for this offering
    let manifest = get_capability_manifest(&service.offering).ok_or_else(|| {
        error_response(
            StatusCode::NOT_FOUND,
            "NO_CAPABILITY_MANIFEST",
            format!("No capability manifest found for offering '{}'. This offering does not support capability discovery.", service.offering),
            None,
        )
    })?;

    // Execute capability discovery
    let executor = CapabilityExecutor::new();
    let capabilities = executor
        .list_capabilities(&service, manifest, mode)
        .await
        .map_err(|e| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DISCOVERY_FAILED",
                format!("Failed to discover capabilities: {}", e),
                None,
            )
        })?;

    // Update service sub_capabilities in registry (lightweight format)
    if !capabilities.is_empty() {
        let mut registry = state.registry.write().await;
        if let Some(svc) = registry.iter_mut().find(|s| s.name == service.name) {
            svc.sub_capabilities = capabilities
                .iter()
                .map(|c| c.to_sub_capability())
                .collect();
        }
        drop(registry);
        let _ = state.persist_registry().await;
    }

    Ok(Json(ApiResponse {
        data: CapabilitiesResponse {
            offering: service.offering.clone(),
            mode,
            capabilities,
        },
        suggestions: None,
    }))
}

/// Determine the offering mode for a service
///
/// Checks adopted registry first, then assumes managed.
async fn determine_offering_mode(state: &AppState, service_name: &str) -> OfferingMode {
    // Check if in adopted offerings
    let adopted = state.adopted_offerings.read().await;
    if adopted.iter().any(|a| a.name == service_name) {
        return OfferingMode::Adopted;
    }
    drop(adopted);

    // Check if in borrowed offerings
    let borrowed = state.borrowed_offerings.read().await;
    if borrowed.iter().any(|b| b.name == service_name) {
        return OfferingMode::Borrowed;
    }
    drop(borrowed);

    // Default to managed
    OfferingMode::Managed
}

/// Get default port for well-known offerings
///
/// Used when adopted offerings don't have an explicit port set.
fn get_default_port_for_offering(offering: &str) -> u16 {
    match offering.to_lowercase().as_str() {
        "ollama" => 11434,
        "postgresql" | "postgres" => 5432,
        "redis" => 6379,
        "mongodb" | "mongo" => 27017,
        "mysql" | "mariadb" => 3306,
        "elasticsearch" => 9200,
        "opensearch" => 9200,
        _ => 0,
    }
}
