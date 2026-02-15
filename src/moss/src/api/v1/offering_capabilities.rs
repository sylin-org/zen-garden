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
    api_utils::ApiErrorResponse, offerings::parse_offering_fqn, CapabilityCollection, Offering,
    OfferingMode, OfferingStatus, Ports, ServiceInfo, ServiceStatus,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Duration;
use urlencoding::encode;

use crate::api::responses::ApiResponse;
use crate::domain::{get_offering_port, topology, CapabilityExecutor};
use crate::infra::manifests::get_capability_manifest;
use crate::{error_response, AppState};

/// Response for capability listing
#[derive(Debug, Serialize, Deserialize)]
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
    let (offering, mode) = resolve_offering_for_capability(&state, &offering_name).await?;

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
            if let Some(o) = offerings
                .iter_mut()
                .find(|o| o.offering_id == offering.offering_id)
            {
                o.sub_capabilities = sub_caps;
            }
        }
        if let Err(e) = state.persist_offerings().await {
            tracing::error!(error = ?e, "Failed to persist offerings after capability discovery");
        }
    }

    Ok(Json(ApiResponse {
        data: CapabilitiesResponse {
            offering: service.name.clone(),
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

    /// If true, only validate without actually adding
    #[serde(default)]
    pub dry_run: bool,
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

/// Response for add capability operation (job-based)
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum AddCapabilityResponse {
    /// Capability already exists
    #[serde(rename = "exists")]
    AlreadyExists {
        offering: String,
        capability: String,
        cap_type: String,
        message: String,
    },

    /// Dry run - validation passed, would add
    #[serde(rename = "dry_run")]
    DryRun {
        offering: String,
        capability: String,
        cap_type: String,
        message: String,
    },

    /// Job already running - return current progress
    #[serde(rename = "in_progress")]
    InProgress {
        offering: String,
        capability: String,
        job_id: String,
        message: String,
    },

    /// Job started - return job_id for tracking
    #[serde(rename = "started")]
    Started {
        offering: String,
        capability: String,
        job_id: String,
        message: String,
    },
}

/// POST /api/v1/stone/offerings/:name/capabilities
///
/// Add a capability to an offering (e.g., pull a model for Ollama).
/// Creates a background job for the add operation.
///
/// # Path Parameters
/// - `name`: Offering name (e.g., "ollama")
///
/// # Request Body
/// ```json
/// {
///   "name": "llama2:7b",
///   "type": "model",  // optional
///   "dry_run": false  // optional - validate only
/// }
/// ```
///
/// # Response Variants
///
/// **Capability already exists:**
/// ```json
/// { "data": { "status": "exists", "offering": "ollama", "capability": "llama2:7b", ... } }
/// ```
///
/// **Dry run (validation):**
/// ```json
/// { "data": { "status": "dry_run", "offering": "ollama", "capability": "llama2:7b", ... } }
/// ```
///
/// **Job already running:**
/// ```json
/// { "data": { "status": "in_progress", "job_id": "...", ... } }
/// ```
///
/// **Job started:**
/// ```json
/// { "data": { "status": "started", "job_id": "...", ... } }
/// ```
pub async fn add_offering_capability_v1(
    State(state): State<AppState>,
    Path(offering_name): Path<String>,
    Json(request): Json<AddCapabilityRequest>,
) -> Result<Json<ApiResponse<AddCapabilityResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    use crate::{Job, JobStatus};

    // Find the service (managed or adopted)
    let (service, mode) = find_service_for_capability(&state, &offering_name).await?;

    // Get capability manifest
    let manifest = get_capability_manifest(&service.offering).ok_or_else(|| {
        error_response(
            StatusCode::NOT_FOUND,
            "NO_CAPABILITY_MANIFEST",
            format!(
                "No capability manifest found for offering '{}'.",
                service.offering
            ),
            None,
        )
    })?;

    // Determine capability type
    let cap_type = request.cap_type.clone().unwrap_or_else(|| {
        manifest
            .capabilities
            .first()
            .map(|c| c.cap_type.clone())
            .unwrap_or_else(|| "model".to_string())
    });

    // Find the capability definition
    let cap_def = manifest
        .capabilities
        .iter()
        .find(|c| c.cap_type == cap_type)
        .ok_or_else(|| {
            error_response(
                StatusCode::BAD_REQUEST,
                "UNKNOWN_CAPABILITY_TYPE",
                format!(
                    "Capability type '{}' not found in manifest for '{}'.",
                    cap_type, service.offering
                ),
                None,
            )
        })?;

    // Check if add operation is available
    if cap_def.add.as_ref().map(|a| !a.available).unwrap_or(true) {
        return Err(error_response(
            StatusCode::NOT_IMPLEMENTED,
            "ADD_NOT_SUPPORTED",
            format!(
                "Adding capabilities of type '{}' is not supported for '{}'.",
                cap_type, service.offering
            ),
            None,
        ));
    }

    // Case 1: Check if capability already exists
    let executor = CapabilityExecutor::new();
    let exists = executor
        .capability_exists(&service, manifest, mode, &cap_type, &request.name)
        .await
        .map_err(|e| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "CHECK_FAILED",
                format!("Failed to check existing capabilities: {}", e),
                None,
            )
        })?;

    if exists {
        return Ok(Json(ApiResponse {
            data: AddCapabilityResponse::AlreadyExists {
                offering: service.name.clone(),
                capability: request.name.clone(),
                cap_type: cap_type.clone(),
                message: format!(
                    "{} '{}' already exists for {}",
                    cap_type, request.name, service.name
                ),
            },
            suggestions: None,
        }));
    }

    // Case 2: Dry run - validation passed
    if request.dry_run {
        return Ok(Json(ApiResponse {
            data: AddCapabilityResponse::DryRun {
                offering: service.name.clone(),
                capability: request.name.clone(),
                cap_type: cap_type.clone(),
                message: format!(
                    "{} '{}' can be added to {}",
                    cap_type, request.name, service.name
                ),
            },
            suggestions: None,
        }));
    }

    // Case 3: Check for existing running add job for this capability
    let job_key = format!("add-capability-{}-{}", service.name, request.name);
    {
        let jobs = state.jobs.read().await;
        for (job_id, job) in jobs.iter() {
            if job_id.starts_with(&job_key)
                && matches!(job.status, JobStatus::Running | JobStatus::Pending)
            {
                return Ok(Json(ApiResponse {
                    data: AddCapabilityResponse::InProgress {
                        offering: service.name.clone(),
                        capability: request.name.clone(),
                        job_id: job_id.clone(),
                        message: format!(
                            "Add operation already in progress for {} '{}'",
                            cap_type, request.name
                        ),
                    },
                    suggestions: None,
                }));
            }
        }
    }

    // Case 4: Create job and spawn background task
    let job_id = format!("{}-{}", job_key, uuid::Uuid::now_v7());

    let job = Job {
        id: job_id.clone(),
        offerings: vec![request.name.clone()], // Track capability name
        status: JobStatus::Pending,
        completed: vec![],
        failed: std::collections::HashMap::new(),
        started_at: std::time::SystemTime::now(),
        completed_at: None,
    };

    state.jobs.write().await.insert(job_id.clone(), job);

    // Spawn background task
    let state_clone = state.clone();
    let job_id_clone = job_id.clone();
    let offering_clone = service.name.clone();
    let cap_name_clone = request.name.clone();
    let cap_type_clone = cap_type.clone();
    tokio::spawn(async move {
        crate::tasks::add_capability_task(
            &state_clone,
            &job_id_clone,
            &offering_clone,
            &cap_type_clone,
            &cap_name_clone,
        )
        .await;
    });

    tracing::info!(
        offering = %service.name,
        capability = %request.name,
        cap_type = %cap_type,
        job_id = %job_id,
        "Capability add job started"
    );

    Ok(Json(ApiResponse {
        data: AddCapabilityResponse::Started {
            offering: service.name.clone(),
            capability: request.name.clone(),
            job_id,
            message: format!("Adding {} '{}' to {}", cap_type, request.name, service.name),
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
            format!(
                "No capability manifest found for offering '{}'.",
                service.offering
            ),
            None,
        )
    })?;

    // Determine capability type
    let cap_type = query.cap_type.as_deref().unwrap_or_else(|| {
        manifest
            .capabilities
            .first()
            .map(|c| c.cap_type.as_str())
            .unwrap_or("model")
    });

    // Find the capability definition
    let cap_def = manifest
        .capabilities
        .iter()
        .find(|c| c.cap_type == cap_type)
        .ok_or_else(|| {
            error_response(
                StatusCode::BAD_REQUEST,
                "UNKNOWN_CAPABILITY_TYPE",
                format!(
                    "Capability type '{}' not found in manifest for '{}'.",
                    cap_type, service.offering
                ),
                None,
            )
        })?;

    // Check if remove operation is available
    if cap_def
        .remove
        .as_ref()
        .map(|r| !r.available)
        .unwrap_or(true)
    {
        return Err(error_response(
            StatusCode::NOT_IMPLEMENTED,
            "REMOVE_NOT_SUPPORTED",
            format!(
                "Removing capabilities of type '{}' is not supported for '{}'.",
                cap_type, service.offering
            ),
            None,
        ));
    }

    // Execute remove operation
    let executor = CapabilityExecutor::new();
    let result = executor
        .remove_capability(&service, manifest, mode, cap_type, &capability_name)
        .await
        .map_err(|e| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "REMOVE_FAILED",
                format!("Failed to remove capability: {}", e),
                None,
            )
        })?;

    if result.success {
        crate::domain::tools::capability_orchestrator::record_capability_removed(
            &state,
            &service.name,
            cap_type,
            &capability_name,
        )
        .await
        .map_err(|e| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "CAPABILITY_STATE_UPDATE_FAILED",
                format!("Capability removed but state update failed: {}", e),
                None,
            )
        })?;
    }

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

