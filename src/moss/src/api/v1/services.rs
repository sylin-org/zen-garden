use crate::api::responses::{ApiResponse, CreateServiceRequest, ServiceActionResponse};
use crate::api::suggestions::{Suggestion, generate_suggestions};
use crate::domain::events::OfferingEvent;
use crate::domain::service_lifecycle;
use crate::{AppState, bad_request, conflict, internal, not_found};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use garden_common::{
    Offering, OfferingStatus, Ports, ServiceInfo, ServiceStatus,
    api_utils::{
        ApiErrorResponse, is_suspicious, sanitize_fqn_input, sanitize_query, sanitize_tag,
    },
    offerings::OfferingFqn,
};
use std::sync::Arc;

/// Convert Offering to ServiceInfo for API responses
fn offering_to_service_info(o: &Offering) -> ServiceInfo {
    ServiceInfo {
        offering_id: o.offering_id.clone(),
        name: o.name.to_string(),
        offering: o.offering.clone(),
        version: o.version.clone(),
        status: match o.status {
            OfferingStatus::Running => ServiceStatus::Running,
            OfferingStatus::Stopped => ServiceStatus::Stopped,
            OfferingStatus::Installing => ServiceStatus::Installing,
            OfferingStatus::Degraded => ServiceStatus::Degraded,
            OfferingStatus::Maintenance => ServiceStatus::Maintenance,
            OfferingStatus::Unknown => ServiceStatus::Unknown,
            OfferingStatus::Cordoned => ServiceStatus::Cordoned,
        },
        health: o.health.clone(),
        ports: Ports {
            native: o.location.port,
            agnostic: o.location.agnostic_port,
        },
        resources: o.managed_data().and_then(|m| m.resources.clone()),
        job_id: o.managed_data().and_then(|m| m.job_id.clone()),
        sub_capabilities: o.sub_capabilities.clone(),
        guidance: o
            .managed_data()
            .and_then(|m| m.guidance.clone())
            .or_else(|| o.adopted_data().and_then(|a| a.guidance.clone())),
        customized_by: o
            .managed_data()
            .map(|m| crate::domain::config_compose::patch_owners(&m.config_patches))
            .unwrap_or_default(),
    }
}

/// Query parameters for GET /api/v1/services
///
/// Unified endpoint behavior:
/// - No params: lists all local services (fast, local-only)
/// - With params: searches/filters across garden via tool registry
///
/// Query parameters:
/// - `q`: Search query (supports prefixes: c:, cat:, category:, t:, tag:, tags:)
/// - `name`: Search by exact service name
/// - `category`: Search by category
/// - `tag`: Search by tag
#[derive(Debug, serde::Deserialize)]
pub struct ServicesQuery {
    /// Search query with optional prefix
    #[serde(default)]
    pub q: Option<String>,

    /// Filter by name
    #[serde(default)]
    pub name: Option<String>,

    /// Filter by category
    #[serde(default)]
    pub category: Option<String>,

    /// Filter by tag
    #[serde(default)]
    pub tag: Option<String>,
}

impl ServicesQuery {
    /// Check if any search/filter params are provided
    fn has_search_params(&self) -> bool {
        self.q.is_some() || self.name.is_some() || self.category.is_some() || self.tag.is_some()
    }
}

/// GET /api/v1/stone/services - List services running on THIS stone
///
/// Returns all local services (containers) running on this stone.
/// This is a local-only operation, no remote queries.
///
/// Response: ServiceDiscoveryResponse with local services
pub async fn list_services_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<
    Json<ApiResponse<crate::domain::ServiceDiscoveryResponse>>,
    (StatusCode, Json<ApiErrorResponse>),
> {
    use crate::domain::list_all_local_services;

    tracing::debug!("list_services_v1: listing local services only");

    let response = list_all_local_services(&state).await;

    let ctx = Suggestion::from_headers(&headers, "list_services");
    let suggestions = generate_suggestions(&ctx);

    Ok(Json(ApiResponse {
        data: response,
        suggestions,
    }))
}

