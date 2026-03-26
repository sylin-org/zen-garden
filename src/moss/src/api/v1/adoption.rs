//! Adoption API endpoints
//!
//! Endpoints for managing adopted and borrowed offerings:
//! - List adoptable offerings (detected but not yet adopted)
//! - Adopt offerings manually
//! - List adopted/borrowed offerings
//! - Remove adopted/borrowed offerings

use crate::api::suggestions::{generate_suggestions, Suggestion};
use crate::domain::{connection, ConnectivityOrchestrator, ConnectivityStatus};
use crate::{bad_request, conflict, internal, not_found, AppState};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use garden_common::offerings::OfferingFqn;
use garden_common::utils::ids::generate_guidv7;
use garden_common::{
    AdoptedControlLevel, AdoptedData, BorrowedData, Offering, OfferingLocation, OfferingModeData,
    OfferingStatus, ServiceHealthStatus,
};
use serde::{Deserialize, Serialize};

/// GET /api/v1/offerings/adoptable - List offerings available for adoption
///
/// Returns list of services detected on the host that can be adopted.
/// These are services detected but not yet managed by Moss.
pub async fn list_adoptable_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> crate::api::ApiResult<Vec<AdoptableOffering>> {
    // Get offering manifests that support adopted mode
    let adoptable_manifests = state
        .manifest_registry
        .offerings_by_mode(&garden_common::OfferingMode::Adopted);

    let mut adoptable = Vec::new();

    for offering in adoptable_manifests {
        // Check if already adopted
        let already_adopted = {
            let offerings = state.offerings.read().await;
            offerings
                .iter()
                .any(|o| o.offering == offering.name && o.is_adopted())
        };

        if already_adopted {
            continue;
        }

        // Try detection (this will use cached results if available)
        let detector = std::sync::Arc::new(crate::infra::detection::ContainerDetector::new(
            state.platform.docker.clone(),
        ));
        let orchestrator = crate::domain::DetectionOrchestrator::new(detector);
        match orchestrator.detect(offering).await {
            Ok(result) if result.detected && result.stable => {
                adoptable.push(AdoptableOffering {
                    name: offering.name.clone(),
                    category: offering.category.clone(),
                    description: offering.description(),
                    version: result.version,
                    detection_method: "auto".to_string(), // Could track actual method used
                });
            }
            _ => {
                // Not detected or not stable yet
            }
        }
    }

    let ctx = Suggestion::from_headers(&headers, "list_adoptable");
    let suggestions = generate_suggestions(&ctx);

    crate::api::ok_maybe(adoptable, suggestions)
}

