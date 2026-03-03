use crate::api::responses::{ApiResponse, CreateServiceRequest, ServiceActionResponse};
use crate::api::suggestions::{generate_suggestions, SuggestionContext};
use crate::domain::connection;
use crate::domain::events::OfferingEvent;
use crate::infra::network::{load_network_state, revert_to_dhcp};
use crate::infra::TaskStore;
use crate::{error_response, AppState};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use garden_common::{
    api_utils::{
        is_suspicious, sanitize_fqn_input, sanitize_query, sanitize_tag, ApiErrorResponse,
    },
    offerings::OfferingFqn,
    utils::ids::generate_guidv7,
    ManagedData, Offering, OfferingLocation, OfferingModeData, OfferingStatus, Ports,
    ServiceHealthStatus, ServiceInfo, ServiceStatus,
};

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
            .map(|m| {
                crate::domain::config_compose::patch_owners(&m.config_patches)
            })
            .unwrap_or_default(),
    }
}

/// Query parameters for GET /api/v1/services
///
/// Unified endpoint behavior:
/// - No params: lists all local services (fast, local-only)
/// - With params: searches/filters across garden (may query remote stones)
///
/// Query parameters:
/// - `q`: Search query (supports prefixes: c:, cat:, category:, t:, tag:, tags:)
/// - `name`: Search by exact service name
/// - `category`: Search by category
/// - `tag`: Search by tag
/// - `fresh`: Force fresh discovery (bypass cache)
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

    /// Force fresh discovery
    #[serde(default)]
    pub fresh: bool,
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

    let ctx = SuggestionContext::from_headers(&headers, "list_services");
    let suggestions = generate_suggestions(&ctx);

    Ok(Json(ApiResponse {
        data: response,
        suggestions,
    }))
}

