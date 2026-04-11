//! Service lifecycle domain service (ARCH-0005 Issue 3).
//!
//! Single entry point for all service state transitions: stop, start,
//! restart, remove, destroy. Encapsulates the lock → execute → update →
//! persist → event sequence that was previously duplicated across handlers.
//!
//! ## Responsibilities
//!
//! - Validate preconditions (service exists, is managed)
//! - Execute infrastructure operations (Docker start/stop/remove)
//! - Update offering registry via gateway (auto-persists)
//! - Emit domain events
//! - Clean up side effects (scheduled tasks, static IP)
//!
//! ## Non-responsibilities
//!
//! - HTTP response formatting (handlers own that)
//! - Request parsing / validation (handlers own that)
//! - Background task spawning for install (job_executors owns that)

use anyhow::{Context, Result};
use garden_common::OfferingStatus;
use tracing::{error, info, warn};

use crate::domain::events::OfferingEvent;
use crate::AppState;

/// Result of a service lifecycle operation — carries metadata for the handler to format.
#[derive(Debug)]
pub struct LifecycleOutcome {
    pub offering_id: String,
    pub service_name: String,
}

/// Find a managed offering by service name. Returns the offering ID.
///
/// This is the common precondition check for all lifecycle operations.
/// Delegates to `offering_lifecycle::find_managed` (single source of truth).
pub async fn find_managed(state: &AppState, service_name: &str) -> Result<String> {
    crate::domain::offering_lifecycle::id_for_managed(state, service_name)
        .await
        .ok_or_else(|| anyhow::anyhow!("Service '{}' not found", service_name))
}

/// Find any offering by service name (managed or adopted/borrowed).
/// Delegates to `offering_lifecycle::id_for_name` (single source of truth).
pub async fn find_any(state: &AppState, service_name: &str) -> Result<String> {
    crate::domain::offering_lifecycle::id_for_name(state, service_name)
        .await
        .ok_or_else(|| anyhow::anyhow!("Service '{}' not found", service_name))
}

// ============================================================================
// Stop
// ============================================================================

/// Stop a running service. Docker container is stopped; offering status → Stopped.
pub async fn stop(state: &AppState, service_name: &str) -> Result<LifecycleOutcome> {
    let offering_id = find_managed(state, service_name).await?;

    state
        .platform
        .docker
        .stop_service(service_name, Some(&state.console))
        .await
        .context("Failed to stop container")?;

    state
        .offerings.update(&offering_id, |o| {
            o.status = OfferingStatus::Stopped;
            true
        })
        .await;

    state.event_bus.emit(OfferingEvent::stopped(
        &offering_id,
        service_name,
        state.stone_name(),
    ));

    Ok(LifecycleOutcome {
        offering_id,
        service_name: service_name.to_string(),
    })
}

// ============================================================================
// Cordon / Uncordon
// ============================================================================

/// Mark a service as cordoned (non-schedulable). The container keeps running
/// but placement logic excludes it from new work assignments.
pub async fn cordon(state: &AppState, service_name: &str) -> Result<LifecycleOutcome> {
    let offering_id = find_managed(state, service_name).await?;

    state
        .offerings.update(&offering_id, |o| {
            if o.status == OfferingStatus::Cordoned {
                return false; // already cordoned
            }
            o.status = OfferingStatus::Cordoned;
            true
        })
        .await;

    info!(service = %service_name, "Service cordoned");

    Ok(LifecycleOutcome {
        offering_id,
        service_name: service_name.to_string(),
    })
}

/// Remove cordon from a service, restoring it to running status.
pub async fn uncordon(state: &AppState, service_name: &str) -> Result<LifecycleOutcome> {
    let offering_id = find_managed(state, service_name).await?;

    state
        .offerings.update(&offering_id, |o| {
            if o.status != OfferingStatus::Cordoned {
                return false; // not cordoned
            }
            o.status = OfferingStatus::Running;
            true
        })
        .await;

    info!(service = %service_name, "Service uncordoned");

    Ok(LifecycleOutcome {
        offering_id,
        service_name: service_name.to_string(),
    })
}

// ============================================================================
// Start
// ============================================================================