/// GET /api/v1/garden/services - Find services across the garden
///
/// Searches for services matching criteria across ALL stones in the garden.
/// Queries the unified tool registry which holds Local, Gateway, and Announced entries.
///
/// Query parameters:
/// - `q`: Search query (supports prefixes: c:, cat:, category:, t:, tag:, tags:)
/// - `name`: Search by exact service name
/// - `category`: Search by category
/// - `tag`: Search by tag
///
/// Response: ServiceDiscoveryResponse with found services
pub async fn find_services_v1(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ServicesQuery>,
    headers: HeaderMap,
) -> Result<
    Json<ApiResponse<crate::domain::ServiceDiscoveryResponse>>,
    (StatusCode, Json<ApiErrorResponse>),
> {
    use crate::domain::{ServiceSearchCriteria, find_services, list_all_local_services};

    tracing::debug!(
        q = ?query.q,
        name = ?query.name,
        category = ?query.category,
        tag = ?query.tag,
        has_params = query.has_search_params(),
        "find_services_v1: garden-wide search"
    );

    // Sanitize and validate inputs - reject suspicious patterns
    if let Some(ref q) = query.q
        && is_suspicious(q)
    {
        tracing::warn!(query = %q, "Suspicious query pattern detected");
        return Err(bad_request(
            "INVALID_QUERY",
            "Query contains invalid patterns".to_string(),
        ));
    }

    let response = if query.has_search_params() {
        // Search mode: filter/search across garden
        let criteria = if let Some(ref q) = query.q {
            let sanitized = sanitize_query(q).into_value();
            ServiceSearchCriteria::parse(&sanitized)
        } else if let Some(ref name) = query.name {
            let sanitized = sanitize_fqn_input(name).into_value();
            ServiceSearchCriteria::by_name(&sanitized)
        } else if let Some(ref category) = query.category {
            let sanitized = sanitize_tag(category).into_value();
            ServiceSearchCriteria::by_category(&sanitized)
        } else if let Some(ref tag) = query.tag {
            let sanitized = sanitize_tag(tag).into_value();
            ServiceSearchCriteria::by_tag(&sanitized)
        } else {
            unreachable!("has_search_params() returned true but no params found")
        };

        find_services(&criteria, &state).await
    } else {
        // No params: return all local services (fallback for convenience)
        list_all_local_services(&state).await
    };

    let ctx = Suggestion::from_headers(&headers, "find_services");
    let suggestions = generate_suggestions(&ctx);

    Ok(Json(ApiResponse {
        data: response,
        suggestions,
    }))
}

/// GET /api/v1/services/:service - Get specific service
pub async fn get_service_v1(
    State(state): State<AppState>,
    Path(service): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<ServiceInfo>>, (StatusCode, Json<ApiErrorResponse>)> {
    let service_name = normalize_service_name(&service)?;
    tracing::debug!(
        service = %service_name,
        "get_service_v1: handler invoked for /api/v1/services/:service"
    );

    let service_info = {
        let offerings = state.offerings.read().await;
        offerings
            .iter()
            .find(|o| o.name.to_string() == service_name && o.is_managed())
            .map(offering_to_service_info)
            .ok_or_else(|| {
                tracing::warn!(
                    service = %service_name,
                    "get_service_v1: service not found in registry"
                );
                not_found(
                    "SERVICE_NOT_FOUND",
                    format!("Service '{}' not found", service_name),
                )
            })?
    };

    let ctx = Suggestion::from_headers(&headers, "get_service");
    let suggestions = generate_suggestions(&ctx);

    Ok(Json(ApiResponse {
        data: service_info,
        suggestions,
    }))
}

/// POST /api/v1/services - Create service (zen: offer)
pub async fn create_service_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateServiceRequest>,
) -> Result<Json<ApiResponse<ServiceActionResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    use crate::domain::service_lifecycle::{self, InstallOutcome};

    let mut offering_fqn = OfferingFqn::parse(&payload.offering).map_err(|e| {
        bad_request(
            "INVALID_OFFERING_NAME",
            format!("Invalid offering name '{}': {}", payload.offering, e),
        )
    })?;

    let outcome = service_lifecycle::install(&state, &mut offering_fqn)
        .await
        .map_err(|e| lifecycle_error("INSTALL_FAILED", &e))?;

    let ctx = Suggestion::from_headers(&headers, "create_service");
    let suggestions = generate_suggestions(&ctx);

    let response = match outcome {
        InstallOutcome::ImageDirectStarted {
            service_name,
            job_id,
        } => ServiceActionResponse {
            service: service_name,
            action: "create".to_string(),
            status: "accepted".to_string(),
            message: format!(
                "Image-direct installation started, check /api/jobs/{} for status",
                job_id
            ),
        },
        InstallOutcome::Adopted { service_name } => ServiceActionResponse {
            service: service_name,
            action: "create".to_string(),
            status: "adopted".to_string(),
            message: "Existing container adopted into registry".to_string(),
        },
        InstallOutcome::InstallStarted {
            service_name,
            job_id,
        } => ServiceActionResponse {
            service: service_name,
            action: "create".to_string(),
            status: "accepted".to_string(),
            message: format!(
                "Installation started, check /api/jobs/{} for status",
                job_id
            ),
        },
        InstallOutcome::Maintenance { service_name } => ServiceActionResponse {
            service: service_name,
            action: "create".to_string(),
            status: "maintenance".to_string(),
            message: "Service under maintenance, retry later".to_string(),
        },
    };

    Ok(Json(ApiResponse {
        data: response,
        suggestions,
    }))
}