/// GET /api/v1/garden/services - Find services across the garden
///
/// Searches for services matching criteria across ALL stones in the garden.
/// This is a garden-wide operation that queries remote stones.
///
/// Query parameters:
/// - `q`: Search query (supports prefixes: c:, cat:, category:, t:, tag:, tags:)
/// - `name`: Search by exact service name
/// - `category`: Search by category
/// - `tag`: Search by tag
/// - `fresh`: Force fresh discovery (bypass cache)
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
    use crate::domain::{find_services, list_all_local_services, ServiceSearchCriteria};

    tracing::debug!(
        q = ?query.q,
        name = ?query.name,
        category = ?query.category,
        tag = ?query.tag,
        fresh = query.fresh,
        has_params = query.has_search_params(),
        "find_services_v1: garden-wide search"
    );

    // Sanitize and validate inputs - reject suspicious patterns
    if let Some(ref q) = query.q {
        if is_suspicious(q) {
            tracing::warn!(query = %q, "Suspicious query pattern detected");
            return Err(error_response(
                StatusCode::BAD_REQUEST,
                "INVALID_QUERY",
                "Query contains invalid patterns".to_string(),
                None,
            ));
        }
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

        find_services(&criteria, &state, query.fresh).await
    } else {
        // No params: return all local services (fallback for convenience)
        list_all_local_services(&state).await
    };

    let ctx = SuggestionContext::from_headers(&headers, "find_services");
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

    let offerings = state.offerings.read().await;
    let service_info = offerings
        .iter()
        .find(|o| o.name.to_string() == service_name && o.is_managed())
        .map(offering_to_service_info)
        .ok_or_else(|| {
            tracing::warn!(
                service = %service_name,
                "get_service_v1: service not found in registry"
            );
            error_response(
                StatusCode::NOT_FOUND,
                "SERVICE_NOT_FOUND",
                format!("Service '{}' not found", service_name),
                None,
            )
        })?;
    drop(offerings);

    let ctx = SuggestionContext::from_headers(&headers, "get_service");
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
    let offering_fqn = OfferingFqn::parse(&payload.offering).map_err(|e| {
        error_response(
            StatusCode::BAD_REQUEST,
            "INVALID_OFFERING_NAME",
            format!("Invalid offering name '{}': {}", payload.offering, e),
            None,
        )
    })?;
    let service_name = offering_fqn.fqn();
    let offering_type = offering_fqn.offering.clone();

    // ── Image-direct deployment (OFFER-0006) ──────────────────────────────
    if offering_fqn.source == Some(garden_common::offerings::OfferingSource::Image) {
        let image_ref = offering_fqn
            .image_ref
            .clone()
            .unwrap_or_else(|| offering_fqn.offering.clone());

        // Check if already installed
        {
            let offerings = state.offerings.read().await;
            if let Some(existing) = offerings
                .iter()
                .find(|o| o.name.to_string() == service_name && o.is_managed())
            {
                if existing.status == OfferingStatus::Maintenance {
                    let ctx = SuggestionContext::from_headers(&headers, "create_service");
                    let suggestions = generate_suggestions(&ctx);
                    return Ok(Json(ApiResponse {
                        data: ServiceActionResponse {
                            service: service_name,
                            action: "create".to_string(),
                            status: "maintenance".to_string(),
                            message: "Service under maintenance, retry later".to_string(),
                        },
                        suggestions,
                    }));
                }
            }
        }

        // Create job
        let job_id = uuid::Uuid::now_v7().to_string();
        let job = crate::Job {
            id: job_id.clone(),
            offerings: vec![service_name.clone()],
            status: crate::JobStatus::Pending,
            completed: vec![],
            failed: std::collections::HashMap::new(),
            started_at: std::time::SystemTime::now(),
            completed_at: None,
        };
        state.jobs.write().await.insert(job_id.clone(), job);

        // Add minimal Installing entry to registry
        let installing_offering = Offering {
            offering_id: generate_guidv7(),
            name: offering_fqn.clone(),
            offering: offering_type.clone(),
            version: image_ref
                .rsplit_once(':')
                .map(|(_, tag)| tag)
                .unwrap_or("latest")
                .to_string(),
            status: OfferingStatus::Installing,
            health: ServiceHealthStatus::Offline,
            sub_capabilities: Vec::new(),
            location: OfferingLocation {
                host: "localhost".to_string(),
                port: 0, // assigned after inspection
                protocol: "http".to_string(),
                agnostic_port: None,
                port_map: std::collections::HashMap::new(),
            },
            mode_data: OfferingModeData::Managed(ManagedData {
                resources: None,
                job_id: Some(job_id.clone()),
                guidance: None,
                ..Default::default()
            }),
            registered_at: chrono::Utc::now(),
            updated_at: None,
            orchestration: None,
        };
        state.upsert_offering(installing_offering, true).await;
        let _ = state.persist_offerings().await;

        // Spawn async image-direct installation
        let state_clone = state.clone();
        let fqn_clone = offering_fqn.clone();
        let image_clone = image_ref.clone();
        let job_id_clone = job_id.clone();
        let svc_name_clone = service_name.clone();
        tokio::spawn(async move {
            crate::install_image_direct_task(
                &state_clone,
                &job_id_clone,
                &fqn_clone,
                &image_clone,
                &svc_name_clone,
            )
            .await;
        });

        let ctx = SuggestionContext::from_headers(&headers, "create_service");
        let suggestions = generate_suggestions(&ctx);
        return Ok(Json(ApiResponse {
            data: ServiceActionResponse {
                service: service_name,
                action: "create".to_string(),
                status: "accepted".to_string(),
                message: format!(
                    "Image-direct installation started, check /api/jobs/{} for status",
                    job_id
                ),
            },
            suggestions,
        }));
    }

    // Self-heal: if the container exists but registry forgot it (e.g. after restart), adopt it.
    if state
        .docker
        .zen_container_exists(&service_name)
        .await
        .unwrap_or(false)
    {
        let in_registry = {
            let offerings = state.offerings.read().await;
            offerings.iter().any(|o| o.name.to_string() == service_name)
        };

        if !in_registry {
            let cached_caps = state.capabilities.read().await.clone();
            if let Ok(Some(adopted_offering)) = crate::adopt_offering_container(
                &state.docker,
                &state.manifest_registry,
                &service_name,
                &state.stone_name,
                cached_caps.as_ref(),
            )
            .await
            {
                state.upsert_offering(adopted_offering, true).await;
                let _ = state.persist_offerings().await;

                let ctx = SuggestionContext::from_headers(&headers, "create_service");
                let suggestions = generate_suggestions(&ctx);

                return Ok(Json(ApiResponse {
                    data: ServiceActionResponse {
                        service: service_name,
                        action: "create".to_string(),
                        status: "adopted".to_string(),
                        message: "Existing container adopted into registry".to_string(),
                    },
                    suggestions,
                }));
            }
        }
    }

    let compiled = match crate::get_compiled_offering(&state, &offering_type).await {
        Ok(Some(o)) => o,
        Ok(None) => {
            return Err(error_response(
                StatusCode::NOT_FOUND,
                "TEMPLATE_NOT_FOUND",
                format!("Unknown offering: {}", offering_type),
                None,
            ));
        }
        Err(e) => {
            return Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                format!("Failed to read offerings index: {}", e),
                None,
            ));
        }
    };

    if compiled.compatibility.decision == garden_common::COMPAT_FAIL {
        let reason = compiled
            .compatibility
            .reason
            .unwrap_or_else(|| "Unknown reason".to_string());
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "COMPATIBILITY_FAILED",
            format!("Offering is incompatible with this stone: {}", reason),
            None,
        ));
    }

    // Check if already running/maintenance
    let offerings = state.offerings.read().await;
    if let Some(existing) = offerings
        .iter()
        .find(|o| o.name.to_string() == service_name && o.is_managed())
    {
        if existing.status == OfferingStatus::Maintenance {
            drop(offerings);
            let ctx = SuggestionContext::from_headers(&headers, "create_service");
            let suggestions = generate_suggestions(&ctx);
            return Ok(Json(ApiResponse {
                data: ServiceActionResponse {
                    service: service_name,
                    action: "create".to_string(),
                    status: "maintenance".to_string(),
                    message: "Service under maintenance, retry later".to_string(),
                },
                suggestions,
            }));
        }
    }
    drop(offerings);

    // Create job
    let job_id = uuid::Uuid::now_v7().to_string();
    let job = crate::Job {
        id: job_id.clone(),
        offerings: vec![service_name.clone()],
        status: crate::JobStatus::Pending,
        completed: vec![],
        failed: std::collections::HashMap::new(),
        started_at: std::time::SystemTime::now(),
        completed_at: None,
    };

    state.jobs.write().await.insert(job_id.clone(), job);

    // Add service to registry immediately with Installing status
    // This ensures `rake list` shows the service as planting
    {
        let native_port = compiled.default_host_port();
        let offering_protocol = connection::infer_protocol_from_manifest_metadata(
            &offering_type,
            &compiled.category,
            state
                .manifest_registry
                .get_offering(&offering_type)
                .and_then(|entry| entry.connection.as_ref()),
        );
        let installing_offering = Offering {
            offering_id: generate_guidv7(),
            name: offering_fqn.clone(),
            offering: offering_type.clone(),
            version: compiled
                .image
                .split(':')
                .next_back()
                .unwrap_or("latest")
                .into(),
            status: OfferingStatus::Installing,
            health: ServiceHealthStatus::Offline,
            sub_capabilities: Vec::new(),
            location: OfferingLocation {
                host: "localhost".to_string(),
                port: native_port,
                protocol: offering_protocol,
                agnostic_port: None,
                port_map: std::collections::HashMap::new(),
            },
            mode_data: OfferingModeData::Managed(ManagedData {
                resources: None,
                job_id: Some(job_id.clone()),
                guidance: None, // Guidance is added when installation completes
                ..Default::default()
            }),
            registered_at: chrono::Utc::now(),
            updated_at: None,
            orchestration: None,
        };

        state.upsert_offering(installing_offering, true).await;
    }
    let _ = state.persist_offerings().await;

    // Spawn async installation task
    let state_clone = state.clone();
    let offering_clone = offering_type.clone();
    let service_name_clone = service_name.clone();
    let job_id_clone = job_id.clone();
    tokio::spawn(async move {
        crate::install_service_task(
            &state_clone,
            &job_id_clone,
            &offering_clone,
            &service_name_clone,
        )
        .await;
    });

    let ctx = SuggestionContext::from_headers(&headers, "create_service");
    let suggestions = generate_suggestions(&ctx);

    Ok(Json(ApiResponse {
        data: ServiceActionResponse {
            service: service_name,
            action: "create".to_string(),
            status: "accepted".to_string(),
            message: format!(
                "Installation started, check /api/jobs/{} for status",
                job_id
            ),
        },
        suggestions,
    }))
}