/// Request body for refreshing capabilities
#[derive(Debug, Deserialize)]
pub struct RefreshCapabilitiesRequest {
    /// Capability type to refresh (optional, refreshes all types if not specified)
    #[serde(rename = "type")]
    pub cap_type: Option<String>,

    /// If true, only report what would be refreshed without actually doing it
    #[serde(default)]
    pub dry_run: bool,
}

/// Response for refresh capabilities operation (job-based)
#[derive(Debug, Serialize)]
#[serde(tag = "status")]
pub enum RefreshCapabilitiesResponse {
    /// No capabilities found to refresh
    #[serde(rename = "no_updates")]
    NoUpdates {
        offering: String,
        cap_type: Option<String>,
        message: String,
    },

    /// Dry run - showing what would be refreshed
    #[serde(rename = "dry_run")]
    DryRun {
        offering: String,
        capabilities: Vec<CapabilityToRefresh>,
        total: usize,
    },

    /// Job already running - return current progress
    #[serde(rename = "in_progress")]
    InProgress {
        offering: String,
        job_id: String,
        progress_percent: u8,
        completed: usize,
        failed: usize,
        total: usize,
    },

    /// Job started - return job_id for tracking
    #[serde(rename = "started")]
    Started {
        offering: String,
        job_id: String,
        total: usize,
        message: String,
    },
}

