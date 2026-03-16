//! Collect phase - create harvest before nourishment
//!
//! Backs up the offering's state (container image + volumes) so we can
//! roll back if the update fails.

use crate::AppState;
use anyhow::Result;
use garden_common::manifests::CeremonyMode;

/// Execute the collect phase
///
/// Creates a harvest (backup) of the offering unless running recklessly.
/// Returns the harvest ID if created, None if skipped.
pub async fn execute(
    state: &AppState,
    offering: &str,
    ceremony_mode: &CeremonyMode,
    recklessly: bool,
) -> Result<Option<String>> {
    if recklessly {
        tracing::info!(offering, "Skipping collect phase (recklessly mode)");
        return Ok(None);
    }

    tracing::info!(offering, mode = ?ceremony_mode, "Starting collect phase");

    // Determine if we should commit the container image
    // Stateless services don't need image commits (no persistent state in container)
    let commit_image = *ceremony_mode != CeremonyMode::Stateless;

    // TODO: If quiesceable, run quiesce command before harvest
    // For now, we only support unsafe mode (container must be stopped) or stateless
    if *ceremony_mode == CeremonyMode::Quiesceable {
        tracing::warn!(
            offering,
            "Quiesceable mode not yet implemented, proceeding with unsafe snapshot"
        );
    }

    // Create the harvest via trait object (no infra import)
    let manifest = state
        .orchestration
        .nurturing
        .harvest_ops
        .create_harvest(offering, &state.current.stone.id, commit_image)
        .await?;

    tracing::info!(
        offering,
        harvest_id = %manifest.id,
        size = %manifest.format_size(),
        "Collect phase completed"
    );

    Ok(Some(manifest.id))
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_recklessly_skips_collect() {
        // Unit test would need mocked AppState
        // Integration tests in tests/ceremony_integration.rs
    }
}