/// POST /api/v1/services/:service/rest - Rest (stop) service
pub async fn rest_service_v1(
    State(state): State<AppState>,
    Path(service): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<ServiceActionResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    let service_name = normalize_service_name(&service)?;
    // Find the offering
    let offering_id = {
        let offerings = state.offerings.read().await;
        offerings
            .iter()
            .find(|o| o.name.to_string() == service_name && o.is_managed())
            .map(|o| o.offering_id.clone())
            .ok_or_else(|| {
                error_response(
                    StatusCode::NOT_FOUND,
                    "SERVICE_NOT_FOUND",
                    format!("Service '{}' not found", service_name),
                    None,
                )
            })?
    };

    // Stop the Docker container
    if let Err(e) = state
        .docker
        .stop_service(&service_name, Some(&state.console))
        .await
    {
        tracing::error!(error = ?e, service = %service_name, "Failed to stop container");
        return Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "STOP_FAILED",
            format!("Failed to stop service: {}", e),
            None,
        ));
    }

    // Update status via gateway (syncs self_entry for chirps)
    state.update_offering(&offering_id, true, |o| {
        o.status = OfferingStatus::Stopped;
        true
    }).await;

    if let Err(e) = state.persist_offerings().await {
        tracing::warn!(error = ?e, "Failed to persist offerings after rest");
    }

    // Emit offering lifecycle event
    state.event_bus.emit(OfferingEvent::stopped(
        &offering_id,
        &service_name,
        state.stone_name(),
    ));

    let ctx = SuggestionContext::from_headers(&headers, "rest_service");
    let suggestions = generate_suggestions(&ctx);

    Ok(Json(ApiResponse {
        data: ServiceActionResponse {
            service: service_name,
            action: "rest".to_string(),
            status: "stopped".to_string(),
            message: "Service stopped successfully".to_string(),
        },
        suggestions,
    }))
}