/// POST /api/v1/services/:service/rest - Rest (stop) service
pub async fn rest_service_v1(
    State(state): State<AppState>,
    Path(service): Path<String>,
    headers: HeaderMap,
) -> crate::api::ApiResult<ServiceActionResponse> {
    let service_name = normalize_service_name(&service)?;

    service_lifecycle::stop(&state, &service_name)
        .await
        .map_err(|e| lifecycle_error("STOP_FAILED", &e))?;

    let ctx = Suggestion::from_headers(&headers, "rest_service");
    let suggestions = generate_suggestions(&ctx);
    crate::api::ok_maybe(
        ServiceActionResponse {
            service: service_name,
            action: "rest".to_string(),
            status: "stopped".to_string(),
            message: "Service stopped successfully".to_string(),
        },
        suggestions,
    )
}

/// POST /api/v1/services/:service/wake - Wake (start) service
pub async fn wake_service_v1(
    State(state): State<AppState>,
    Path(service): Path<String>,
    headers: HeaderMap,
) -> crate::api::ApiResult<ServiceActionResponse> {
    let service_name = normalize_service_name(&service)?;

    service_lifecycle::start(&state, &service_name)
        .await
        .map_err(|e| lifecycle_error("START_FAILED", &e))?;

    let ctx = Suggestion::from_headers(&headers, "wake_service");
    let suggestions = generate_suggestions(&ctx);
    crate::api::ok_maybe(
        ServiceActionResponse {
            service: service_name,
            action: "wake".to_string(),
            status: "running".to_string(),
            message: "Service started successfully".to_string(),
        },
        suggestions,
    )
}

/// POST /api/v1/services/:service/nourish - Nourish (upgrade) service
pub async fn nourish_service_v1(
    State(state): State<AppState>,
    Path(service): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<ServiceActionResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    use crate::domain::service_lifecycle::{self, NourishOutcome};

    let service_name = normalize_service_name(&service)?;

    let outcome = service_lifecycle::nourish(&state, &service_name)
        .await
        .map_err(|e| lifecycle_error("NOURISH_FAILED", &e))?;

    let (status, message) = match outcome {
        NourishOutcome::Maintenance => ("maintenance", "Service under maintenance, retry later"),
        NourishOutcome::Upgraded => ("upgraded", "Service upgraded successfully"),
    };

    let ctx = Suggestion::from_headers(&headers, "nourish_service");
    let suggestions = generate_suggestions(&ctx);

    Ok(Json(ApiResponse {
        data: ServiceActionResponse {
            service: service_name,
            action: "nourish".to_string(),
            status: status.to_string(),
            message: message.to_string(),
        },
        suggestions,
    }))
}

/// DELETE /api/v1/services/:service - Remove service and stop container (preserves volumes)
/// Container is stopped and removed, but volumes are preserved for potential recovery.
/// Use POST /api/v1/services/:service/destroy for complete destruction (uproot).
pub async fn delete_service_v1(
    State(state): State<AppState>,
    Path(service): Path<String>,
    headers: HeaderMap,
) -> crate::api::ApiResult<ServiceActionResponse> {
    let service_name = normalize_service_name(&service)?;

    service_lifecycle::remove(&state, &service_name)
        .await
        .map_err(|e| lifecycle_error("REMOVE_FAILED", &e))?;

    let ctx = Suggestion::from_headers(&headers, "delete_service");
    let suggestions = generate_suggestions(&ctx);
    crate::api::ok_maybe(
        ServiceActionResponse {
            service: service_name,
            action: "delete".to_string(),
            status: "removed".to_string(),
            message: "Service removed (container stopped and removed, volumes preserved)"
                .to_string(),
        },
        suggestions,
    )
}

