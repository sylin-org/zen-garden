//! Nourish phase - pull new image and recreate container
//!
//! The core update operation: pulls the new image, stops the old container,
//! removes it, and creates a new one with the same configuration but new image.
//! Config patches are composed into the new container spec to ensure they survive
//! image upgrades.

use crate::AppState;
use anyhow::{Context, Result};

/// Execute the nourish phase
///
/// Pulls the new image and recreates the container with the updated image.
/// Existing config patches are composed into the new container spec.
pub async fn execute(state: &AppState, offering: &str, new_image: &str) -> Result<()> {
    tracing::info!(offering, new_image, "Starting nourish phase");

    // Step 1: Pull the new image
    tracing::info!(offering, image = new_image, "Pulling new image");
    state
        .platform
        .docker
        .pull_image(new_image, Some(&state.console))
        .await
        .context("Failed to pull new image")?;

    // Step 2: Build the effective container spec from manifest + patches
    let spec = build_nourish_spec(state, offering, new_image).await?;

    // Step 3: Stop the old container
    tracing::info!(offering, "Stopping old container");
    state
        .platform
        .docker
        .stop_service(offering, Some(&state.console))
        .await
        .context("Failed to stop container")?;

    // Step 4: Remove the old container
    tracing::info!(offering, "Removing old container");
    state
        .platform
        .docker
        .remove_service(offering, Some(&state.console))
        .await
        .context("Failed to remove container")?;

    // Step 5: Create new container with composed spec
    tracing::info!(offering, new_image, "Creating new container");
    state
        .platform
        .docker
        .install_service(offering, &spec, Some(&state.console))
        .await
        .context("Failed to create new container")?;

    tracing::info!(
        offering,
        new_image,
        "Nourish phase completed - container recreated"
    );

    Ok(())
}

/// Build the container spec for a nourish operation.
///
/// Parses the manifest template, overrides the image with `new_image`,
/// and composes any existing config patches into the effective spec.
async fn build_nourish_spec(
    state: &AppState,
    offering: &str,
    new_image: &str,
) -> Result<crate::docker::ContainerSpec> {
    // Get existing config patches from the offering registry
    let patches = {
        let offerings = state.offerings.read().await;
        offerings
            .iter()
            .find(|o| o.name.to_string() == offering && o.is_managed())
            .and_then(|o| o.managed_data())
            .map(|d| d.config_patches.clone())
            .unwrap_or_default()
    };

    // Parse the manifest template
    let manifest = state.manifest_registry.get_offering(offering);

    if let Some(manifest) = manifest {
        let template = manifest
            .parse_template()
            .context("Failed to parse template")?;

        if !patches.is_empty() {
            // Compose manifest + patches, then override image with new_image
            let effective = crate::domain::config_compose::compose(&template, &patches)
                .context("Failed to compose config patches")?;

            Ok(crate::docker::ContainerSpec {
                image: new_image.to_string(),
                command: effective.command,
                ports: effective.ports,
                environment: effective.environment,
                volumes: effective.volumes,
                config_files: effective.config_files,
            })
        } else {
            // No patches — use template directly
            let ports = template.ports_vec();
            Ok(crate::docker::ContainerSpec {
                image: new_image.to_string(),
                command: template.command,
                ports,
                environment: template.environment,
                volumes: template.volumes,
                config_files: template.config_files,
            })
        }
    } else {
        // No manifest — best effort with empty config
        Ok(crate::docker::ContainerSpec {
            image: new_image.to_string(),
            command: None,
            ports: Vec::new(),
            environment: Vec::new(),
            volumes: Vec::new(),
            config_files: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    // Integration tests require Docker - see tests/ceremony_integration.rs
}