/// POST /api/v1/services/:service/wake - Wake (start) service
pub async fn wake_service_v1(
    State(state): State<AppState>,
    Path(service): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<ServiceActionResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    let service_name = normalize_service_name(&service)?;
    // Find the offering
    let offering_id = {
        let offerings = state.offerings.read().await;
        offerings
            .iter()
            .find(|o| o.name.to_string() == service_name && o.is_managed())
            .map(|o| o.offering_id.clone())
            .ok_or_else(|| {
                error_response(
                    StatusCode::NOT_FOUND,
                    "SERVICE_NOT_FOUND",
                    format!("Service '{}' not found", service_name),
                    None,
                )
            })?
    };

    // Check if the Docker container still exists. If the offering is registered
    // but the container was removed (pruned, failed upgrade, etc.), transparently
    // reinstall it from the manifest — preserving data in named volumes/mounts.
    let container_exists = state
        .docker
        .zen_container_exists(&service_name)
        .await
        .unwrap_or(false);

    if !container_exists {
        tracing::info!(
            service = %service_name,
            "Container missing for registered offering, reinstalling"
        );
        match rebuild_missing_container(&state, &service_name).await {
            Ok(_) => {
                tracing::info!(
                    service = %service_name,
                    "Container reinstalled successfully (data preserved)"
                );
            }
            Err(e) => {
                tracing::error!(error = ?e, service = %service_name, "Failed to reinstall missing container");
                return Err(error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "REINSTALL_FAILED",
                    format!(
                        "Container is missing and reinstall failed: {}. Try 'garden-rake offer {}' to reinstall manually.",
                        e, service_name
                    ),
                    None,
                ));
            }
        }
    } else {
        // Container exists — run compose-on-start if needed, then start normally.
        let needs_compose = {
            let offerings = state.offerings.read().await;
            offerings
                .iter()
                .find(|o| o.offering_id == offering_id)
                .and_then(|o| o.managed_data())
                .map(|d| !d.config_patches.is_empty())
                .unwrap_or(false)
        };

        if needs_compose {
            if let Err(e) = compose_on_start(&state, &service_name).await {
                tracing::warn!(
                    service = %service_name,
                    error = ?e,
                    "Compose-on-start failed, falling back to normal start"
                );
            }
        }

        // Start the Docker container
        if let Err(e) = state
            .docker
            .start_service(&service_name, Some(&state.console))
            .await
        {
            tracing::error!(error = ?e, service = %service_name, "Failed to start container");
            return Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "START_FAILED",
                format!("Failed to start service: {}", e),
                None,
            ));
        }
    }

    // Update status via gateway (syncs self_entry for chirps)
    state.update_offering(&offering_id, true, |o| {
        o.status = OfferingStatus::Running;
        true
    }).await;

    if let Err(e) = state.persist_offerings().await {
        tracing::warn!(error = ?e, "Failed to persist offerings after wake");
    }

    // Emit offering lifecycle event
    state.event_bus.emit(OfferingEvent::started(
        &offering_id,
        &service_name,
        state.stone_name(),
    ));

    let ctx = SuggestionContext::from_headers(&headers, "wake_service");
    let suggestions = generate_suggestions(&ctx);

    Ok(Json(ApiResponse {
        data: ServiceActionResponse {
            service: service_name,
            action: "wake".to_string(),
            status: "running".to_string(),
            message: "Service started successfully".to_string(),
        },
        suggestions,
    }))
}