/// POST /api/v1/services/:service/destroy - Hard delete (uproot: remove from registry AND destroy container)
pub async fn destroy_service_v1(
    State(state): State<AppState>,
    Path(service): Path<String>,
    headers: HeaderMap,
) -> crate::api::ApiResult<ServiceActionResponse> {
    let service_name = normalize_service_name(&service)?;

    service_lifecycle::destroy(&state, &service_name)
        .await
        .map_err(|e| lifecycle_error("DESTROY_FAILED", &e))?;

    let ctx = Suggestion::from_headers(&headers, "destroy_service");
    let suggestions = generate_suggestions(&ctx);
    crate::api::ok_maybe(
        ServiceActionResponse {
            service: service_name,
            action: "destroy".to_string(),
            status: "uprooted".to_string(),
            message: "Service destroyed (container removed)".to_string(),
        },
        suggestions,
    )
}

// ============================================================================
// Services API - Technical Layer (New Endpoints)
// ============================================================================

/// GET /api/v1/services/manifests - List all service manifests
pub async fn list_manifests_v1(
    State(catalog): State<Arc<crate::domain::Catalog>>,
) -> Result<
    (
        StatusCode,
        Json<ApiResponse<Vec<crate::infra::manifests::TemplateInfo>>>,
    ),
    (StatusCode, Json<ApiErrorResponse>),
> {
    let manifests: Vec<_> = catalog
        .manifests()
        .sw
        .entries
        .values()
        .map(|e| e.to_template_info())
        .collect();

    Ok((
        StatusCode::OK,
        Json(ApiResponse {
            data: manifests,
            suggestions: None,
        }),
    ))
}

/// GET /api/v1/services/:name/manifest - Get specific manifest YAML
pub async fn get_manifest_v1(
    State(catalog): State<Arc<crate::domain::Catalog>>,
    Path(name): Path<String>,
) -> Result<(StatusCode, String), (StatusCode, Json<ApiErrorResponse>)> {
    let offering_fqn = OfferingFqn::parse(&name).map_err(|e| {
        bad_request(
            "INVALID_SERVICE_NAME",
            format!("Invalid service name '{}': {}", name, e),
        )
    })?;
    let offering_type = offering_fqn.offering.clone();

    let entry = catalog.get_manifest(&offering_type).ok_or_else(|| {
        not_found(
            "MANIFEST_NOT_FOUND",
            format!("Manifest for '{}' not found", offering_type),
        )
    })?;

    let yaml = entry
        .managed
        .as_ref()
        .map(|m| m.snippet_yaml.clone())
        .unwrap_or_default();
    Ok((StatusCode::OK, yaml))
}

/// GET /api/v1/services/:service/logs - Stream service logs (SSE)
pub async fn stream_service_logs_v1(
    Path(service): Path<String>,
    State(state): State<AppState>,
) -> Result<
    axum::response::sse::Sse<
        impl futures_util::stream::Stream<
            Item = Result<axum::response::sse::Event, std::convert::Infallible>,
        >,
    >,
    (StatusCode, Json<ApiErrorResponse>),
