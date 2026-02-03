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
    CapabilityCollection, OfferingMode, ServiceInfo, ServiceStatus, Ports, UnifiedOffering,
};
use serde::{Deserialize, Serialize};

use crate::api::responses::ApiResponse;
use crate::domain::{CapabilityExecutor, get_offering_port};
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
    // Find the offering in unified registry
    let (offering, mode) = {
        let offerings = state.offerings.read().await;
        let found = offerings
            .iter()
            .find(|o| o.offering.to_lowercase() == offering_name.to_lowercase())
            .cloned();

        match found {
            Some(o) => {
                let mode = o.mode();
                (o, mode)
            }
            None => {
                return Err(error_response(
                    StatusCode::NOT_FOUND,
                    "OFFERING_NOT_FOUND",
                    format!("Offering '{}' is not running on this stone. Use 'rake list' or 'rake adopted' to see offerings.", offering_name),
                    None,
                ));
            }
        }
    };

    // Convert to ServiceInfo for the capability executor (which still uses ServiceInfo)
    let service = offering_to_service_info(&offering, &state).await;

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

    // Update offering sub_capabilities in registry (lightweight format)
    if !capabilities.is_empty() {
        let sub_caps: Vec<_> = capabilities.iter().map(|c| c.to_sub_capability()).collect();

        // Update in unified registry
        {
            let mut offerings = state.offerings.write().await;
            if let Some(o) = offerings.iter_mut().find(|o| o.offering_id == offering.offering_id) {
                o.sub_capabilities = sub_caps;
            }
        }
        if let Err(e) = state.persist_offerings().await {
            tracing::error!(error = ?e, "Failed to persist offerings after capability discovery");
        }
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


/// Request body for adding a capability
#[derive(Debug, Deserialize)]
pub struct AddCapabilityRequest {
    /// Capability name (e.g., "llama2:7b" for Ollama models)
    pub name: String,

    /// Capability type (optional, defaults to first capability type in manifest)
    #[serde(rename = "type")]
    pub cap_type: Option<String>,
}

/// Response for capability mutations (add/remove)
#[derive(Debug, Serialize)]
pub struct CapabilityMutationResponse {
    /// Whether the operation succeeded
    pub success: bool,

    /// The capability name
    pub capability: String,

    /// The operation performed
    pub operation: String,

    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// POST /api/v1/stone/offerings/:name/capabilities
///
/// Add a capability to an offering (e.g., pull a model for Ollama).
///
/// # Path Parameters
/// - `name`: Offering name (e.g., "ollama")
///
/// # Request Body
/// ```json
/// {
///   "name": "llama2:7b",
///   "type": "model"  // optional
/// }
/// ```
///
/// # Response
/// ```json
/// {
///   "data": {
///     "success": true,
///     "capability": "llama2:7b",
///     "operation": "add"
///   }
/// }
/// ```
pub async fn add_offering_capability_v1(
    State(state): State<AppState>,
    Path(offering_name): Path<String>,
    Json(request): Json<AddCapabilityRequest>,
) -> Result<Json<ApiResponse<CapabilityMutationResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    // Find the service (managed or adopted)
    let (service, mode) = find_service_for_capability(&state, &offering_name).await?;

    // Get capability manifest
    let manifest = get_capability_manifest(&service.offering).ok_or_else(|| {
        error_response(
            StatusCode::NOT_FOUND,
            "NO_CAPABILITY_MANIFEST",
            format!("No capability manifest found for offering '{}'.", service.offering),
            None,
        )
    })?;

    // Determine capability type
    let cap_type = request.cap_type.as_deref().unwrap_or_else(|| {
        manifest.capabilities.first()
            .map(|c| c.cap_type.as_str())
            .unwrap_or("model")
    });

    // Find the capability definition
    let cap_def = manifest.capabilities.iter()
        .find(|c| c.cap_type == cap_type)
        .ok_or_else(|| {
            error_response(
                StatusCode::BAD_REQUEST,
                "UNKNOWN_CAPABILITY_TYPE",
                format!("Capability type '{}' not found in manifest for '{}'.", cap_type, service.offering),
                None,
            )
        })?;

    // Check if add operation is available
    if cap_def.add.as_ref().map(|a| !a.available).unwrap_or(true) {
        return Err(error_response(
            StatusCode::NOT_IMPLEMENTED,
            "ADD_NOT_SUPPORTED",
            format!("Adding capabilities of type '{}' is not supported for '{}'.", cap_type, service.offering),
            None,
        ));
    }

    // Execute add operation
    let executor = CapabilityExecutor::new();
    let result = executor
        .add_capability(&service, &manifest, mode, cap_type, &request.name)
        .await
        .map_err(|e| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "ADD_FAILED",
                format!("Failed to add capability: {}", e),
                None,
            )
        })?;

    Ok(Json(ApiResponse {
        data: CapabilityMutationResponse {
            success: result.success,
            capability: result.capability,
            operation: result.operation,
            error: result.error,
        },
        suggestions: None,
    }))
}