/// POST /api/v1/services/:service/nourish - Nourish (upgrade) service
pub async fn nourish_service_v1(
    State(state): State<AppState>,
    Path(service): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<ServiceActionResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    let service_name = normalize_service_name(&service)?;

    // Find and validate the service
    let (offering_id, offering, old_version) = {
        let offerings = state.offerings.read().await;
        let o = offerings
            .iter()
            .find(|o| o.name.to_string() == service_name && o.is_managed())
            .ok_or_else(|| {
                error_response(
                    StatusCode::NOT_FOUND,
                    "SERVICE_NOT_FOUND",
                    format!("Service '{}' not found", service_name),
                    None,
                )
            })?;

        if o.status == OfferingStatus::Maintenance {
            let ctx = SuggestionContext::from_headers(&headers, "nourish_service");
            let suggestions = generate_suggestions(&ctx);
            return Ok(Json(ApiResponse {
                data: ServiceActionResponse {
                    service: service_name,
                    action: "nourish".to_string(),
                    status: "maintenance".to_string(),
                    message: "Service under maintenance, retry later".to_string(),
                },
                suggestions,
            }));
        }

        (o.offering_id.clone(), o.offering.clone(), o.version.clone())
    };

    // Mark as Maintenance via gateway (syncs self_entry)
    state.update_offering(&offering_id, true, |o| {
        o.status = OfferingStatus::Maintenance;
        true
    }).await;

    // Load template for upgrade
    let entry = state.manifest_registry.sw.get(&offering).ok_or_else(|| {
        error_response(
            StatusCode::NOT_FOUND,
            "TEMPLATE_NOT_FOUND",
            format!("Template for '{}' not found", offering),
            None,
        )
    })?;
    let template = entry.parse_template().map_err(|e| {
        // Restore status on error via gateway (syncs self_entry)
        let state_clone = state.clone();
        let id_clone = offering_id.clone();
        tokio::spawn(async move {
            state_clone.update_offering(&id_clone, true, |o| {
                o.status = OfferingStatus::Running;
                true
            }).await;
        });
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "TEMPLATE_LOAD_FAILED",
            format!("Failed to load template: {}", e),
            None,
        )
    })?;

    // Perform Docker upgrade
    let ports = template.ports_vec();
    let spec = crate::docker::ContainerSpec {
        image: template.image.clone(),
        command: template.command,
        ports,
        environment: template.environment,
        volumes: template.volumes,
        config_files: template.config_files,
    };
    if let Err(e) = state
        .docker
        .upgrade_service(&service_name, &spec, Some(&state.console))
        .await
    {
        tracing::error!(error = ?e, service = %service_name, "Docker upgrade failed");
        // Restore status via gateway (syncs self_entry)
        state.update_offering(&offering_id, true, |o| {
            o.status = OfferingStatus::Running;
            true
        }).await;
        return Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "UPGRADE_FAILED",
            format!("Failed to upgrade: {}", e),
            None,
        ));
    }

    let new_version = template
        .image
        .split(':')
        .next_back()
        .unwrap_or("latest")
        .to_string();
    let new_image = template.image.clone();

    // Update status and version via gateway (syncs self_entry + persists)
    let nv = new_version.clone();
    state.update_offering(&offering_id, true, |o| {
        o.status = OfferingStatus::Running;
        o.version = nv;
        true
    }).await;

    if let Err(e) = state.persist_offerings().await {
        tracing::warn!(error = ?e, "Failed to persist offerings after nourish");
    }

    // Emit offering lifecycle event (old_image reconstructed from old_version)
    let old_image = format!(
        "{}:{}",
        template.image.split(':').next().unwrap_or(&offering),
        old_version
    );
    state.event_bus.emit(OfferingEvent::updated(
        &offering_id,
        &service_name,
        state.stone_name(),
        &old_image,
        &new_image,
    ));

    let ctx = SuggestionContext::from_headers(&headers, "nourish_service");
    let suggestions = generate_suggestions(&ctx);

    Ok(Json(ApiResponse {
        data: ServiceActionResponse {
            service: service_name,
            action: "nourish".to_string(),
            status: "upgraded".to_string(),
            message: "Service upgraded successfully".to_string(),
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
) -> Result<Json<ApiResponse<ServiceActionResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    let service_name = normalize_service_name(&service)?;
    // Find and remove the service from registry
    let offering_id = {
        let offerings = state.offerings.read().await;
        let o = offerings
            .iter()
            .find(|o| o.name.to_string() == service_name && o.is_managed())
            .ok_or_else(|| {
                error_response(
                    StatusCode::NOT_FOUND,
                    "SERVICE_NOT_FOUND",
                    format!("Service '{}' not found", service_name),
                    None,
                )
            })?;
        o.offering_id.clone()
    };

    // Remove container first (preserves volumes by default)
    if let Err(e) = state
        .docker
        .remove_service(&service_name, Some(&state.console))
        .await
    {
        tracing::error!(error = ?e, service = %service_name, "Docker remove failed");
        // Don't fail completely - continue to remove from registry even if container removal fails
        tracing::warn!(service = %service_name, "Container removal failed, continuing with registry cleanup");
    }

    // Then remove from registry
    state.remove_offering(&offering_id, true).await;

    if let Err(e) = state.persist_offerings().await {
        tracing::warn!(error = ?e, "Failed to persist offerings after delete");
    }

    // Emit offering lifecycle event (removed = soft delete, volumes preserved)
    state.event_bus.emit(OfferingEvent::removed(
        &offering_id,
        &service_name,
        state.stone_name(),
    ));

    // Unregister scheduled tasks for this offering
    let task_store = TaskStore::new();
    if let Err(e) = task_store.unregister_tasks(&offering_id).await {
        tracing::warn!(
            offering_id = %offering_id,
            error = ?e,
            "Failed to unregister scheduled tasks (non-fatal)"
        );
    }

    // Release static IP if this offering was a requester
    // (will revert to DHCP if no other requesters remain)
    let mut network_state = load_network_state().await;
    if network_state.requested_by.contains(&service_name) {
        if let Err(e) = revert_to_dhcp(&service_name, &mut network_state).await {
            tracing::warn!(
                service = %service_name,
                error = ?e,
                "Failed to release static IP (non-fatal)"
            );
        } else {
            let remaining = network_state.requester_count();
            if remaining == 0 {
                tracing::info!(service = %service_name, "Released static IP, reverted to DHCP");
            } else {
                tracing::info!(
                    service = %service_name,
                    remaining_requesters = remaining,
                    "Released static IP requester, other services still using it"
                );
            }
        }
    }

    let ctx = SuggestionContext::from_headers(&headers, "delete_service");
    let suggestions = generate_suggestions(&ctx);

    Ok(Json(ApiResponse {
        data: ServiceActionResponse {
            service: service_name,
            action: "delete".to_string(),
            status: "removed".to_string(),
            message: "Service removed (container stopped and removed, volumes preserved)"
                .to_string(),
        },
        suggestions,
    }))
}

/// POST /api/v1/services/:service/destroy - Hard delete (uproot: remove from registry AND destroy container)
pub async fn destroy_service_v1(
    State(state): State<AppState>,
    Path(service): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<ServiceActionResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    let service_name = normalize_service_name(&service)?;
    // Find the service
    let offering_id = {
        let offerings = state.offerings.read().await;
        let o = offerings
            .iter()
            .find(|o| o.name.to_string() == service_name && o.is_managed())
            .ok_or_else(|| {
                error_response(
                    StatusCode::NOT_FOUND,
                    "SERVICE_NOT_FOUND",
                    format!("Service '{}' not found", service_name),
                    None,
                )
            })?;
        o.offering_id.clone()
    };

    // Hard delete: destroy Docker container first
    // If the container doesn't exist (already removed, failed install, etc.),
    // continue with registry cleanup — the goal is to fully remove the offering.
    if let Err(e) = state
        .docker
        .remove_service(&service_name, Some(&state.console))
        .await
    {
        tracing::warn!(
            service = %service_name,
            error = ?e,
            "Container removal failed, continuing with registry cleanup"
        );
    }

    // Then remove from registry
    state.remove_offering(&offering_id, true).await;

    if let Err(e) = state.persist_offerings().await {
        tracing::warn!(error = ?e, "Failed to persist offerings after destroy");
    }

    // Emit offering lifecycle event (destroyed = hard delete)
    state.event_bus.emit(OfferingEvent::destroyed(
        &offering_id,
        &service_name,
        state.stone_name(),
    ));

    // Unregister scheduled tasks for this offering
    let task_store = TaskStore::new();
    if let Err(e) = task_store.unregister_tasks(&offering_id).await {
        tracing::warn!(
            offering_id = %offering_id,
            error = ?e,
            "Failed to unregister scheduled tasks (non-fatal)"
        );
    }

    // Release static IP if this offering was a requester
    // (will revert to DHCP if no other requesters remain)
    let mut network_state = load_network_state().await;
    if network_state.requested_by.contains(&service_name) {
        if let Err(e) = revert_to_dhcp(&service_name, &mut network_state).await {
            tracing::warn!(
                service = %service_name,
                error = ?e,
                "Failed to release static IP (non-fatal)"
            );
        } else {
            let remaining = network_state.requester_count();
            if remaining == 0 {
                tracing::info!(service = %service_name, "Released static IP, reverted to DHCP");
            } else {
                tracing::info!(
                    service = %service_name,
                    remaining_requesters = remaining,
                    "Released static IP requester, other services still using it"
                );
            }
        }
    }

    let ctx = SuggestionContext::from_headers(&headers, "destroy_service");
    let suggestions = generate_suggestions(&ctx);

    Ok(Json(ApiResponse {
        data: ServiceActionResponse {
            service: service_name,
            action: "destroy".to_string(),
            status: "uprooted".to_string(),
            message: "Service destroyed (container removed)".to_string(),
        },
        suggestions,
    }))
}

// ============================================================================
// Services API - Technical Layer (New Endpoints)
// ============================================================================

/// GET /api/v1/services/manifests - List all service manifests
pub async fn list_manifests_v1(
    State(state): State<AppState>,
) -> Result<
    (
        StatusCode,
        Json<ApiResponse<Vec<crate::infra::manifests::TemplateInfo>>>,
    ),
    (StatusCode, Json<ApiErrorResponse>),
> {
    let manifests: Vec<_> = state
        .manifest_registry
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
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<(StatusCode, String), (StatusCode, Json<ApiErrorResponse>)> {
    let offering_fqn = OfferingFqn::parse(&name).map_err(|e| {
        error_response(
            StatusCode::BAD_REQUEST,
            "INVALID_SERVICE_NAME",
            format!("Invalid service name '{}': {}", name, e),
            None,
        )
    })?;
    let offering_type = offering_fqn.offering.clone();

    let entry = state
        .manifest_registry
        .sw
        .get(&offering_type)
        .ok_or_else(|| {
            error_response(
                StatusCode::NOT_FOUND,
                "MANIFEST_NOT_FOUND",
                format!("Manifest for '{}' not found", offering_type),
                None,
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
    State(_state): State<AppState>,
) -> Result<
    axum::response::sse::Sse<
        impl futures_util::stream::Stream<
            Item = Result<axum::response::sse::Event, std::convert::Infallible>,
        >,
    >,
    (StatusCode, Json<ApiErrorResponse>),
> {
    let service_name = normalize_service_name(&service)?;
    // TODO: Implement log streaming from Docker container
    use async_stream::stream;
    use axum::response::sse::{Event, KeepAlive, Sse};

    let log_stream = stream! {
        yield Ok(Event::default().data(format!("Log streaming for '{}' not yet implemented", service_name)));
    };

    Ok(Sse::new(log_stream).keep_alive(KeepAlive::default()))
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
        offerings.iter().find(|o| o.name.to_string() == service_name).cloned()
    };

    let offering = offering.ok_or_else(|| {
        error_response(
            StatusCode::NOT_FOUND,
            "SERVICE_NOT_FOUND",
            format!("Service '{}' not found", service_name),
            None,
        )
    })?;

    // For Docker-managed containers, inspect to get env vars
    if offering.is_managed() {
        match state.docker.inspect_container_spec(&service_name).await {
            Ok(spec) => {
                let env_map: std::collections::HashMap<String, String> = spec
                    .environment
                    .iter()
                    .filter_map(|e| {
                        let (k, v) = e.split_once('=')?;
                        Some((k.to_string(), v.to_string()))
                    })
                    .collect();
                Ok(Json(serde_json::json!({ "data": env_map })))
            }
            Err(e) => Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "ENV_FETCH_FAILED",
                format!("Failed to read env for '{}': {}", service_name, e),
                None,
            )),
        }
    } else {
        // Adopted/borrowed: no env access — return empty
        Ok(Json(serde_json::json!({ "data": {} })))
    }
}

/// POST /api/v1/services/:service:restart - Restart service
pub async fn restart_service_v1(
    State(state): State<AppState>,
    Path(service): Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<ApiErrorResponse>)> {
    let service_name = normalize_service_name(&service)?;
    // Stop then start
    state
        .docker
        .stop_service(&service_name, Some(&state.console))
        .await
        .map_err(|e| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "RESTART_FAILED",
                format!("Failed to stop service: {}", e),
                None,
            )
        })?;

    state
        .docker
        .start_service(&service_name, Some(&state.console))
        .await
        .map_err(|e| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "RESTART_FAILED",
                format!("Failed to start service: {}", e),
                None,
            )
        })?;

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