> {
    let service_name = normalize_service_name(&service)?;

    // Verify the service exists before starting the stream
    let exists = {
        let offerings = state.offerings.read().await;
        offerings.iter().any(|o| o.name.to_string() == service_name)
    };
    if !exists {
        return Err(not_found(
            "SERVICE_NOT_FOUND",
            format!("Service '{}' not found", service_name),
        ));
    }

    use axum::response::sse::{Event, KeepAlive, Sse};
    use tokio_stream::StreamExt;

    let token = state.shutdown_token.child_token();
    let mut log_source = state.platform.container.get_logs_stream(&service_name, true);

    let stream = async_stream::stream! {
        loop {
            tokio::select! {
                item = log_source.next() => {
                    match item {
                        Some(Ok(line)) => {
                            if let Ok(event) = Event::default().json_data(&line) {
                                yield Ok::<Event, std::convert::Infallible>(event);
                            }
                        }
                        Some(Err(e)) => {
                            tracing::warn!(error = %e, "Docker log stream error");
                            break;
                        }
                        None => break,
                    }
                }
                _ = token.cancelled() => break,
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// GET /api/v1/services/:service/env - Get service environment variables
///
/// Returns environment variables for a running Docker-managed service.
/// Adopted services return an empty map (env is managed by the host OS).
pub async fn get_service_env_v1(
    State(state): State<AppState>,
    Path(service): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiErrorResponse>)> {
    let service_name = normalize_service_name(&service)?;

    // Check if service exists in registry
    let offering = {
        let offerings = state.offerings.read().await;
        offerings
            .iter()
            .find(|o| o.name.to_string() == service_name)
            .cloned()
    };

    let offering = offering.ok_or_else(|| {
        not_found(
            "SERVICE_NOT_FOUND",
            format!("Service '{}' not found", service_name),
        )
    })?;

    // Look up manageable_env from the manifest
    let manifest_offering = state.catalog.get_manifest(&service_name);
    let manageable = manifest_offering
        .as_ref()
        .and_then(|o| o.manageable_env.as_ref());
    let manageable_vars: Vec<String> = manageable.map(|m| m.vars.clone()).unwrap_or_default();

    // For Docker-managed containers, inspect to get env vars
    if offering.is_managed() {
        match state
            .platform
            .container
            .inspect_container_spec(&service_name)
            .await
        {
            Ok(spec) => {
                let env_map: std::collections::HashMap<String, String> = spec
                    .environment
                    .iter()
                    .filter_map(|e| {
                        let (k, v) = e.split_once('=')?;
                        Some((k.to_string(), v.to_string()))
                    })
                    .collect();
                Ok(Json(serde_json::json!({
                    "data": env_map,
                    "manageable": manageable_vars,
                })))
            }
            Err(e) => Err(internal(
                "ENV_FETCH_FAILED",
                format!("Failed to read env for '{}': {}", service_name, e),
            )),
        }
    } else if !manageable_vars.is_empty() {
        // Adopted with manageable env: read via platform mechanism
        let svc_name = manageable
            .and_then(|m| m.service_name.clone())
            .unwrap_or_else(|| service_name.clone());
        let env_map =
            crate::infra::platform::service_env::read_env(&svc_name, &manageable_vars).await;
        Ok(Json(serde_json::json!({
            "data": env_map,
            "manageable": manageable_vars,
        })))
    } else {
        // No manageable env declared
        Ok(Json(serde_json::json!({ "data": {}, "manageable": [] })))
    }
}

/// PATCH /api/v1/services/:service/env - Update manageable environment variables
pub async fn patch_service_env_v1(
    State(state): State<AppState>,
    Path(service): Path<String>,
    Json(body): Json<std::collections::HashMap<String, Option<String>>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiErrorResponse>)> {
    let service_name = normalize_service_name(&service)?;

    // Check if service exists
    {
        let offerings = state.offerings.read().await;
        if !offerings.iter().any(|o| o.name.to_string() == service_name) {
            return Err(not_found(
                "SERVICE_NOT_FOUND",
                format!("Service '{}' not found", service_name),
            ));
        }
    }

    // Look up manageable_env from the manifest
    let manifest_offering = state.catalog.get_manifest(&service_name);
    let manageable_ref = manifest_offering
        .as_ref()
        .and_then(|o| o.manageable_env.as_ref());

    let manageable = manageable_ref.ok_or_else(|| {
        bad_request(
            "NO_MANAGEABLE_ENV",
            format!(
                "Service '{}' has no manageable environment variables declared",
                service_name
            ),
        )
    })?;

    // Validate all keys against the allowlist
    let allowed: std::collections::HashSet<&str> =
        manageable.vars.iter().map(|s| s.as_str()).collect();
    let rejected: Vec<&str> = body
        .keys()
        .filter(|k| !allowed.contains(k.as_str()))
        .map(|k| k.as_str())
        .collect();

    if !rejected.is_empty() {
        return Err(bad_request(
            "VARS_NOT_ALLOWED",
            format!(
                "Variables not in manageable allowlist: {}. Allowed: {}",
                rejected.join(", "),
                manageable.vars.join(", ")
            ),
        ));
    }

    // Apply via platform mechanism
    let svc_name = manageable
        .service_name
        .clone()
        .unwrap_or_else(|| service_name.clone());

    if let Err(e) = crate::infra::platform::service_env::write_env(&svc_name, &body).await {
        return Err(internal(
            "ENV_WRITE_FAILED",
            format!("Failed to write env for '{}': {}", service_name, e),
        ));
    }

    // Build the applied map (only the non-null values)
    let applied: std::collections::HashMap<&str, &str> = body
        .iter()
        .filter_map(|(k, v)| v.as_ref().map(|val| (k.as_str(), val.as_str())))
        .collect();

    Ok(Json(serde_json::json!({
        "applied": applied,
        "restart_required": manageable.restart_required,
    })))
}

/// POST /api/v1/services/:service:restart - Restart service
pub async fn restart_service_v1(
    State(state): State<AppState>,
    Path(service): Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<ApiErrorResponse>)> {
    let service_name = normalize_service_name(&service)?;

    service_lifecycle::restart(&state, &service_name)
        .await
        .map_err(|e| lifecycle_error("RESTART_FAILED", &e))?;

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "service": service_name,
            "action": "restart",
            "status": "restarted",
            "message": "Service restarted successfully"
        })),
    ))
}