/// POST /api/v1/offerings/:offering/adopt - Manually adopt an offering
///
/// Attempts to detect and adopt a specific offering.
/// Returns the adopted offering info if successful.
pub async fn adopt_offering_v1(
    State(state): State<AppState>,
    Path(offering): Path<String>,
    headers: HeaderMap,
    Json(req): Json<AdoptOfferingRequest>,
) -> crate::api::ApiResult<Offering> {
    let offering_fqn = OfferingFqn::parse(&offering).map_err(|e| {
        bad_request(
            "INVALID_OFFERING_NAME",
            format!("Invalid offering name '{}': {}", offering, e),
        )
    })?;
    let offering_type = offering_fqn.offering.clone();
    let adopted_fqn = OfferingFqn::adopted(&offering_type).map_err(|e| {
        bad_request(
            "INVALID_OFFERING_NAME",
            format!("Invalid offering name '{}': {}", offering_type, e),
        )
    })?;

    // Check if already adopted
    {
        let offerings = state.offerings.read().await;
        if offerings
            .iter()
            .any(|o| o.offering == offering_type && o.is_adopted())
        {
            return Err(conflict(
                "ALREADY_ADOPTED",
                format!("Offering '{}' is already adopted", offering_type),
            ));
        }
    }

    // Find offering definition
    let offering_def = state
        .manifest_registry
        .get_offering(&offering_type)
        .ok_or_else(|| {
            not_found(
                "OFFERING_NOT_FOUND",
                format!("Offering '{}' not found", offering_type),
            )
        })?;

    // Verify offering supports adopted mode
    if !offering_def.supports_mode(&garden_common::OfferingMode::Adopted) {
        return Err(bad_request(
            "NOT_ADOPTABLE",
            format!("Offering '{}' does not support adopted mode", offering_type),
        ));
    }

    // Detect offering
    let detector = std::sync::Arc::new(
        crate::infra::detection::ContainerDetector::new(state.platform.docker.clone()),
    );
    let orchestrator = crate::domain::DetectionOrchestrator::new(detector);
    let detection_result = orchestrator
        .detect(offering_def)
        .await
        .map_err(|e| internal("DETECTION_FAILED", format!("Detection failed: {}", e)))?;

    if !detection_result.detected {
        return Err(not_found(
            "NOT_DETECTED",
            format!("Offering '{}' not detected on this system", offering_type),
        ));
    }

    // Extract location from offering or detection result
    let offering_protocol = connection::infer_protocol_from_manifest_metadata(
        &offering_type,
        &offering_def.category,
        offering_def.connection.as_ref(),
    );
    let location = OfferingLocation {
        host: req
            .location
            .clone()
            .unwrap_or_else(|| "localhost".to_string()),
        port: req.port.unwrap_or_else(|| offering_def.default_host_port()),
        protocol: offering_protocol,
        agnostic_port: None,
        port_map: std::collections::HashMap::new(),
    };

    let detector = std::sync::Arc::new(
        crate::infra::detection::ContainerDetector::new(state.platform.docker.clone()),
    );
    let connectivity = ConnectivityOrchestrator::new(detector);
    let connectivity_outcome = connectivity
        .ensure_connectivity(offering_def, Some(&location), &state.current.stone.name)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                offering = %offering_type,
                error = %e,
                "Connectivity enforcement failed"
            );
            crate::domain::ConnectivityOutcome {
                status: ConnectivityStatus::Failed,
                details: format!("Connectivity enforcement error: {}", e),
            }
        });
    let health = if connectivity_outcome.is_ok() {
        ServiceHealthStatus::Healthy
    } else {
        ServiceHealthStatus::Degraded
    };

    let control_level = req
        .control_level
        .as_ref()
        .and_then(|s| match s.as_str() {
            "full" => Some(AdoptedControlLevel::Full),
            "monitor" => Some(AdoptedControlLevel::Monitor),
            "announce" => Some(AdoptedControlLevel::Announce),
            _ => None,
        })
        .unwrap_or_default();

    let guidance = crate::tasks::build_adopted_guidance(
        &state,
        &adopted_fqn.to_string(),
        &offering_type,
        location.port,
        None,
    );

    // Get control config from adopted mode
    let control = offering_def.get_control_config();

    let unified = Offering {
        offering_id: generate_guidv7(),
        name: adopted_fqn,
        offering: offering_type.clone(),
        category: offering_def.category.clone(),
        version: detection_result
            .version
            .unwrap_or_else(|| "unknown".to_string()),
        status: OfferingStatus::Running,
        health,
        sub_capabilities: Vec::new(), // Populated by capabilities discovery task
        location,
        mode_data: OfferingModeData::Adopted(AdoptedData {
            control_level,
            start_command: control.as_ref().and_then(|c| c.start_command.clone()),
            stop_command: control.as_ref().and_then(|c| c.stop_command.clone()),
            restart_command: control.as_ref().and_then(|c| c.restart_command.clone()),
            health_check_url: control.as_ref().and_then(|c| c.health_check_url.clone()),
            guidance,
            container_name: None,
            detected_at: chrono::Utc::now(),
        }),
        registered_at: chrono::Utc::now(),
        updated_at: None,
        orchestration: None,
    };

    // Add to registry (gateway auto-persists)
    state.upsert_offering(unified.clone(), true).await;

    let ctx = Suggestion::from_headers(&headers, "adopt_offering");
    let suggestions = generate_suggestions(&ctx);

    crate::api::ok_maybe(unified, suggestions)
}

/// GET /api/v1/offerings/adopted - List adopted offerings
pub async fn list_adopted_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> crate::api::ApiResult<Vec<Offering>> {
    let offerings = state.get_adopted_offerings().await;

    let ctx = Suggestion::from_headers(&headers, "list_adopted");
    let suggestions = generate_suggestions(&ctx);

    crate::api::ok_maybe(offerings, suggestions)
}

/// GET /api/v1/offerings/borrowed - List borrowed offerings
pub async fn list_borrowed_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> crate::api::ApiResult<Vec<Offering>> {
    let offerings = state.get_borrowed_offerings().await;

    let ctx = Suggestion::from_headers(&headers, "list_borrowed");
    let suggestions = generate_suggestions(&ctx);

    crate::api::ok_maybe(offerings, suggestions)
}

/// DELETE /api/v1/offerings/:offering/adopt - Remove adopted offering
///
/// Removes an adopted offering from management (doesn't stop/delete the service).
pub async fn unadopt_offering_v1(
    State(state): State<AppState>,
    Path(offering): Path<String>,
    headers: HeaderMap,
) -> crate::api::ApiResult<String> {
    let offering_fqn = OfferingFqn::parse(&offering).map_err(|e| {
        bad_request(
            "INVALID_OFFERING_NAME",
            format!("Invalid offering name '{}': {}", offering, e),
        )
    })?;
    let offering_name = offering_fqn.fqn();

    // Find the offering to remove
    let found = {
        let offerings = state.offerings.read().await;
        offerings
            .iter()
            .find(|o| o.name == offering_fqn && o.is_adopted())
            .cloned()
    };

    match found {
        Some(to_remove) => {
            state.remove_offering(&to_remove.offering_id, true).await;
        }
        None => {
            return Err(not_found(
                "NOT_ADOPTED",
                format!("Offering '{}' is not currently adopted", offering_name),
            ));
        }
    }

    let ctx = Suggestion::from_headers(&headers, "unadopt_offering");
    let suggestions = generate_suggestions(&ctx);

    crate::api::ok_maybe(
        format!("Offering '{}' unadopted successfully", offering_name),
        suggestions,
    )
}