/// Capability that would be refreshed (for dry run)
#[derive(Debug, Serialize, Clone)]
pub struct CapabilityToRefresh {
    pub name: String,
    pub cap_type: String,
}

/// Request body for mirroring capabilities between stones
#[derive(Debug, Deserialize)]
pub struct MirrorCapabilitiesRequest {
    /// Source stone name
    pub from: String,
    /// Destination stone name
    pub to: String,
    /// If true, only report what would be mirrored
    #[serde(default)]
    pub dry_run: bool,
}

/// Failure details for a mirrored capability
#[derive(Debug, Serialize, Deserialize)]
pub struct MirrorCapabilityFailure {
    pub name: String,
    pub cap_type: String,
    pub error: String,
}

/// Response for mirror capabilities operation
#[derive(Debug, Serialize, Deserialize)]
pub struct MirrorCapabilitiesResponse {
    pub offering: String,
    pub from: String,
    pub to: String,
    pub added: usize,
    pub skipped: usize,
    pub failed: usize,
    pub total: usize,
    pub dry_run: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failures: Vec<MirrorCapabilityFailure>,
}

/// POST /api/v1/stone/offerings/:name/capabilities/refresh
///
/// Refresh/update all capabilities for an offering (e.g., update all Ollama models to latest).
/// Creates a background job for the refresh operation.
///
/// # Path Parameters
/// - `name`: Offering name (e.g., "ollama")
///
/// # Request Body
/// ```json
/// {
///   "type": "model",  // optional - filter by capability type
///   "dry_run": false  // optional - preview only (no job created)
/// }
/// ```
///
/// # Response Variants
///
/// **No capabilities found:**
/// ```json
/// { "data": { "status": "no_updates", "offering": "ollama", "message": "..." } }
/// ```
///
/// **Dry run (preview):**
/// ```json
/// { "data": { "status": "dry_run", "offering": "ollama", "capabilities": [...], "total": 5 } }
/// ```
///
/// **Job already running:**
/// ```json
/// { "data": { "status": "in_progress", "job_id": "...", "progress_percent": 40, ... } }
/// ```
///
/// **Job started:**
/// ```json
/// { "data": { "status": "started", "job_id": "...", "total": 5, "message": "..." } }
/// ```
pub async fn refresh_offering_capabilities_v1(
    State(state): State<AppState>,
    Path(offering_name): Path<String>,
    Json(request): Json<RefreshCapabilitiesRequest>,
) -> Result<Json<ApiResponse<RefreshCapabilitiesResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    use crate::{Job, JobStatus};

    // Find the service (managed or adopted)
    let (service, mode) = find_service_for_capability(&state, &offering_name).await?;

    // Get capability manifest
    let manifest = get_capability_manifest(&service.offering).ok_or_else(|| {
        error_response(
            StatusCode::NOT_FOUND,
            "NO_CAPABILITY_MANIFEST",
            format!(
                "No capability manifest found for offering '{}'.",
                service.offering
            ),
            None,
        )
    })?;

    // List existing capabilities
    let executor = CapabilityExecutor::new();
    let collections = executor
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

    // Filter by type if specified
    let filtered_collections: Vec<_> = if let Some(ref cap_type) = request.cap_type {
        collections
            .into_iter()
            .filter(|c| c.cap_type == *cap_type)
            .collect()
    } else {
        collections
    };

    // Build list of capabilities to refresh (only those with add operation available)
    let mut capabilities_to_refresh: Vec<CapabilityToRefresh> = Vec::new();
    for collection in &filtered_collections {
        let cap_config = manifest.get_capability_type(&collection.cap_type);
        let can_refresh = cap_config
            .and_then(|c| c.add.as_ref())
            .map(|a| a.available)
            .unwrap_or(false);

        if can_refresh {
            for item in &collection.items {
                capabilities_to_refresh.push(CapabilityToRefresh {
                    name: item.name.clone(),
                    cap_type: collection.cap_type.clone(),
                });
            }
        }
    }

    let total = capabilities_to_refresh.len();

    // Case 1: No capabilities to refresh
    if total == 0 {
        let type_label = request.cap_type.as_deref().unwrap_or("capabilities");
        return Ok(Json(ApiResponse {
            data: RefreshCapabilitiesResponse::NoUpdates {
                offering: service.name.clone(),
                cap_type: request.cap_type.clone(),
                message: format!("No {} found for {}", type_label, service.name),
            },
            suggestions: None,
        }));
    }

    // Case 2: Dry run - return what would be refreshed
    if request.dry_run {
        return Ok(Json(ApiResponse {
            data: RefreshCapabilitiesResponse::DryRun {
                offering: service.name.clone(),
                capabilities: capabilities_to_refresh,
                total,
            },
            suggestions: None,
        }));
    }

    // Case 3: Check for existing running refresh job for this offering
    let job_key = format!("refresh-capabilities-{}", service.name);
    {
        let jobs = state.jobs.read().await;
        for (job_id, job) in jobs.iter() {
            // Check if this is a refresh job for the same offering and still running
            if job_id.starts_with(&job_key)
                && matches!(job.status, JobStatus::Running | JobStatus::Pending)
            {
                let completed = job.completed.len();
                let failed = job.failed.len();
                let job_total = job.offerings.len(); // offerings holds capability names for refresh jobs
                let progress = if job_total > 0 {
                    ((completed + failed) * 100 / job_total) as u8
                } else {
                    0
                };

                return Ok(Json(ApiResponse {
                    data: RefreshCapabilitiesResponse::InProgress {
                        offering: service.name.clone(),
                        job_id: job_id.clone(),
                        progress_percent: progress,
                        completed,
                        failed,
                        total: job_total,
                    },
                    suggestions: None,
                }));
            }
        }
    }

    // Case 4: Create new job and spawn background task
    let job_id = format!("{}-{}", job_key, uuid::Uuid::now_v7());

    // For refresh jobs, we use offerings to store capability names for progress tracking
    let capability_names: Vec<String> = capabilities_to_refresh
        .iter()
        .map(|c| c.name.clone())
        .collect();

    let job = Job {
        id: job_id.clone(),
        offerings: capability_names, // Repurposed: holds capability names for progress
        status: JobStatus::Pending,
        completed: vec![],
        failed: std::collections::HashMap::new(),
        started_at: std::time::SystemTime::now(),
        completed_at: None,
    };

    state.jobs.write().await.insert(job_id.clone(), job);

    // Spawn background task
    let state_clone = state.clone();
    let job_id_clone = job_id.clone();
    let offering_clone = service.name.clone();
    let cap_type_filter = request.cap_type.clone();
    tokio::spawn(async move {
        crate::tasks::refresh_capabilities_task(
            &state_clone,
            &job_id_clone,
            &offering_clone,
            cap_type_filter.as_deref(),
        )
        .await;
    });

    tracing::info!(
        offering = %service.name,
        job_id = %job_id,
        total = total,
        "Capabilities refresh job started"
    );

    Ok(Json(ApiResponse {
        data: RefreshCapabilitiesResponse::Started {
            offering: service.name.clone(),
            job_id,
            total,
            message: format!("Refresh started for {} capabilities", total),
        },
        suggestions: None,
    }))
}

