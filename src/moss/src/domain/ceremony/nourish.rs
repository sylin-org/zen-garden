//! Nourish offering ceremony orchestrator
//!
//! Coordinates the three phases of offering nourishment:
//! 1. Collect - backup current state (harvest)
//! 2. Nourish - pull new image, recreate container
//! 3. Water - start service, verify health
//!
//! Handles ceremony lifecycle, journal persistence, and rollback.

use super::phases::{collect, nourish, water};
use super::types::{Ceremony, CeremonyState, Phase};
use crate::AppState;
use anyhow::Result;
use garden_common::manifests::CeremonyPolicy;

/// Execute a nourish-offering ceremony
///
/// Runs all three phases (collect, nourish, water) in sequence,
/// persisting state to the journal after each phase transition.
///
/// # Arguments
/// * `state` - Application state with all dependencies
/// * `ceremony` - The ceremony instance (will be mutated with progress)
/// * `offering` - Name of the offering to update
/// * `new_image` - Docker image to update to
/// * `policy` - Ceremony policy from the offering manifest
///
/// # Returns
/// * `Ok(())` - Ceremony completed successfully
/// * `Err` - Ceremony failed (may have been rolled back)
pub async fn execute_nourish_offering(
    state: &AppState,
    ceremony: &mut Ceremony,
    offering: &str,
    new_image: &str,
    policy: &CeremonyPolicy,
) -> Result<()> {
    // Initialize phases
    ceremony.phases = vec![
        Phase::new("collect"),
        Phase::new("nourish"),
        Phase::new("water"),
    ];

    // Mark ceremony as executing
    ceremony.start();
    persist_ceremony(state, ceremony).await?;

    tracing::info!(
        ceremony_id = %ceremony.id,
        offering,
        new_image,
        "Starting nourish-offering ceremony"
    );

    // === Phase 1: Collect ===
    // NOTE: Extract recklessly to avoid borrow conflict in async closure (2026-01-24)
    let recklessly = ceremony.options.recklessly;
    let harvest_id = execute_phase(state, ceremony, 0, async {
        collect::execute(state, offering, &policy.mode, recklessly).await
    })
    .await?;

    // Store harvest ID as artifact
    if let Some(ref id) = harvest_id {
        ceremony
            .artifacts
            .insert("harvest_id".to_string(), id.clone());
        persist_ceremony(state, ceremony).await?;
    }

    // === Phase 2: Nourish ===
    execute_phase(state, ceremony, 1, async {
        nourish::execute(state, offering, new_image)
            .await
            .map(|_| ())
    })
    .await?;

    // === Phase 3: Water ===
    // NOTE: Extract auto_rollback to avoid borrow conflict in async closure (2026-01-24)
    let auto_rollback = ceremony.options.auto_rollback;
    let water_result = execute_phase(state, ceremony, 2, async {
        water::execute(state, offering, harvest_id.as_deref(), auto_rollback)
            .await
            .map(|_| ())
    })
    .await;

    // Handle final state
    match water_result {
        Ok(_) => {
            ceremony.complete();
            tracing::info!(
                ceremony_id = %ceremony.id,
                offering,
                "Nourish ceremony completed successfully"
            );
        }
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("rolled back") {
                ceremony.rollback(&error_msg);
                tracing::warn!(
                    ceremony_id = %ceremony.id,
                    offering,
                    "Nourish ceremony rolled back"
                );
            } else {
                ceremony.fail(&error_msg);
                tracing::error!(
                    ceremony_id = %ceremony.id,
                    offering,
                    error = %error_msg,
                    "Nourish ceremony failed"
                );
            }
        }
    }

    persist_ceremony(state, ceremony).await?;

    // Return based on final state
    if ceremony.state == CeremonyState::Completed {
        Ok(())
    } else {
        anyhow::bail!("{}", ceremony.error.as_deref().unwrap_or("Ceremony failed"))
    }
}

/// Execute a single phase with proper state management
///
/// Handles:
/// - Setting phase to Running
/// - Persisting state before execution
/// - Marking Completed or Failed
/// - Persisting state after execution
/// - Advancing to next phase on success
async fn execute_phase<T, F>(
    state: &AppState,
    ceremony: &mut Ceremony,
    phase_index: usize,
    phase_fn: F,
) -> Result<T>
where
    F: std::future::Future<Output = Result<T>>,
{
    let phase_name = ceremony.phases[phase_index].name.clone();
    ceremony.current_phase = phase_index;

    // Mark phase as running
    ceremony.phases[phase_index].start();
    persist_ceremony(state, ceremony).await?;

    tracing::debug!(
        ceremony_id = %ceremony.id,
        phase = %phase_name,
        "Phase started"
    );

    // Execute the phase
    match phase_fn.await {
        Ok(result) => {
            ceremony.phases[phase_index].complete();
            persist_ceremony(state, ceremony).await?;

            tracing::debug!(
                ceremony_id = %ceremony.id,
                phase = %phase_name,
                "Phase completed"
            );

            Ok(result)
        }
        Err(e) => {
            let error_msg = e.to_string();
            ceremony.phases[phase_index].fail(&error_msg);
            ceremony.fail(&error_msg);
            persist_ceremony(state, ceremony).await?;

            tracing::error!(
                ceremony_id = %ceremony.id,
                phase = %phase_name,
                error = %error_msg,
                "Phase failed"
            );

            Err(e)
        }
    }
}

/// Persist ceremony state to journal
async fn persist_ceremony(state: &AppState, ceremony: &Ceremony) -> Result<()> {
    state.security.pond.ceremony.journal.persist(ceremony).await.map_err(|e| {
        tracing::error!(
            ceremony_id = %ceremony.id,
            error = %e,
            "Failed to persist ceremony state"
        );
        e
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phase_names() {
        let phases = vec![
            Phase::new("collect"),
            Phase::new("nourish"),
            Phase::new("water"),
        ];
        assert_eq!(phases[0].name, "collect");
        assert_eq!(phases[1].name, "nourish");
        assert_eq!(phases[2].name, "water");
    }
}
