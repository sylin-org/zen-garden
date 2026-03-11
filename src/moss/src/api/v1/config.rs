//! Config Patches API — owned configuration overlays for managed services.
//!
//! External actors (orchestrators, admins) apply named config patches to managed
//! services. Moss composes the effective container config from manifest + patches
//! at every lifecycle event.
//!
//! ## Endpoints
//!
//! - `GET    /api/v1/stone/services/{service}/config`          — view patches + effective config
//! - `PATCH  /api/v1/stone/services/{service}/config`          — upsert a patch (by owner)
//! - `DELETE /api/v1/stone/services/{service}/config?owner=..` — remove a patch

use crate::domain::config_compose;
use crate::{error_response, AppState};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use garden_common::{
    api_utils::ApiErrorResponse,
    manifests::offering::ServiceTemplate,
    offerings::OfferingFqn,
    types::ConfigPatch,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Request / Response types
// ============================================================================

/// Request body for PATCH (upsert a config patch).
#[derive(Debug, Deserialize)]
pub struct PatchConfigRequest {
    pub owner: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub command: Option<Vec<String>>,
    #[serde(default)]
    pub environment: HashMap<String, String>,
    #[serde(default)]
    pub volumes: Vec<(String, String)>,
    /// Config file content keyed by container path.
    /// e.g., "/etc/mongod.conf" → "replication:\n  replSetName: zen-garden\n"
    #[serde(default)]
    pub config: HashMap<String, String>,
}

/// Query params for DELETE and GET.
#[derive(Debug, Deserialize)]
pub struct ConfigQuery {
    pub owner: Option<String>,
}

/// Response for config endpoints.
#[derive(Debug, Serialize)]
pub struct ConfigResponse {
    /// The effective container configuration after composing manifest + patches.
    pub effective: EffectiveConfigResponse,
    /// All patches currently applied to this service.
    pub patches: Vec<ConfigPatch>,
    /// The base image from the manifest template.
    pub base_template: String,
}

/// Effective config in API-friendly form.
#[derive(Debug, Serialize)]
pub struct EffectiveConfigResponse {
    pub image: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<Vec<String>>,
    pub environment: Vec<String>,
    pub volumes: Vec<(String, String)>,
    pub ports: Vec<(u16, u16)>,
}

// ============================================================================
// GET /api/v1/stone/services/{service}/config
// ============================================================================

pub async fn get_config_v1(
    State(state): State<AppState>,
    Path(service): Path<String>,
    Query(query): Query<ConfigQuery>,
) -> Result<Json<ConfigResponse>, (StatusCode, Json<ApiErrorResponse>)> {
    let service_name = normalize_service_name(&service)?;

    let patches = {
        let offerings = state.offerings.read().await;
        let offering = offerings
            .iter()
            .find(|o| o.name.fqn() == service_name && o.is_managed())
            .ok_or_else(|| {
                error_response(
                    StatusCode::NOT_FOUND,
                    "SERVICE_NOT_FOUND",
                    format!("Managed service '{}' not found", service_name),
                    None,
                )
            })?;

        offering
            .managed_data()
            .map(|d| d.config_patches.clone())
            .unwrap_or_default()
    };

    // If owner filter is specified, return only that owner's patch(es)
    let filtered_patches = if let Some(ref owner) = query.owner {
        patches
            .iter()
            .filter(|p| p.owner == *owner)
            .cloned()
            .collect()
    } else {
        patches.clone()
    };

    let template = get_service_template(&state, &service_name)?;

    let effective = config_compose::compose(&template, &patches).map_err(|e| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "COMPOSE_ERROR",
            format!("Failed to compose config: {}", e),
            None,
        )
    })?;

    Ok(Json(build_response(effective, filtered_patches, &template)))
}

// ============================================================================
// PATCH /api/v1/stone/services/{service}/config
// ============================================================================