/// POST /api/v1/stone/offerings/:name/capabilities/mirror
///
/// Mirror capabilities from one stone to another for the same offering instance.
/// The tended stone orchestrates by querying source and target, then adding missing
/// capabilities on the destination.
pub async fn mirror_offering_capabilities_v1(
    State(state): State<AppState>,
    Path(offering_name): Path<String>,
    Json(request): Json<MirrorCapabilitiesRequest>,
) -> Result<Json<ApiResponse<MirrorCapabilitiesResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    let offering_fqn = parse_offering_fqn(&offering_name).map_err(|e| {
        error_response(
            StatusCode::BAD_REQUEST,
            "INVALID_OFFERING_NAME",
            format!("Invalid offering name '{}': {}", offering_name, e),
            None,
        )
    })?;
    let offering_fqn = offering_fqn.fqn();

    let from = request.from.trim();
    let to = request.to.trim();

    if from.is_empty() || to.is_empty() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "MIRROR_REQUIRES_STONES",
            "Both 'from' and 'to' stones are required".to_string(),
            None,
        ));
    }

    if from.eq_ignore_ascii_case(to) {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "MIRROR_SAME_STONE",
            "Source and destination stones must be different".to_string(),
            None,
        ));
    }

    let from_endpoint = resolve_stone_endpoint(&state, from).await.ok_or_else(|| {
        error_response(
            StatusCode::NOT_FOUND,
            "STONE_NOT_FOUND",
            format!("Stone '{}' not found in topology cache", from),
            None,
        )
    })?;
    let to_endpoint = resolve_stone_endpoint(&state, to).await.ok_or_else(|| {
        error_response(
            StatusCode::NOT_FOUND,
            "STONE_NOT_FOUND",
            format!("Stone '{}' not found in topology cache", to),
            None,
        )
    })?;

    let client = Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .unwrap_or_else(|_| Client::new());

    let source_caps =
        fetch_remote_capabilities(&client, &from_endpoint, from, &offering_fqn).await?;
    let target_caps = fetch_remote_capabilities(&client, &to_endpoint, to, &offering_fqn).await?;

    let mut target_set: HashSet<(String, String)> = HashSet::new();
    for collection in &target_caps.capabilities {
        for item in &collection.items {
            target_set.insert((collection.cap_type.clone(), item.name.clone()));
        }
    }

    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut added = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;
    let mut total = 0usize;
    let mut failures: Vec<MirrorCapabilityFailure> = Vec::new();

    for collection in &source_caps.capabilities {
        let cap_type = collection.cap_type.clone();
        for item in &collection.items {
            let key = (cap_type.clone(), item.name.clone());
            if !seen.insert(key.clone()) {
                continue;
            }
            total += 1;

            if target_set.contains(&key) {
                skipped += 1;
                continue;
            }

            if request.dry_run {
                added += 1;
                continue;
            }

            match add_capability_to_stone(
                &client,
                &to_endpoint,
                &offering_fqn,
                &key.0,
                &key.1,
                request.dry_run,
            )
            .await
            {
                Ok(response) => match response {
                    AddCapabilityResponse::AlreadyExists { .. } => {
                        skipped += 1;
                    }
                    AddCapabilityResponse::DryRun { .. } => {
                        added += 1;
                    }
                    AddCapabilityResponse::InProgress { .. } => {
                        added += 1;
                    }
                    AddCapabilityResponse::Started { .. } => {
                        added += 1;
                    }
                },
                Err(error) => {
                    failed += 1;
                    failures.push(MirrorCapabilityFailure {
                        name: key.1.clone(),
                        cap_type: key.0.clone(),
                        error,
                    });
                }
            }
        }
    }

    let message = if request.dry_run {
        Some(format!("Dry run: {} capabilities would be mirrored", added))
    } else {
        Some(format!(
            "Mirror completed: {} added, {} skipped, {} failed",
            added, skipped, failed
        ))
    };

    Ok(Json(ApiResponse {
        data: MirrorCapabilitiesResponse {
            offering: offering_fqn,
            from: from.to_string(),
            to: to.to_string(),
            added,
            skipped,
            failed,
            total,
            dry_run: request.dry_run,
            message,
            failures,
        },
        suggestions: None,
    }))
}