/// Start a stopped service. Handles missing containers (self-heal reinstall)
/// and config-patch drift (compose-on-start).
pub async fn start(state: &AppState, service_name: &str) -> Result<LifecycleOutcome> {
    let offering_id = find_managed(state, service_name).await?;

    // Check if the Docker container still exists
    let container_exists = state
        .platform
        .docker
        .zen_container_exists(service_name)
        .await
        .unwrap_or(false);

    if !container_exists {
        // OFFER-0008: If the offering is already Installing (health monitor reconciliation
        // in-flight), skip the redundant reconcile to avoid a Docker "name already in use"
        // race. The health monitor will complete it.
        let current_status = {
            let offerings = state.offerings.read().await;
            offerings
                .iter()
                .find(|o| o.offering_id == offering_id)
                .map(|o| o.status)
        };
        if current_status == Some(OfferingStatus::Installing) {
            info!(
                service = %service_name,
                "Container missing but reconciliation already in-flight, waiting"
            );
            return Err(anyhow::anyhow!(
                "Service '{}' is being reconciled by the health monitor — try again shortly",
                service_name
            ));
        }

        // Self-heal: reconcile from manifest, preserving ports + volumes (OFFER-0008)
        info!(service = %service_name, "Container missing for registered offering, reconciling");
        let result =
            crate::domain::services_internal::reconcile_offering(state, service_name)
                .await
                .context("Container is missing and reconciliation failed")?;

        result.apply_port_updates(state, &offering_id).await;

        info!(service = %service_name, "Container reconciled successfully (data preserved)");
    } else {
        // Check if compose-on-start needed (config patches exist)
        let needs_compose = {
            let offerings = state.offerings.read().await;
            offerings
                .iter()
                .find(|o| o.offering_id == offering_id)
                .and_then(|o| o.managed_data())
                .map(|d| !d.config_patches.is_empty())
                .unwrap_or(false)
        };

        if needs_compose
            && let Err(e) =
                crate::domain::services_internal::compose_on_start(state, service_name).await
            {
                warn!(
                    service = %service_name,
                    error = ?e,
                    "Compose-on-start failed, falling back to normal start"
                );
            }

        // Start the container
        state
            .platform
            .docker
            .start_service(service_name, Some(&state.console))
            .await
            .context("Failed to start container")?;
    }

    state
        .offerings.update(&offering_id, |o| {
            o.status = OfferingStatus::Running;
            true
        })
        .await;

    state.event_bus.emit(OfferingEvent::started(
        &offering_id,
        service_name,
        state.stone_name(),
    ));

    Ok(LifecycleOutcome {
        offering_id,
        service_name: service_name.to_string(),
    })
}

// ============================================================================
// Restart
// ============================================================================

/// Restart a service (stop + start). No status transition in registry.
pub async fn restart(state: &AppState, service_name: &str) -> Result<LifecycleOutcome> {
    let offering_id = find_managed(state, service_name).await?;

    state
        .platform
        .docker
        .stop_service(service_name, Some(&state.console))
        .await
        .context("Failed to stop service")?;

    state
        .platform
        .docker
        .start_service(service_name, Some(&state.console))
        .await
        .context("Failed to start service")?;

    Ok(LifecycleOutcome {
        offering_id,
        service_name: service_name.to_string(),
    })
}

// ============================================================================
// Remove / Destroy (shared implementation, different event semantics)
// ============================================================================

/// Remove a service (soft delete). Container is stopped and removed (volumes preserved).
/// Cleans up scheduled tasks and static IP allocation.
pub async fn remove(state: &AppState, service_name: &str) -> Result<LifecycleOutcome> {
    remove_impl(state, service_name, false).await
}

/// Destroy a service (hard delete). Same as remove but signals permanent deletion.
pub async fn destroy(state: &AppState, service_name: &str) -> Result<LifecycleOutcome> {
    remove_impl(state, service_name, true).await
}