pub async fn patch_config_v1(
    State(state): State<AppState>,
    Path(service): Path<String>,
    Json(request): Json<PatchConfigRequest>,
) -> Result<Json<ConfigResponse>, (StatusCode, Json<ApiErrorResponse>)> {
    let service_name = normalize_service_name(&service)?;

    if request.owner.is_empty() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "OWNER_REQUIRED",
            "The 'owner' field is required and cannot be empty".to_string(),
            None,
        ));
    }

    // Build the new ConfigPatch
    let new_patch = ConfigPatch {
        owner: request.owner.clone(),
        description: request.description,
        applied_at: chrono::Utc::now(),
        command: request.command,
        environment: request.environment,
        volumes: request.volumes,
        config: request.config,
    };

    // Get existing patches, validate, and build the updated list
    let patches_after = {
        let offerings = state.offerings.read().await;
        let offering = offerings
            .iter()
            .find(|o| o.name.fqn() == service_name && o.is_managed())
            .ok_or_else(|| {
                error_response(
                    StatusCode::NOT_FOUND,
                    "SERVICE_NOT_FOUND",
                    format!("Managed service '{}' not found", service_name),
                    None,
                )
            })?;

        let existing = offering
            .managed_data()
            .map(|d| d.config_patches.clone())
            .unwrap_or_default();

        // Validate against existing patches from OTHER owners
        config_compose::validate_patch(&existing, &new_patch).map_err(|e| {
            error_response(
                StatusCode::CONFLICT,
                "PATCH_CONFLICT",
                e.to_string(),
                None,
            )
        })?;

        // Replace existing from same owner, or append
        let mut updated: Vec<ConfigPatch> = existing
            .into_iter()
            .filter(|p| p.owner != new_patch.owner)
            .collect();
        updated.push(new_patch);

        updated
    };

    let template = get_service_template(&state, &service_name)?;

    // Compose effective config with the new patch list
    let effective = config_compose::compose(&template, &patches_after).map_err(|e| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "COMPOSE_ERROR",
            format!("Failed to compose config: {}", e),
            None,
        )
    })?;

    // Write patches to the offering via gateway (detail-only, no chirp sync)
    let patches = patches_after.clone();
    state.update_offering_by_name(&service_name, false, |o| {
        if o.is_managed() {
            if let Some(managed) = o.managed_data_mut() {
                managed.config_patches = patches;
            }
        }
        false // config patches are detail-only, don't trigger sync
    }).await;

    // Persist
    if let Err(e) = state.persist_offerings().await {
        tracing::error!(error = ?e, "Failed to persist offerings after config patch");
    }

    tracing::info!(
        service = %service_name,
        owner = %request.owner,
        "Config patch applied"
    );

    // If service is running, check if we need to cycle the container
    maybe_cycle_container(&state, &service_name, &effective, &patches_after).await;

    Ok(Json(build_response(effective, patches_after, &template)))
}

// ============================================================================
// DELETE /api/v1/stone/services/{service}/config?owner=...
// ============================================================================

pub async fn delete_config_v1(
    State(state): State<AppState>,
    Path(service): Path<String>,
    Query(query): Query<ConfigQuery>,
) -> Result<Json<ConfigResponse>, (StatusCode, Json<ApiErrorResponse>)> {
    let service_name = normalize_service_name(&service)?;

    let owner = query.owner.as_deref().ok_or_else(|| {
        error_response(
            StatusCode::BAD_REQUEST,
            "OWNER_REQUIRED",
            "Query parameter 'owner' is required for DELETE".to_string(),
            None,
        )
    })?;

    // Remove the patch
    let (patches_after, had_patch) = {
        let offerings = state.offerings.read().await;
        let offering = offerings
            .iter()
            .find(|o| o.name.fqn() == service_name && o.is_managed())
            .ok_or_else(|| {
                error_response(
                    StatusCode::NOT_FOUND,
                    "SERVICE_NOT_FOUND",
                    format!("Managed service '{}' not found", service_name),
                    None,
                )
            })?;

        let existing = offering
            .managed_data()
            .map(|d| d.config_patches.clone())
            .unwrap_or_default();

        let had_patch = existing.iter().any(|p| p.owner == owner);

        let updated: Vec<ConfigPatch> = existing
            .into_iter()
            .filter(|p| p.owner != owner)
            .collect();

        (updated, had_patch)
    };

    if !had_patch {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            "PATCH_NOT_FOUND",
            format!("No config patch from owner '{}' found", owner),
            None,
        ));
    }

    let template = get_service_template(&state, &service_name)?;

    // Compose effective config without the removed patch
    let effective = config_compose::compose(&template, &patches_after).map_err(|e| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "COMPOSE_ERROR",
            format!("Failed to compose config: {}", e),
            None,
        )
    })?;

    // Write updated patches via gateway (detail-only, no chirp sync)
    let patches = patches_after.clone();
    state.update_offering_by_name(&service_name, false, |o| {
        if o.is_managed() {
            if let Some(managed) = o.managed_data_mut() {
                managed.config_patches = patches;
            }
        }
        false // config patches are detail-only, don't trigger sync
    }).await;

    // Persist
    if let Err(e) = state.persist_offerings().await {
        tracing::error!(error = ?e, "Failed to persist offerings after config unpatch");
    }

    tracing::info!(
        service = %service_name,
        owner = %owner,
        "Config patch removed"
    );

    // If service is running, check if we need to cycle the container
    maybe_cycle_container(&state, &service_name, &effective, &patches_after).await;

    Ok(Json(build_response(effective, patches_after, &template)))
}