async fn resolve_stone_endpoint(state: &AppState, stone_name: &str) -> Option<String> {
    if stone_name.eq_ignore_ascii_case(&state.stone_name) {
        let entry = state.self_entry.read().await;
        let base = entry.address.http_base();
        if base.contains("0.0.0.0") {
            Some(format!("http://127.0.0.1:{}", state.api_port))
        } else {
            Some(base)
        }
    } else {
        topology::get_stone_by_name(&state.topology_cache, stone_name)
            .await
            .map(|entry| entry.address.http_base())
    }
}

async fn fetch_remote_capabilities(
    client: &Client,
    endpoint: &str,
    stone_name: &str,
    offering: &str,
) -> Result<CapabilitiesResponse, (StatusCode, Json<ApiErrorResponse>)> {
    let offering_path = encode(offering);
    let url = format!(
        "{}/api/v1/stone/offerings/{}/capabilities",
        endpoint.trim_end_matches('/'),
        offering_path
    );

    let response = client.get(&url).send().await.map_err(|e| {
        error_response(
            StatusCode::BAD_GATEWAY,
            "REMOTE_UNREACHABLE",
            format!("Failed to reach stone '{}': {}", stone_name, e),
            None,
        )
    })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let message = serde_json::from_str::<ApiErrorResponse>(&body)
            .map(|err| err.error.message)
            .unwrap_or_else(|_| body);

        let code = if status.as_u16() == StatusCode::NOT_FOUND.as_u16() {
            "OFFERING_NOT_FOUND"
        } else {
            "REMOTE_ERROR"
        };

        let http_status = if status.as_u16() == StatusCode::NOT_FOUND.as_u16() {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::BAD_GATEWAY
        };

        return Err(error_response(
            http_status,
            code,
            format!(
                "Failed to fetch capabilities from '{}': {}",
                stone_name, message
            ),
            None,
        ));
    }

    let api_response: ApiResponse<CapabilitiesResponse> = response.json().await.map_err(|e| {
        error_response(
            StatusCode::BAD_GATEWAY,
            "REMOTE_PARSE_FAILED",
            format!("Failed to parse capabilities from '{}': {}", stone_name, e),
            None,
        )
    })?;

    Ok(api_response.data)
}