/// POST /api/v1/services/:service:cordon - Mark service unavailable
pub async fn cordon_service_v1(
    State(_state): State<AppState>,
    Path(service): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let service_name = match OfferingFqn::parse(&service) {
        Ok(fqn) => fqn.fqn(),
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "INVALID_SERVICE_NAME",
                    "message": format!("Invalid service name '{}': {}", service, e),
                })),
            );
        }
    };
    // TODO: Implement cordon logic (mark in registry, update status)
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({
            "error": "NOT_IMPLEMENTED",
            "message": "Cordon operation not yet implemented",
            "service": service_name
        })),
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

    // Persist changes if any adoptions or drops occurred
    if result.has_changes() {
        let _ = state.persist_offerings().await;
    }

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
    // Rebuild offerings index (which includes manifest validation)
    crate::ensure_offerings_index(&state, true)
        .await
        .map_err(|e| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "REFRESH_FAILED",
                format!("Failed to refresh manifests: {}", e),
                None,
            )
        })?;

    let idx_guard = state.offerings_index.read().await;
    let idx = idx_guard.as_ref().ok_or_else(|| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INDEX_UNAVAILABLE",
            "Manifests index unavailable after refresh".to_string(),
            None,
        )
    })?;

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "refreshed",
            "count": idx.offerings.len(),
            "fingerprint": idx.fingerprint,
            "generated_at": idx.generated_at
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
                error_response(
                    StatusCode::NOT_FOUND,
                    "SERVICE_NOT_FOUND",
                    format!("Service '{}' not found", service_name),
                    None,
                )
            })?
    };

    // Get capability manifest for this offering
    let cap_manifest = crate::infra::manifests::get_capability_manifest(&service.offering)
        .ok_or_else(|| {
            error_response(
                StatusCode::NOT_FOUND,
                "NO_CAPABILITY_MANIFEST",
                format!("No capability manifest found for '{}'", service.offering),
                None,
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
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DISCOVERY_FAILED",
                format!("Failed to discover capabilities: {}", e),
                None,
            )
        })?;

    // Convert to SubCapability format
    let capabilities: Vec<garden_common::SubCapability> =
        collections.iter().map(|c| c.to_sub_capability()).collect();

    // Update the offering in registry with discovered capabilities via gateway
    if !capabilities.is_empty() {
        let caps = capabilities.clone();
        state.update_offering_by_name(&service_name, false, |o| {
            o.sub_capabilities = caps;
            false // sub_capabilities are detail-only, don't trigger chirp sync
        }).await;
        let _ = state.persist_offerings().await;
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

    // Persist updated capabilities via gateway (detail-only, no chirp sync)
    if !updates.is_empty() {
        state.update_offerings_batch(|offerings| {
            let mut count = 0;
            for (name, sub_caps) in updates {
                if let Some(o) = offerings.iter_mut().find(|o| o.name.to_string() == name) {
                    o.sub_capabilities = sub_caps;
                    count += 1;
                }
            }
            count
        }, false).await;
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
            error_response(
                StatusCode::BAD_REQUEST,
                "INVALID_SERVICE_NAME",
                format!("Invalid service name '{}': {}", service, e),
                None,
            )
        })
}