/// Adoptable offering information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdoptableOffering {
    pub name: String,
    pub category: String,
    pub description: String,
    pub version: Option<String>,
    pub detection_method: String,
}

/// Adopt offering request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdoptOfferingRequest {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub control_level: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub location: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub port: Option<u16>,
}

/// Borrow offering request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BorrowOfferingRequest {
    /// Name for this borrowed service
    pub name: String,

    /// URL/connection string for the external service
    pub url: String,

    /// Optional category (e.g., "Database", "Cache")
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub category: Option<String>,

    /// Optional credentials (password, API key, token) — stored encrypted in vault
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub credentials: Option<String>,

    /// Optional description
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
}

/// POST /api/v1/adoption/borrow - Register an external (borrowed) service
///
/// Borrowed services are external network services not managed by this stone,
/// but registered for reference and service discovery.
pub async fn borrow_service_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<BorrowOfferingRequest>,
) -> crate::api::ApiResult<Offering> {
    let borrow_fqn = OfferingFqn::new(&req.name).map_err(|e| {
        bad_request(
            "INVALID_NAME",
            format!("Invalid offering name '{}': {}", req.name, e),
        )
    })?;

    // Check if already borrowed with this name
    {
        let offerings = state.offerings.read().await;
        if offerings
            .iter()
            .any(|o| o.name == borrow_fqn && o.is_borrowed())
        {
            return Err(conflict(
                "ALREADY_BORROWED",
                format!("Service '{}' is already registered as borrowed", req.name),
            ));
        }
    }

    // Parse URL to extract host/port/protocol
    let url_parsed = url::Url::parse(&req.url)
        .map_err(|e| bad_request("INVALID_URL", format!("Invalid URL: {}", e)))?;

    let host = url_parsed.host_str().unwrap_or("localhost").to_string();
    let port = url_parsed.port().unwrap_or(0);
    let protocol = url_parsed.scheme().to_string();

    let location = OfferingLocation {
        host,
        port,
        protocol,
        agnostic_port: None,
        port_map: std::collections::HashMap::new(),
    };

    let unified = Offering {
        offering_id: generate_guidv7(),
        name: borrow_fqn,
        offering: req.name.clone(), // For borrowed, name and offering are the same
        category: req.category.clone().unwrap_or_default(),
        version: "unknown".to_string(),
        status: OfferingStatus::Running,
        health: ServiceHealthStatus::Offline, // Unknown until health check runs
        sub_capabilities: Vec::new(),
        location,
        mode_data: OfferingModeData::Borrowed(BorrowedData {
            health_method: None,
            credentials_key: {
                // Store credentials in Koi vault if provided
                if let Some(ref creds) = req.credentials {
                    let vault_key = format!("borrowed:{}:credentials", req.name);
                    match state.discovery.koi.vault() {
                        Ok(vault) => {
                            if let Err(e) = vault.store(&vault_key, creds) {
                                tracing::warn!(error = %e, "Failed to store borrowed credentials in vault");
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "Vault unavailable, credentials not stored");
                        }
                    }
                    Some(vault_key)
                } else {
                    None
                }
            },
            connection_template: Some(req.url.clone()),
            announced_at: chrono::Utc::now(),
        }),
        registered_at: chrono::Utc::now(),
        updated_at: None,
        orchestration: None,
    };

    // Add to registry (gateway auto-persists)
    state.upsert_offering(unified.clone(), true).await;

    let ctx = Suggestion::from_headers(&headers, "borrow_service");
    let suggestions = generate_suggestions(&ctx);

    crate::api::ok_maybe(unified, suggestions)
}

/// DELETE /api/v1/adoption/borrow/:name - Unregister a borrowed service
///
/// Removes a borrowed service registration (doesn't affect the external service).
pub async fn unborrow_service_v1(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> crate::api::ApiResult<String> {
    let unborrow_fqn = OfferingFqn::parse(&name).map_err(|e| {
        bad_request(
            "INVALID_NAME",
            format!("Invalid offering name '{}': {}", name, e),
        )
    })?;

    // Find the offering to remove
    let found = {
        let offerings = state.offerings.read().await;
        offerings
            .iter()
            .find(|o| o.name == unborrow_fqn && o.is_borrowed())
            .cloned()
    };

    match found {
        Some(to_remove) => {
            state.remove_offering(&to_remove.offering_id, true).await;
        }
        None => {
            return Err(not_found(
                "NOT_BORROWED",
                format!("Service '{}' is not currently registered as borrowed", name),
            ));
        }
    }

    let ctx = Suggestion::from_headers(&headers, "unborrow_service");
    let suggestions = generate_suggestions(&ctx);

    crate::api::ok_maybe(
        format!("Borrowed service '{}' unregistered successfully", name),
        suggestions,
    )
}