async fn add_capability_to_stone(
    client: &Client,
    endpoint: &str,
    offering: &str,
    cap_type: &str,
    capability: &str,
    dry_run: bool,
) -> Result<AddCapabilityResponse, String> {
    let offering_path = encode(offering);
    let url = format!(
        "{}/api/v1/stone/offerings/{}/capabilities",
        endpoint.trim_end_matches('/'),
        offering_path
    );

    let body = serde_json::json!({
        "name": capability,
        "type": cap_type,
        "dry_run": dry_run,
    });

    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        let message = serde_json::from_str::<ApiErrorResponse>(&text)
            .map(|err| err.error.message)
            .unwrap_or_else(|_| text);
        return Err(format!("{}: {}", status, message));
    }

    let api_response: ApiResponse<AddCapabilityResponse> = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse add response: {}", e))?;

    Ok(api_response.data)
}

/// Convert Offering to ServiceInfo for capability executor compatibility
async fn offering_to_service_info(offering: &Offering, state: &AppState) -> ServiceInfo {
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
        guidance: offering
            .managed_data()
            .and_then(|m| m.guidance.clone())
            .or_else(|| offering.adopted_data().and_then(|a| a.guidance.clone())),
    }
}

/// Helper to find a service (managed or adopted) for capability operations
async fn find_service_for_capability(
    state: &AppState,
    offering_name: &str,
) -> Result<(ServiceInfo, OfferingMode), (StatusCode, Json<ApiErrorResponse>)> {
    let (offering, mode) = resolve_offering_for_capability(state, offering_name).await?;
    let service = offering_to_service_info(&offering, state).await;
    Ok((service, mode))
}