/// POST /api/v1/services/:service/cordon - Mark service non-schedulable
pub async fn cordon_service_v1(
    State(state): State<AppState>,
    Path(service): Path<String>,
    headers: HeaderMap,
) -> crate::api::ApiResult<ServiceActionResponse> {
    let service_name = normalize_service_name(&service)?;

    service_lifecycle::cordon(&state, &service_name)
        .await
        .map_err(|e| lifecycle_error("CORDON_FAILED", &e))?;

    let ctx = Suggestion::from_headers(&headers, "cordon_service");
    let suggestions = generate_suggestions(&ctx);
    crate::api::ok_maybe(
        ServiceActionResponse {
            service: service_name,
            action: "cordon".to_string(),
            status: "cordoned".to_string(),
            message: "Service cordoned (non-schedulable)".to_string(),
        },
        suggestions,
    )
}

/// POST /api/v1/services:reconcile - Reconcile container inventory
#[derive(Debug, serde::Deserialize)]
pub struct ReconcileRequest {
    #[serde(default)]
    pub drop_invalid: bool,
}

pub async fn reconcile_inventory_v1(
    State(state): State<AppState>,
    Json(payload): Json<ReconcileRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<ApiErrorResponse>)> {
    use crate::domain::reconcile_services;

    let result = reconcile_services(&state, payload.drop_invalid).await;

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "ok",
            "adopted": result.adopted,
            "dropped_invalid": result.dropped_invalid,
            "skipped_existing": result.skipped_existing,
            "left_unregistered": result.left_unregistered,
            "error": result.error,
        })),
    ))
}

/// POST /api/v1/services:refresh - Refresh manifests catalog
pub async fn refresh_manifests_v1(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<ApiErrorResponse>)> {
    // Rebuild offerings catalog (which includes manifest validation)
    state.catalog.rebuild().await.map_err(|e| {
        internal(
            "REFRESH_FAILED",
            format!("Failed to refresh manifests: {}", e),
        )
    })?;

    let stats = state.catalog.stats().await;
    let fingerprint = stats.fingerprint.ok_or_else(|| {
        internal(
            "INDEX_UNAVAILABLE",
            "Manifests index unavailable after refresh".to_string(),
        )
    })?;

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "refreshed",
            "count": stats.compiled_count,
            "fingerprint": fingerprint,
            "generated_at": chrono::Utc::now().to_rfc3339()
        })),
    ))
}

// ============================================================================
// Sub-Capability Discovery
// ============================================================================

/// Discover sub-capabilities for a specific service
///
/// GET /api/v1/stone/services/:service/capabilities
pub async fn discover_service_capabilities_v1(
    State(state): State<AppState>,
    Path(service_name): Path<String>,
) -> Result<
    Json<ApiResponse<Vec<garden_common::SubCapability>>>,
    (StatusCode, Json<ApiErrorResponse>),