/// Build a ContainerSpec from the manifest template + any config patches.
///
/// Uses the **compiled offerings index** for the image (which factors in
/// hardware capabilities like AVX detection) and the raw manifest for
/// command/config_files/ports (which are hardware-independent).
///
/// Used by both `compose_on_start` (drift convergence) and
/// `rebuild_missing_container` (dormant container reinstall).
async fn build_spec_from_manifest(
    state: &crate::AppState,
    service_name: &str,
) -> anyhow::Result<crate::docker::ContainerSpec> {
    use anyhow::Context;

    let patches = {
        let offerings = state.offerings.read().await;
        offerings
            .iter()
            .find(|o| o.name.to_string() == service_name && o.is_managed())
            .and_then(|o| o.managed_data())
            .map(|d| d.config_patches.clone())
            .unwrap_or_default()
    };

    // Resolve the offering type (strip instance suffix for FQN lookups)
    let offering_type = OfferingFqn::parse(service_name)
        .map(|fqn| fqn.offering.clone())
        .unwrap_or_else(|_| service_name.to_string());

    let manifest = state
        .manifest_registry
        .get_offering(service_name)
        .context("No manifest for offering")?;
    let template = manifest
        .parse_template()
        .context("Failed to parse template")?;

    let effective = crate::domain::config_compose::compose(&template, &patches)
        .context("Failed to compose config")?;

    // Use the compiled offerings index for the image — it applies hardware
    // capability resolution (e.g., AVX fallback: mongo:7 → mongo:4.4).
    // Fall back to the raw manifest image if the index is unavailable.
    let resolved_image =
        match crate::get_compiled_offering(state, &offering_type).await {
            Ok(Some(compiled)) => {
                if compiled.image != effective.image {
                    tracing::info!(
                        service = %service_name,
                        manifest_image = %effective.image,
                        resolved_image = %compiled.image,
                        "Using hardware-resolved image from compiled index"
                    );
                }
                compiled.image
            }
            Ok(None) => {
                tracing::debug!(
                    service = %service_name,
                    "No compiled offering found, using manifest image"
                );
                effective.image
            }
            Err(e) => {
                tracing::warn!(
                    service = %service_name,
                    error = ?e,
                    "Failed to read compiled offerings index, using manifest image"
                );
                effective.image
            }
        };

    Ok(crate::docker::ContainerSpec {
        image: resolved_image,
        command: effective.command,
        ports: effective.ports,
        environment: effective.environment,
        volumes: effective.volumes,
        config_files: effective.config_files,
    })
}