/// DELETE /api/v1/stone/offerings/:name/capabilities/:capability
///
/// Remove a capability from an offering (e.g., delete a model from Ollama).
///
/// # Path Parameters
/// - `name`: Offering name (e.g., "ollama")
/// - `capability`: Capability name to remove (e.g., "llama2:7b")
///
/// # Query Parameters
/// - `type`: Capability type (optional, defaults to first type in manifest)
///
/// # Response
/// ```json
/// {
///   "data": {
///     "success": true,
///     "capability": "llama2:7b",
///     "operation": "remove"
///   }
/// }
/// ```
pub async fn remove_offering_capability_v1(
    State(state): State<AppState>,
    Path((offering_name, capability_name)): Path<(String, String)>,
    axum::extract::Query(query): axum::extract::Query<RemoveCapabilityQuery>,
) -> Result<Json<ApiResponse<CapabilityMutationResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    // Find the service (managed or adopted)
    let (service, mode) = find_service_for_capability(&state, &offering_name).await?;

    // Get capability manifest
    let manifest = get_capability_manifest(&service.offering).ok_or_else(|| {
        error_response(
            StatusCode::NOT_FOUND,
            "NO_CAPABILITY_MANIFEST",
            format!("No capability manifest found for offering '{}'.", service.offering),
            None,
        )
    })?;

    // Determine capability type
    let cap_type = query.cap_type.as_deref().unwrap_or_else(|| {
        manifest.capabilities.first()
            .map(|c| c.cap_type.as_str())
            .unwrap_or("model")
    });

    // Find the capability definition
    let cap_def = manifest.capabilities.iter()
        .find(|c| c.cap_type == cap_type)
        .ok_or_else(|| {
            error_response(
                StatusCode::BAD_REQUEST,
                "UNKNOWN_CAPABILITY_TYPE",
                format!("Capability type '{}' not found in manifest for '{}'.", cap_type, service.offering),
                None,
            )
        })?;

    // Check if remove operation is available
    if cap_def.remove.as_ref().map(|r| !r.available).unwrap_or(true) {
        return Err(error_response(
            StatusCode::NOT_IMPLEMENTED,
            "REMOVE_NOT_SUPPORTED",
            format!("Removing capabilities of type '{}' is not supported for '{}'.", cap_type, service.offering),
            None,
        ));
    }

    // Execute remove operation
    let executor = CapabilityExecutor::new();
    let result = executor
        .remove_capability(&service, &manifest, mode, cap_type, &capability_name)
        .await
        .map_err(|e| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "REMOVE_FAILED",
                format!("Failed to remove capability: {}", e),
                None,
            )
        })?;

    Ok(Json(ApiResponse {
        data: CapabilityMutationResponse {
            success: result.success,
            capability: result.capability,
            operation: result.operation,
            error: result.error,
        },
        suggestions: None,
    }))
}

/// Query parameters for remove capability endpoint
#[derive(Debug, Deserialize)]
pub struct RemoveCapabilityQuery {
    /// Capability type (optional)
    #[serde(rename = "type")]
    pub cap_type: Option<String>,
}

/// Convert UnifiedOffering to ServiceInfo for capability executor compatibility
async fn offering_to_service_info(offering: &UnifiedOffering, state: &AppState) -> ServiceInfo {
    // Use location port, falling back to manifest default
    let port = if offering.location.port > 0 {
        offering.location.port
    } else {
        get_offering_port(&offering.offering, state).await
    };

    ServiceInfo {
        offering_id: offering.offering_id.clone(),
        name: offering.name.clone(),
        offering: offering.offering.clone(),
        version: offering.version.clone(),
        status: match offering.status {
            garden_common::OfferingStatus::Running => ServiceStatus::Running,
            garden_common::OfferingStatus::Stopped => ServiceStatus::Stopped,
            garden_common::OfferingStatus::Installing => ServiceStatus::Installing,
            garden_common::OfferingStatus::Degraded => ServiceStatus::Degraded,
            garden_common::OfferingStatus::Maintenance => ServiceStatus::Maintenance,
            garden_common::OfferingStatus::Unknown => ServiceStatus::Unknown,
        },
        health: offering.health.clone(),
        ports: Ports {
            native: port,
            agnostic: offering.location.agnostic_port,
        },
        resources: offering.managed_data().and_then(|m| m.resources.clone()),
        job_id: offering.managed_data().and_then(|m| m.job_id.clone()),
        sub_capabilities: offering.sub_capabilities.clone(),
        guidance: offering.managed_data().and_then(|m| m.guidance.clone()),
    }
}

/// Helper to find a service (managed or adopted) for capability operations
async fn find_service_for_capability(
    state: &AppState,
    offering_name: &str,
) -> Result<(ServiceInfo, OfferingMode), (StatusCode, Json<ApiErrorResponse>)> {
    // Find in unified registry
    let offerings = state.offerings.read().await;
    let found = offerings
        .iter()
        .find(|o| o.offering.to_lowercase() == offering_name.to_lowercase())
        .cloned();
    drop(offerings);

    match found {
        Some(offering) => {
            let mode = offering.mode();
            let service = offering_to_service_info(&offering, state).await;
            Ok((service, mode))
        }
        None => {
            Err(error_response(
                StatusCode::NOT_FOUND,
                "OFFERING_NOT_FOUND",
                format!("Offering '{}' is not running on this stone.", offering_name),
                None,
            ))
        }
    }
}
