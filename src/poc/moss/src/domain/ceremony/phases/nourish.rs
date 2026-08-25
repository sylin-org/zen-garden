//! Nourish phase - pull new image and recreate container
//!
//! The core update operation: pulls the new image, stops the old container,
//! removes it, and creates a new one with the same configuration but new image.
//! Config patches are composed into the new container spec to ensure they survive
//! image upgrades.

use crate::Moss;
use anyhow::{Context, Result};

/// Execute the nourish phase
///
/// Pulls the new image and recreates the container with the updated image.
/// Existing config patches are composed into the new container spec.
pub async fn execute(state: &Moss, offering: &str, new_image: &str) -> Result<()> {
    tracing::info!(offering, new_image, "Starting nourish phase");

    // Step 1: Pull the new image
    tracing::info!(offering, image = new_image, "Pulling new image");
    state
        .platform
        .container
        .pull_image(new_image, Some(&state.console))
        .await
        .context("Failed to pull new image")?;

    // Step 2: Build the effective container spec from manifest + patches
    let spec = build_nourish_spec(state, offering, new_image).await?;

    // Step 3: Stop the old container
    tracing::info!(offering, "Stopping old container");
    state
        .platform
        .container
        .stop_service(offering, Some(&state.console))
        .await
        .context("Failed to stop container")?;

    // Step 4: Remove the old container
    tracing::info!(offering, "Removing old container");
    state
        .platform
        .container
        .remove_service(offering, Some(&state.console))
        .await
        .context("Failed to remove container")?;

    // Step 5: Create new container with composed spec
    tracing::info!(offering, new_image, "Creating new container");
    state
        .platform
        .container
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
/// Uses `build_spec_from_manifest` (CompiledOffering + config patches) to get
/// the hardware-resolved spec, then overrides the image with `new_image`.
async fn build_nourish_spec(
    state: &Moss,
    offering: &str,
    new_image: &str,
) -> Result<crate::docker::ContainerSpec> {
    let mut spec =
        crate::domain::services_internal::build_spec_from_manifest(state, offering).await?;
    spec.image = new_image.to_string();
    Ok(spec)
}

#[cfg(test)]
mod tests {
    // Integration tests require Docker - see tests/ceremony_integration.rs
}