/// Reinstall a container for a registered offering whose Docker container is
/// missing. Rebuilds the spec from the manifest + config patches and calls
/// `install_service`, which preserves named Docker volumes on disk.
async fn rebuild_missing_container(
    state: &crate::AppState,
    service_name: &str,
) -> anyhow::Result<()> {
    use anyhow::Context;

    let spec = build_spec_from_manifest(state, service_name)
        .await
        .context("Failed to build container spec from manifest")?;

    state
        .docker
        .install_service(service_name, &spec, Some(&state.console))
        .await
        .context("Failed to install container")?;

    Ok(())
}

/// Compose-on-start: check if a stopped container needs to be recreated
/// to match the effective config (manifest + patches).
///
/// This ensures config patches are applied even after a container restart or
/// Moss daemon restart. If the container spec doesn't match the desired config,
/// it is removed and recreated.
async fn compose_on_start(state: &crate::AppState, service_name: &str) -> anyhow::Result<()> {
    use anyhow::Context;

    // Only run if there are config patches to apply
    let has_patches = {
        let offerings = state.offerings.read().await;
        offerings
            .iter()
            .find(|o| o.name.to_string() == service_name && o.is_managed())
            .and_then(|o| o.managed_data())
            .map(|d| !d.config_patches.is_empty())
            .unwrap_or(false)
    };

    if !has_patches {
        return Ok(());
    }

    let desired_spec = build_spec_from_manifest(state, service_name)
        .await
        .context("Failed to build spec for compose-on-start")?;

    // Check if container needs cycling
    match state.docker.needs_cycle(service_name, &desired_spec).await {
        Ok(true) => {
            tracing::info!(
                service = %service_name,
                "Compose-on-start: container spec mismatch, cycling"
            );
            state
                .docker
                .recreate_service(service_name, &desired_spec)
                .await
                .context("Failed to recreate container for compose-on-start")?;
        }
        Ok(false) => {
            tracing::debug!(
                service = %service_name,
                "Compose-on-start: container already matches desired config"
            );
        }
        Err(e) => {
            tracing::warn!(
                service = %service_name,
                error = ?e,
                "Compose-on-start: could not inspect container, will start as-is"
            );
        }
    }

    Ok(())
}