/// Shared implementation for remove and destroy.
///
/// `hard_delete` controls the event semantics: `false` emits `removed`,
/// `true` emits `destroyed`.
async fn remove_impl(
    state: &AppState,
    service_name: &str,
    hard_delete: bool,
) -> Result<LifecycleOutcome> {
    let offering_id = find_managed(state, service_name).await?;

    // Remove container (non-fatal — continue cleanup even if container is already gone)
    if let Err(e) = state
        .platform
        .docker
        .remove_service(service_name, Some(&state.console))
        .await
    {
        warn!(service = %service_name, error = ?e, "Container removal failed, continuing with registry cleanup");
    }

    // Remove from registry (auto-persists)
    state.offerings.remove(&offering_id).await;

    // Emit domain event
    let event = if hard_delete {
        OfferingEvent::destroyed(&offering_id, service_name, state.stone_name())
    } else {
        OfferingEvent::removed(&offering_id, service_name, state.stone_name())
    };
    state.event_bus.emit(event);

    // Cleanup: unregister scheduled tasks + release static IP
    cleanup_tasks(&offering_id).await;
    cleanup_static_ip(service_name).await;

    Ok(LifecycleOutcome {
        offering_id,
        service_name: service_name.to_string(),
    })
}

// ============================================================================
// Install (create service — the most complex lifecycle operation)
// ============================================================================

/// Result of an install attempt — the handler maps this to HTTP responses.
#[derive(Debug)]
pub enum InstallOutcome {
    /// Image-direct installation started (background job).
    ImageDirectStarted {
        service_name: String,
        job_id: String,
    },
    /// Existing container adopted into registry (self-heal).
    Adopted { service_name: String },
    /// Standard manifest-based installation started (background job).
    InstallStarted {
        service_name: String,
        job_id: String,
    },
    /// Service is under maintenance, retry later.
    Maintenance { service_name: String },
}

/// Install (create) a service. Handles all entry paths:
/// - Image-direct deployment (OFFER-0006)
/// - Self-heal container adoption
/// - Manifest-based installation with compatibility checks
pub async fn install(
    state: &AppState,
    offering_fqn: &mut garden_common::offerings::OfferingFqn,
) -> Result<InstallOutcome> {
    use garden_common::offerings::OfferingSource;

    let service_name = offering_fqn.fqn();

    // Route to the appropriate install path
    if offering_fqn.source == Some(OfferingSource::Image) {
        return install_image_direct(state, offering_fqn, &service_name).await;
    }

    if let Some(outcome) = try_adopt_existing(state, &service_name).await? {
        return Ok(outcome);
    }

    install_from_manifest(state, offering_fqn).await
}

