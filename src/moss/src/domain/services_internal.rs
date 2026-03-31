//! Service infrastructure helpers (shared between lifecycle and API handlers).
//!
//! These functions bridge domain decisions with infra execution:
//! spec composition, container rebuild, compose-on-start.

use anyhow::Context;
use garden_common::offerings::OfferingFqn;

use crate::AppState;

/// Build a Docker container spec from the offering manifest + config patches.
///
/// Applies hardware-aware image resolution from the compiled offerings index,
/// falling back to the raw manifest image if the index is unavailable.
pub async fn build_spec_from_manifest(
    state: &AppState,
    service_name: &str,
) -> anyhow::Result<crate::docker::ContainerSpec> {
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
    let fqn = OfferingFqn::parse(service_name)
        .context("Invalid offering FQN")?;
    let offering_type = fqn.offering.clone();

    let manifest = state
        .manifest_registry
        .get_offering(&offering_type)
        .context("No manifest for offering")?;
    let template = manifest
        .parse_template_for_fqn(&fqn)
        .context("Failed to parse template")?;

    let effective = crate::domain::config_compose::compose(&template, &patches)
        .context("Failed to compose config")?;

    // Use the compiled offerings index for the image — it applies hardware
    // capability resolution (e.g., AVX fallback: mongo:7 → mongo:4.4).
    // Fall back to the raw manifest image if the index is unavailable.
    let resolved_image = match crate::get_compiled_offering(
        state,
        &offering_type,
        &crate::infra::persistence::OsOfferingsCache,
    )
    .await
    {
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
pub async fn rebuild_missing_container(state: &AppState, service_name: &str) -> anyhow::Result<()> {
    let spec = build_spec_from_manifest(state, service_name)
        .await
        .context("Failed to build container spec from manifest")?;

    state
        .platform
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
pub async fn compose_on_start(state: &AppState, service_name: &str) -> anyhow::Result<()> {
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
    match state
        .platform
        .docker
        .needs_cycle(service_name, &desired_spec)
        .await
    {
        Ok(true) => {
            tracing::info!(
                service = %service_name,
                "Compose-on-start: container spec mismatch, cycling"
            );
            state
                .platform
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