> {
    let service_name = normalize_service_name(&service_name)?;
    // Find the service and convert to ServiceInfo for discovery
    let service = {
        let offerings = state.offerings.read().await;
        offerings
            .iter()
            .find(|o| o.name.to_string() == service_name && o.is_managed())
            .map(offering_to_service_info)
            .ok_or_else(|| {
                not_found(
                    "SERVICE_NOT_FOUND",
                    format!("Service '{}' not found", service_name),
                )
            })?
    };

    // Get capability manifest for this offering
    let cap_manifest = crate::infra::manifests::get_capability_manifest(&service.offering)
        .ok_or_else(|| {
            not_found(
                "NO_CAPABILITY_MANIFEST",
                format!("No capability manifest found for '{}'", service.offering),
            )
        })?;

    // Determine offering mode
    let mode = {
        let offerings = state.offerings.read().await;
        offerings
            .iter()
            .find(|o| o.name.to_string() == service_name)
            .map(|o| o.mode_data.mode())
            .unwrap_or(garden_common::OfferingMode::Managed)
    };

    // Discover capabilities using manifest-based executor
    let executor = crate::domain::CapabilityExecutor::new();
    let collections = executor
        .list_capabilities(&service, cap_manifest, mode)
        .await
        .map_err(|e| {
            internal(
                "DISCOVERY_FAILED",
                format!("Failed to discover capabilities: {}", e),
            )
        })?;

    // Convert to SubCapability format
    let capabilities: Vec<garden_common::SubCapability> =
        collections.iter().map(|c| c.to_sub_capability()).collect();

    // Update the offering in registry with discovered capabilities via gateway
    if !capabilities.is_empty() {
        let caps = capabilities.clone();
        state
            .offerings
            .update_by_name(&service_name, |o| {
                o.sub_capabilities = caps;
                false // sub_capabilities are detail-only, don't trigger chirp sync
            })
            .await;
    }

    Ok(Json(ApiResponse {
        data: capabilities,
        suggestions: None,
    }))
}

/// Refresh sub-capabilities for all running services
///
/// POST /api/v1/stone/services/refresh-capabilities
pub async fn refresh_all_capabilities_v1(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<ApiErrorResponse>)> {
    let executor = crate::domain::CapabilityExecutor::new();
    let mut updated = 0;

    // Get offerings snapshot
    let offerings_snapshot: Vec<(String, String, garden_common::OfferingMode, ServiceInfo)> = {
        let offerings = state.offerings.read().await;
        offerings
            .iter()
            .filter(|o| o.status == OfferingStatus::Running)
            .map(|o| {
                (
                    o.name.to_string(),
                    o.offering.clone(),
                    o.mode_data.mode(),
                    offering_to_service_info(o),
                )
            })
            .collect()
    };

    // Discover capabilities for each offering that has a manifest
    let mut updates: Vec<(String, Vec<garden_common::SubCapability>)> = Vec::new();

    for (name, offering, mode, service) in offerings_snapshot {
        // Check if there's a capability manifest for this offering
        let cap_manifest = match crate::infra::manifests::get_capability_manifest(&offering) {
            Some(m) => m,
            None => continue, // No capability manifest, skip
        };

        // Discover capabilities
        match executor
            .list_capabilities(&service, cap_manifest, mode)
            .await
        {
            Ok(collections) if !collections.is_empty() => {
                let sub_caps: Vec<garden_common::SubCapability> =
                    collections.iter().map(|c| c.to_sub_capability()).collect();
                tracing::debug!(
                    service = %name,
                    capabilities = ?sub_caps.iter().map(|c| format!("{}:{}", c.cap_type, c.items.len())).collect::<Vec<_>>(),
                    "Discovered capabilities"
                );
                updates.push((name, sub_caps));
                updated += 1;
            }
            Ok(_) => {} // No capabilities found
            Err(e) => {
                tracing::warn!(
                    service = %name,
                    error = ?e,
                    "Failed to discover capabilities"
                );
            }
        }
    }

    // Persist updated capabilities through the Offerings aggregate.
    if !updates.is_empty() {
        state
            .offerings
            .update_batch(|offerings| {
                let mut count = 0;
                for (name, sub_caps) in updates {
                    if let Some(o) = offerings.iter_mut().find(|o| o.name.to_string() == name) {
                        o.sub_capabilities = sub_caps;
                        count += 1;
                    }
                }
                count
            })
            .await;
    }

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "refreshed",
            "updated_services": updated,
        })),
    ))
}