/// Image-direct deployment: pull and run a Docker image by reference (OFFER-0006).
async fn install_image_direct(
    state: &AppState,
    offering_fqn: &garden_common::offerings::OfferingFqn,
    service_name: &str,
) -> Result<InstallOutcome> {
    use garden_common::utils::generate_guidv7;
    use garden_common::{
        ManagedData, Offering, OfferingLocation, OfferingModeData, OfferingStatus,
        ServiceHealthStatus,
    };

    let offering_type = offering_fqn.offering.clone();
    let image_ref = offering_fqn
        .image_ref
        .clone()
        .unwrap_or_else(|| offering_fqn.offering.clone());

    if crate::domain::offering_lifecycle::has_status(state, service_name, OfferingStatus::Maintenance).await {
        return Ok(InstallOutcome::Maintenance { service_name: service_name.to_string() });
    }

    let job_id = uuid::Uuid::now_v7().to_string();
    let job = crate::Job {
        id: job_id.clone(),
        offerings: vec![service_name.to_string()],
        status: crate::JobStatus::Pending,
        completed: vec![],
        failed: std::collections::HashMap::new(),
        started_at: std::time::SystemTime::now(),
        completed_at: None,
    };
    state.jobs.write().await.insert(job_id.clone(), job);

    let installing_offering = Offering {
        offering_id: generate_guidv7(),
        name: offering_fqn.clone(),
        offering: offering_type.clone(),
        category: String::new(),
        version: image_ref.rsplit_once(':').map(|(_, tag)| tag).unwrap_or("latest").to_string(),
        status: OfferingStatus::Installing,
        health: ServiceHealthStatus::Offline,
        sub_capabilities: Vec::new(),
        location: OfferingLocation {
            host: "localhost".to_string(),
            port: 0,
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
    state.offerings.upsert(installing_offering).await;

    let state = state.clone();
    let task_fqn = offering_fqn.clone();
    let task_image = image_ref;
    let task_job_id = job_id.clone();
    let task_svc_name = service_name.to_string();
    tokio::spawn(async move {
        crate::install_image_direct_task(&state, &task_job_id, &task_fqn, &task_image, &task_svc_name).await;
        tracing::debug!(fqn = %task_fqn, "Image-direct install task completed");
    });

    Ok(InstallOutcome::ImageDirectStarted {
        service_name: service_name.to_string(),
        job_id,
    })
}

/// Self-heal: adopt an orphaned zen-offering-* container not in the registry.
async fn try_adopt_existing(state: &AppState, service_name: &str) -> Result<Option<InstallOutcome>> {
    if !state.platform.docker.zen_container_exists(service_name).await.unwrap_or(false) {
        return Ok(None);
    }

    let in_registry = crate::domain::offering_lifecycle::exists(state, service_name).await;
    if in_registry {
        return Ok(None);
    }

    let cached_caps = state.current.capabilities.read().await.clone();
    if let Ok(Some(adopted_offering)) = crate::adopt_offering_container(
        &state.platform.docker,
        &state.manifest_registry,
        service_name,
        &state.current.stone.name,
        cached_caps.as_ref(),
    )
    .await
    {
        state.offerings.upsert(adopted_offering).await;
        return Ok(Some(InstallOutcome::Adopted { service_name: service_name.to_string() }));
    }

    Ok(None)
}

/// Manifest-based installation: resolve offering from catalog and deploy.
async fn install_from_manifest(
    state: &AppState,
    offering_fqn: &mut garden_common::offerings::OfferingFqn,
) -> Result<InstallOutcome> {
    use garden_common::utils::generate_guidv7;
    use garden_common::{
        ManagedData, Offering, OfferingLocation, OfferingModeData, OfferingStatus,
        ServiceHealthStatus,
    };

    let offering_type = offering_fqn.offering.clone();
    let mut service_name = offering_fqn.fqn();

    let compiled = crate::get_compiled_offering(
        state,
        &offering_type,
        &crate::infra::persistence::OsOfferingsCache,
    )
    .await?
    .ok_or_else(|| anyhow::anyhow!("Unknown offering: {}", offering_type))?;

    if compiled.compatibility.decision == garden_common::constants::COMPAT_FAIL {
        let reason = compiled.compatibility.reason.unwrap_or_else(|| "Unknown reason".to_string());
        anyhow::bail!("Offering is incompatible with this stone: {}", reason);
    }

    // Compatibility fallback renaming
    if offering_fqn.instance.is_none()
        && let Some(ref fallback_name) = compiled.compatibility.fallback_name
            && let Ok(adjusted) =
                garden_common::offerings::OfferingFqn::with_instance(&offering_type, fallback_name)
            {
                tracing::info!(
                    original = %service_name,
                    adjusted = %adjusted.fqn(),
                    reason = ?compiled.compatibility.reason,
                    "Compatibility fallback renamed offering instance"
                );
                service_name = adjusted.fqn();
                *offering_fqn = adjusted;
            }

    if crate::domain::offering_lifecycle::has_status(state, &service_name, OfferingStatus::Maintenance).await {
        return Ok(InstallOutcome::Maintenance { service_name });
    }

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

    let native_port = compiled.default_host_port();
    let offering_protocol = crate::domain::connection::infer_protocol_from_manifest_metadata(
        &offering_type,
        &compiled.category,
        state.manifest_registry.get_offering(&offering_type).and_then(|entry| entry.connection.as_ref()),
    );
    let installing_offering = Offering {
        offering_id: generate_guidv7(),
        name: offering_fqn.clone(),
        offering: offering_type.clone(),
        category: compiled.category.clone(),
        version: compiled.image.split(':').next_back().unwrap_or("latest").into(),
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
            guidance: None,
            ..Default::default()
        }),
        registered_at: chrono::Utc::now(),
        updated_at: None,
        orchestration: None,
    };
    state.offerings.upsert(installing_offering).await;

    let state = state.clone();
    let task_offering = offering_type;
    let task_svc_name = service_name.clone();
    let task_job_id = job_id.clone();
    tokio::spawn(async move {
        crate::install_service_task(&state, &task_job_id, &task_offering, &task_svc_name).await;
        tracing::debug!(offering = %task_offering, "Install task completed");
    });

    Ok(InstallOutcome::InstallStarted {
        service_name,
        job_id,
    })
}

// ============================================================================
// Nourish (upgrade)
// ============================================================================

/// Result of a nourish (upgrade) operation.
#[derive(Debug)]
pub enum NourishOutcome {
    /// Service is under maintenance, retry later.
    Maintenance,
    /// Service was upgraded successfully.
    Upgraded,
}

