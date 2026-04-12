//! Collect phase - create harvest before nourishment
//!
//! Backs up the offering's state (container image + volumes) so we can
//! roll back if the update fails.

use crate::Moss;
use crate::domain::traits::HarvestOps;
use anyhow::{Context, Result};
use garden_common::manifests::{CeremonyMode, CeremonyPolicy};

/// Execute the collect phase
///
/// Creates a harvest (backup) of the offering unless running recklessly.
/// For quiesceable offerings, runs quiesce/resume commands around the harvest
/// so the container stays running while data is frozen to disk.
/// Returns the harvest ID if created, None if skipped.
pub async fn execute(
    state: &Moss,
    offering: &str,
    policy: &CeremonyPolicy,
    recklessly: bool,
) -> Result<Option<String>> {
    if recklessly {
        tracing::info!(offering, "Skipping collect phase (recklessly mode)");
        return Ok(None);
    }

    tracing::info!(offering, mode = ?policy.mode, "Starting collect phase");

    // Determine if we should commit the container image
    // Stateless services don't need image commits (no persistent state in container)
    let commit_image = policy.mode != CeremonyMode::Stateless;

    // Quiesceable: freeze data before harvest, resume after
    if policy.mode == CeremonyMode::Quiesceable
        && let Some(ref quiesce) = policy.quiesce
    {
        tracing::info!(offering, cmd = ?quiesce.exec, "Running quiesce command");
        let (exit_code, output) = state
            .platform
            .container
            .exec_in_container(offering, &quiesce.exec, quiesce.timeout_seconds)
            .await
            .context("Failed to execute quiesce command")?;

        if exit_code != 0 {
            anyhow::bail!(
                "Quiesce command failed (exit {}): {}",
                exit_code,
                output.trim()
            );
        }
    }

    // Create the harvest via trait object (no infra import)
    let harvest_result = state
        .nurturing
        .harvest_ops
        .create_harvest(offering, &state.current.stone.id, commit_image)
        .await;

    // Quiesceable: always resume after harvest, even if harvest failed
    if policy.mode == CeremonyMode::Quiesceable
        && let Some(ref resume) = policy.resume
    {
        tracing::info!(offering, cmd = ?resume.exec, "Running resume command");
        match state
            .platform
            .container
            .exec_in_container(offering, &resume.exec, resume.timeout_seconds)
            .await
        {
            Ok((0, _)) => {}
            Ok((code, output)) => {
                tracing::warn!(
                    offering,
                    exit_code = code,
                    "Resume command returned non-zero: {}",
                    output.trim()
                );
            }
            Err(e) => {
                tracing::error!(offering, error = %e, "Resume command failed — manual intervention may be needed");
            }
        }
    }

    let manifest = harvest_result?;

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
        // Unit test would need mocked Moss
        // Integration tests in tests/ceremony_integration.rs
    }
}