fn normalize_service_name(service: &str) -> Result<String, (StatusCode, Json<ApiErrorResponse>)> {
    OfferingFqn::parse(service)
        .map(|fqn| fqn.fqn())
        .map_err(|e| {
            bad_request(
                "INVALID_SERVICE_NAME",
                format!("Invalid service name '{}': {}", service, e),
            )
        })
}

/// Map a domain lifecycle error to an API error tuple.
///
/// Uses `not_found` when the error message contains "not found",
/// otherwise `internal`.
fn lifecycle_error(code: &str, err: &anyhow::Error) -> (StatusCode, Json<ApiErrorResponse>) {
    let msg = format!("{}", err);
    if msg.contains("not found") {
        not_found("SERVICE_NOT_FOUND", msg)
    } else {
        internal(code, msg)
    }
}

/// POST /api/v1/stone/services/:service/reassign — Non-destructive FQN reassign.
///
/// Stops the container, renames it to match the new FQN, updates the offering
/// in the registry, persists, and starts the container back up.
/// Volumes are bound by container ID and survive the rename.
pub async fn reassign_service_v1(
    State(state): State<AppState>,
    Path(service): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiErrorResponse>)> {
    let old_name = normalize_service_name(&service)?;

    let new_fqn_str = body
        .get("new_fqn")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            bad_request(
                "MISSING_FIELD",
                "Missing required field 'new_fqn'".to_string(),
            )
        })?;

    let new_fqn = OfferingFqn::parse(new_fqn_str).map_err(|e| {
        bad_request(
            "INVALID_FQN",
            format!("Invalid FQN '{}': {}", new_fqn_str, e),
        )
    })?;
    let new_name = new_fqn.fqn();

    if old_name == new_name {
        return Err(bad_request(
            "SAME_FQN",
            "New FQN is the same as the current one".to_string(),
        ));
    }

    // Check no existing service has the new name
    {
        let offerings = state.offerings.read().await;
        if offerings.iter().any(|o| o.name.to_string() == new_name) {
            return Err(conflict(
                "FQN_EXISTS",
                format!("A service with FQN '{}' already exists", new_name),
            ));
        }
    }

    // Find the offering
    let offering_id = {
        let offerings = state.offerings.read().await;
        offerings
            .iter()
            .find(|o| o.name.to_string() == old_name && o.is_managed())
            .map(|o| o.offering_id.clone())
            .ok_or_else(|| {
                not_found(
                    "SERVICE_NOT_FOUND",
                    format!("Managed service '{}' not found", old_name),
                )
            })?
    };

    // Step 1: Stop the container
    if let Err(e) = state
        .platform
        .container
        .stop_service(&old_name, Some(&state.console))
        .await
    {
        // Container may already be stopped — log but continue
        tracing::warn!(error = ?e, service = %old_name, "Stop before rename failed (may already be stopped)");
    }

    // Step 2: Rename the Docker container
    if let Err(e) = state
        .platform
        .container
        .rename_service(&old_name, &new_name)
        .await
    {
        // Try to restart the old container on failure
        let _ = state
            .platform
            .container
            .start_service(&old_name, Some(&state.console))
            .await;
        return Err(internal(
            "RENAME_FAILED",
            format!("Failed to rename container: {}", e),
        ));
    }

    // Step 3: Update offering in registry
    state
        .offerings
        .update(&offering_id, |o| {
            o.name = new_fqn.clone();
            true
        })
        .await;

    // Step 4: Start the container with its new name
    if let Err(e) = state
        .platform
        .container
        .start_service(&new_name, Some(&state.console))
        .await
    {
        tracing::error!(error = ?e, service = %new_name, "Failed to start container after rename");
        state
            .offerings
            .update(&offering_id, |o| {
                o.status = OfferingStatus::Stopped;
                true
            })
            .await;
    }

    // Step 5: Emit renamed event (triggers chirp, timer rename, tools projection)
    state.event_bus.emit(OfferingEvent::renamed(
        &offering_id,
        &old_name,
        &new_name,
        state.stone_name(),
    ));

    tracing::info!(
        from = %old_name,
        to = %new_name,
        "Service reassigned to new FQN"
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "old_fqn": old_name,
        "new_fqn": new_name,
        "message": format!("Service reassigned from {} to {}", old_name, new_name),
    })))
}