/// Nourish (upgrade) a service to the latest manifest version.
///
/// Loads the manifest template, pulls the new image, recreates the container,
/// and updates the offering registry. On failure, restores the previous status.
pub async fn nourish(state: &AppState, service_name: &str) -> Result<NourishOutcome> {
    // Find and validate the service
    let (offering_id, offering, old_version) = {
        let offerings = state.offerings.read().await;
        let o = offerings
            .iter()
            .find(|o| o.name.to_string() == service_name && o.is_managed())
            .ok_or_else(|| anyhow::anyhow!("Service '{}' not found", service_name))?;

        if o.status == OfferingStatus::Maintenance {
            return Ok(NourishOutcome::Maintenance);
        }

        (o.offering_id.clone(), o.offering.clone(), o.version.clone())
    };

    // Mark as Maintenance via gateway (syncs self_entry)
    state
        .offerings.update(&offering_id, |o| {
            o.status = OfferingStatus::Maintenance;
            true
        })
        .await;

    // Build container spec via CompiledOffering (hardware-resolved image,
    // device_requests, etc.) + config patches. Falls back to raw template
    // only if the compiled index is unavailable.
    let spec = match crate::domain::services_internal::build_spec_from_manifest(
        state,
        service_name,
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            // Restore status on spec build failure
            state
                .offerings.update(&offering_id, |o| {
                    o.status = OfferingStatus::Running;
                    true
                })
                .await;
            return Err(anyhow::anyhow!("Failed to build upgrade spec: {}", e));
        }
    };

    // Perform Docker upgrade
    if let Err(e) = state
        .platform
        .docker
        .upgrade_service(service_name, &spec, Some(&state.console))
        .await
    {
        error!(error = ?e, service = %service_name, "Docker upgrade failed");
        // Restore status on Docker failure
        state
            .offerings.update(&offering_id, |o| {
                o.status = OfferingStatus::Running;
                true
            })
            .await;
        return Err(anyhow::anyhow!("Failed to upgrade: {}", e));
    }

    let new_version = spec
        .image
        .split(':')
        .next_back()
        .unwrap_or("latest")
        .to_string();
    let new_image = spec.image.clone();

    // Update status and version via gateway (syncs self_entry + persists)
    let nv = new_version.clone();
    state
        .offerings.update(&offering_id, |o| {
            o.status = OfferingStatus::Running;
            o.version = nv;
            true
        })
        .await;

    // Emit offering lifecycle event (old_image reconstructed from old_version)
    let old_image = format!(
        "{}:{}",
        spec.image.split(':').next().unwrap_or(&offering),
        old_version
    );
    state.event_bus.emit(OfferingEvent::updated(
        &offering_id,
        service_name,
        state.stone_name(),
        &old_image,
        &new_image,
    ));

    Ok(NourishOutcome::Upgraded)
}

// ============================================================================
// Shared cleanup helpers
// ============================================================================

/// Unregister scheduled tasks for an offering (non-fatal).
///
/// NOTE: `TaskStore::new()` is lightweight (path construction only, no I/O or
/// connections). A per-call instance is acceptable until `TaskStore` is injected
/// via `AppState` as part of the ARCH-0005 trait boundary work.
async fn cleanup_tasks(offering_id: &str) {
    let task_store = crate::infra::task_store::TaskStore::new();
    if let Err(e) = task_store.unregister_tasks(offering_id).await {
        warn!(
            offering_id = %offering_id,
            error = ?e,
            "Failed to unregister scheduled tasks (non-fatal)"
        );
    }
}

/// Release static IP if this service was a requester (non-fatal).
async fn cleanup_static_ip(service_name: &str) {
    let mut network_state = crate::infra::network::load_network_state().await;
    if network_state.requested_by.iter().any(|r| r == service_name) {
        if let Err(e) =
            crate::infra::network::revert_to_dhcp(service_name, &mut network_state).await
        {
            warn!(
                service = %service_name,
                error = ?e,
                "Failed to release static IP (non-fatal)"
            );
        } else {
            let remaining = network_state.requester_count();
            if remaining == 0 {
                info!(service = %service_name, "Released static IP, reverted to DHCP");
            } else {
                info!(
                    service = %service_name,
                    remaining_requesters = remaining,
                    "Released static IP requester, other services still using it"
                );
            }
        }
    }
}