// ============================================================================
// Helpers
// ============================================================================

fn normalize_service_name(
    service: &str,
) -> Result<String, (StatusCode, Json<ApiErrorResponse>)> {
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

/// Resolve the `ServiceTemplate` for a managed offering from the manifest registry.
///
/// Accepts full FQN strings (e.g., `mongodb::prod`) — strips the instance part
/// to look up the base offering name in the manifest registry.
fn get_service_template(
    state: &AppState,
    name: &str,
) -> Result<ServiceTemplate, (StatusCode, Json<ApiErrorResponse>)> {
    // Manifest registry keys by base offering name (e.g., "mongodb"),
    // but callers pass the full FQN (e.g., "mongodb::prod").
    let base_name = OfferingFqn::parse(name)
        .map(|fqn| fqn.offering)
        .unwrap_or_else(|_| name.to_string());

    let manifest = state
        .manifest_registry
        .get_offering(&base_name)
        .ok_or_else(|| {
            error_response(
                StatusCode::NOT_FOUND,
                "TEMPLATE_NOT_FOUND",
                format!("No manifest template for '{}'", name),
                None,
            )
        })?;

    manifest.parse_template().map_err(|e| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "TEMPLATE_PARSE_ERROR",
            format!("Failed to parse template for '{}': {}", name, e),
            None,
        )
    })
}

/// Build a ConfigResponse from the composed effective config.
fn build_response(
    effective: config_compose::EffectiveConfig,
    patches: Vec<ConfigPatch>,
    template: &ServiceTemplate,
) -> ConfigResponse {
    ConfigResponse {
        effective: EffectiveConfigResponse {
            image: effective.image,
            command: effective.command,
            environment: effective.environment,
            volumes: effective.volumes,
            ports: effective.ports,
        },
        patches,
        base_template: template.image.clone(),
    }
}

/// Convert EffectiveConfig to ContainerSpec for Docker operations.
fn effective_to_container_spec(
    effective: &config_compose::EffectiveConfig,
) -> crate::docker::ContainerSpec {
    crate::docker::ContainerSpec {
        image: effective.image.clone(),
        command: effective.command.clone(),
        ports: effective.ports.clone(),
        environment: effective.environment.clone(),
        volumes: effective.volumes.clone(),
        config_files: effective.config_files.clone(),
    }
}

/// If the service is running, apply config changes using the least destructive method:
///
/// 1. **Config file changes** → write files, then restart or signal (no recreation)
/// 2. **Env/volume/command changes** → recreate container (Docker limitation)
/// 3. **Nothing changed** → no-op
async fn maybe_cycle_container(
    state: &AppState,
    service_name: &str,
    effective: &config_compose::EffectiveConfig,
    patches: &[garden_common::types::ConfigPatch],
) {
    let is_running = {
        let offerings = state.offerings.read().await;
        offerings
            .iter()
            .find(|o| o.name.fqn() == service_name)
            .map(|o| o.status == garden_common::OfferingStatus::Running)
            .unwrap_or(false)
    };

    if !is_running {
        return;
    }

    // Step 1: Write config files if any patches have config content
    let config_changed = write_config_files(service_name, &effective.config_files, patches).await;

    // Step 2: Check if container spec changes require recreation
    let desired_spec = effective_to_container_spec(effective);
    let needs_recreate = match state.platform.docker.needs_cycle(service_name, &desired_spec).await {
        Ok(needs) => needs,
        Err(e) => {
            tracing::warn!(service = %service_name, error = ?e, "Could not check if cycle needed");
            false
        }
    };

    if needs_recreate {
        // Container spec changed (env, volumes, command) — must recreate.
        // Resolve image via compiled offerings index for hardware capability
        // resolution (e.g., AVX fallback: mongo:7 → mongo:4.4).
        let mut resolved_spec = desired_spec;
        let offering_type = OfferingFqn::parse(service_name)
            .map(|fqn| fqn.offering.clone())
            .unwrap_or_else(|_| service_name.to_string());

        match crate::get_compiled_offering(state, &offering_type).await {
            Ok(Some(compiled)) if compiled.image != resolved_spec.image => {
                tracing::info!(
                    service = %service_name,
                    manifest_image = %resolved_spec.image,
                    resolved_image = %compiled.image,
                    "Using hardware-resolved image for container recreation"
                );
                resolved_spec.image = compiled.image;
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(
                    service = %service_name,
                    error = ?e,
                    "Failed to read compiled offerings index, using manifest image for recreation"
                );
            }
        }

        tracing::info!(service = %service_name, "Config change requires container recreation");
        if let Err(e) = state.platform.docker.recreate_service(service_name, &resolved_spec).await {
            tracing::error!(
                service = %service_name,
                error = ?e,
                "Failed to recreate container — patch is persisted and will converge on next start"
            );
        } else {
            tracing::info!(service = %service_name, "Container recreated successfully");
        }
    } else if config_changed {
        // Only config files changed — use the least destructive reload method
        apply_config_reload(state, service_name, &effective.config_files).await;
    } else {
        tracing::debug!(service = %service_name, "Container already matches desired config");
    }
}