async fn resolve_offering_for_capability(
    state: &AppState,
    offering_name: &str,
) -> Result<(Offering, OfferingMode), (StatusCode, Json<ApiErrorResponse>)> {
    let normalized = normalize_offering_selector(offering_name);
    let offering_fqn = parse_offering_fqn(&normalized).map_err(|e| {
        error_response(
            StatusCode::BAD_REQUEST,
            "INVALID_OFFERING_NAME",
            format!("Invalid offering name '{}': {}", offering_name, e),
            None,
        )
    })?;
    let offering_fqn_str = offering_fqn.fqn();

    let offerings = state.offerings.read().await;

    // If instance is explicitly provided, require exact match.
    if offering_fqn.instance.is_some() {
        let found = offerings
            .iter()
            .find(|o| o.name.eq_ignore_ascii_case(&offering_fqn_str))
            .cloned();
        return match found {
            Some(offering) => Ok((offering.clone(), offering.mode())),
            None => Err(error_response(
                StatusCode::NOT_FOUND,
                "OFFERING_NOT_FOUND",
                format!(
                    "Offering '{}' is not running on this stone.",
                    offering_fqn_str
                ),
                None,
            )),
        };
    }

    // If a default instance exists (exact offering name), prefer it.
    if let Some(default_instance) = offerings
        .iter()
        .find(|o| o.name.eq_ignore_ascii_case(&offering_fqn_str))
        .cloned()
    {
        return Ok((default_instance.clone(), default_instance.mode()));
    }

    // Otherwise, match by offering type.
    let matches: Vec<Offering> = offerings
        .iter()
        .filter(|o| o.offering.eq_ignore_ascii_case(&offering_fqn.offering))
        .cloned()
        .collect();

    if matches.is_empty() {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            "OFFERING_NOT_FOUND",
            format!(
                "Offering '{}' is not running on this stone.",
                offering_fqn.offering
            ),
            None,
        ));
    }

    // Prefer a single running instance when multiple exist.
    let running: Vec<Offering> = matches
        .iter()
        .filter(|&o| o.status == OfferingStatus::Running)
        .cloned()
        .collect();

    let selected = if running.len() == 1 {
        running
    } else {
        matches.clone()
    };

    if selected.len() == 1 {
        let offering = selected.into_iter().next().unwrap();
        return Ok((offering.clone(), offering.mode()));
    }

    let candidates: Vec<String> = selected.iter().map(|o| o.name.clone()).collect();
    Err(error_response(
        StatusCode::CONFLICT,
        "OFFERING_AMBIGUOUS",
        format!(
            "Offering '{}' matches multiple instances: {}",
            offering_fqn.offering,
            candidates.join(", ")
        ),
        None,
    ))
}

fn normalize_offering_selector(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.contains('@') {
        return trimmed.replace('@', ":");
    }
    trimmed.to_string()
}