/// Write config file content from patches to the host config directory.
/// Returns `true` if any file was actually changed.
async fn write_config_files(
    offering_name: &str,
    config_files: &[garden_common::manifests::offering::ConfigFileMapping],
    patches: &[garden_common::types::ConfigPatch],
) -> bool {
    let mut any_changed = false;

    for cf in config_files {
        // Merge content from all patches for this config file path
        let content = config_compose::merge_config_content(patches, &cf.path);
        let content = match content {
            Some(c) => c,
            None => continue, // No patches touch this file
        };

        let host_dir = garden_common::constants::paths::offering_config_dir(offering_name);
        let filename = std::path::Path::new(&cf.path)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| "config".to_string());
        let host_path = format!("{}/{}", host_dir, filename);

        // Read existing content to check if it actually changed
        let existing = tokio::fs::read_to_string(&host_path).await.unwrap_or_default();
        if existing == content {
            continue;
        }

        // Write the new content
        if let Err(e) = tokio::fs::create_dir_all(&host_dir).await {
            tracing::error!(path = %host_dir, error = ?e, "Failed to create config dir");
            continue;
        }
        if let Err(e) = tokio::fs::write(&host_path, &content).await {
            tracing::error!(path = %host_path, error = ?e, "Failed to write config file");
            continue;
        }

        tracing::info!(
            service = %offering_name,
            path = %host_path,
            container_path = %cf.path,
            bytes = content.len(),
            "Config file updated"
        );
        any_changed = true;
    }

    any_changed
}

/// Apply the appropriate reload method after config file changes.
/// Uses the reload policy declared in the manifest (restart or signal).
async fn apply_config_reload(
    state: &AppState,
    service_name: &str,
    config_files: &[garden_common::manifests::offering::ConfigFileMapping],
) {
    use garden_common::manifests::offering::ReloadPolicy;

    // Find the most appropriate reload policy from the changed config files.
    // If any file requires restart, use restart. Signal is only used if all files support it.
    let mut needs_restart = false;
    let mut signal: Option<String> = None;

    for cf in config_files {
        match &cf.reload {
            ReloadPolicy::Restart => {
                needs_restart = true;
            }
            ReloadPolicy::Signal(sig) => {
                signal = Some(sig.clone());
            }
        }
    }

    if needs_restart {
        tracing::info!(service = %service_name, "Restarting container for config file changes");
        if let Err(e) = state.platform.docker.restart_service(service_name).await {
            tracing::error!(
                service = %service_name,
                error = ?e,
                "Failed to restart container — config file is written and will take effect on next start"
            );
        }
    } else if let Some(sig) = signal {
        tracing::info!(service = %service_name, signal = %sig, "Sending signal for config reload");
        if let Err(e) = state.platform.docker.signal_container(service_name, &sig).await {
            tracing::error!(
                service = %service_name,
                error = ?e,
                "Failed to send signal — falling back to restart"
            );
            if let Err(e) = state.platform.docker.restart_service(service_name).await {
                tracing::error!(
                    service = %service_name,
                    error = ?e,
                    "Restart fallback also failed — config file is written and will take effect on next start"
                );
            }
        }
    }
}
