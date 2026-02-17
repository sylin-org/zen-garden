//! Offering orchestration background task (ORCH-0001 Phase 3)
//!
//! Single background task managing the full offering orchestration lifecycle:
//! - Role assignment (Primary / Dormant / Joining / Degraded)
//! - Primary heartbeat monitoring via topology cache (chirps)
//! - Dual-primary resolution (deterministic: lower stone_id yields)
//! - Election triggering on primary absence
//! - Startup reconciliation (3s window before asserting Primary)
//! - Pin recovery on boot
//!
//! **Design principle**: emit `OfferingEvent::RoleChanged` once per transition;
//! downstream listeners (chirp, presence, tools) react automatically.

use anyhow::Result;
use chrono::Utc;
use garden_common::constants::orchestration::{
    DEGRADATION_CHECK_INTERVAL_SECS, FITNESS_HARD_CAP_MS,
};
use garden_common::election::{ElectionType, ScoreMechanism};
use garden_common::utils::ids::generate_guidv7;
use garden_common::{OfferingRole, OrchestrationState, StoneStatus};
use tokio_util::sync::CancellationToken;

use crate::app_state::AppState;
use crate::domain::events::OfferingEvent;

/// How long to wait during startup reconciliation before asserting Primary (ms).
const STARTUP_RECONCILIATION_MS: u64 = FITNESS_HARD_CAP_MS;

/// Primary heartbeat staleness threshold — twice the hard cap (ms).
/// If a primary's chirp hasn't been seen for this duration, trigger election.
const PRIMARY_STALE_THRESHOLD_MS: u64 = FITNESS_HARD_CAP_MS * 2;

/// Main loop tick interval (seconds).
const ORCHESTRATION_TICK_SECS: u64 = DEGRADATION_CHECK_INTERVAL_SECS;

// ============================================================================
// Public entry point
// ============================================================================

/// Background orchestration task — spawned at daemon startup.
///
/// Runs for the daemon's entire lifetime. Iterates offerings that have
/// orchestration state and dispatches the state-machine per role.
pub async fn offering_orchestration_task(state: AppState, token: CancellationToken) -> Result<()> {
    tracing::info!("Offering orchestration task starting");

    // Phase 1: Startup reconciliation
    startup_reconciliation(&state, &token).await?;

    // Phase 1.5: Backfill — assign initial roles to offerings missing orchestration state.
    // This handles pre-existing offerings that were deployed before ORCH-0001.
    backfill_orchestration(&state).await;

    // Phase 2: Pin recovery — re-elect pinned offerings
    pin_recovery(&state).await;

    // Phase 3: Main loop — periodic state-machine dispatch
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(ORCHESTRATION_TICK_SECS));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = tick.tick() => {}
            _ = token.cancelled() => {
                tracing::info!("Offering orchestration task shutting down");
                return Ok(());
            }
        }

        if let Err(e) = orchestration_tick(&state).await {
            tracing::error!(error = ?e, "Orchestration tick failed");
        }
    }
}

// ============================================================================
// Startup reconciliation
// ============================================================================

/// On restart, wait one election window before asserting Primary.
///
/// During this window, watch for chirps from other Stones that may have
/// legitimately taken over while this Stone was down. If another Stone is
/// already Primary for a given FQN, yield to it.
async fn startup_reconciliation(state: &AppState, token: &CancellationToken) -> Result<()> {
    let offerings = state.get_offerings().await;
    let orchestrated: Vec<_> = offerings
        .iter()
        .filter(|o| o.orchestration.is_some())
        .collect();

    if orchestrated.is_empty() {
        return Ok(());
    }

    tracing::info!(
        count = orchestrated.len(),
        window_ms = STARTUP_RECONCILIATION_MS,
        "Startup reconciliation: waiting before asserting Primary"
    );

    // Wait one election window (3s)
    tokio::select! {
        _ = tokio::time::sleep(std::time::Duration::from_millis(STARTUP_RECONCILIATION_MS)) => {}
        _ = token.cancelled() => return Ok(()),
    }

    // Check topology for each orchestrated offering
    for offering in &orchestrated {
        let orch = match &offering.orchestration {
            Some(o) => o,
            None => continue,
        };

        // Only reconcile offerings we think we're Primary for
        if orch.role != OfferingRole::Primary {
            continue;
        }

        let fqn = &offering.name;

        // Check if another stone is already claiming Primary for this FQN
        if let Some(other_primary_id) = find_remote_primary(state, fqn).await {
            tracing::info!(
                offering = %fqn,
                other_primary = %other_primary_id,
                "Startup reconciliation: yielding Primary to existing holder"
            );
            transition_role(state, &offering.offering_id, fqn, OfferingRole::Dormant).await?;
        }
    }

    Ok(())
}

// ============================================================================
// Backfill orchestration state
// ============================================================================

/// Assign initial orchestration roles to offerings loaded from disk that have
/// `orchestration: None`. This handles pre-existing offerings deployed before
/// ORCH-0001 was implemented.
///
/// Only backfills offerings where:
/// - `orchestration` is `None`
/// - The manifest is marked `replicable: true`
/// - The offering is in `Running` status
async fn backfill_orchestration(state: &AppState) {
    use garden_common::OfferingStatus;

    let offerings = state.get_offerings().await;
    let candidates: Vec<(String, String, String)> = offerings
        .iter()
        .filter(|o| o.orchestration.is_none() && o.status == OfferingStatus::Running)
        .map(|o| (o.offering_id.clone(), o.name.clone(), o.offering.clone()))
        .collect();

    if candidates.is_empty() {
        return;
    }

    // Check offerings index for replicable flag
    let replicable_types: std::collections::HashSet<String> = {
        let index_guard = state.offerings_index.read().await;
        match index_guard.as_ref() {
            Some(index) => index
                .offerings
                .iter()
                .filter(|co| co.replicable)
                .map(|co| co.name.clone())
                .collect(),
            None => {
                // Index not built yet — assume all are replicable (safe default)
                candidates.iter().map(|(_, _, t)| t.clone()).collect()
            }
        }
    };

    let mut count = 0u32;
    for (offering_id, fqn, offering_type) in &candidates {
        if !replicable_types.contains(offering_type) {
            continue;
        }

        if let Err(e) = assign_initial_role(state, offering_id, fqn).await {
            tracing::warn!(
                offering = %fqn,
                error = ?e,
                "Backfill: failed to assign orchestration role"
            );
        } else {
            count += 1;
        }
    }

    if count > 0 {
        tracing::info!(
            count,
            "Backfill: assigned initial orchestration roles to pre-existing offerings"
        );
    }
}

// ============================================================================
// Pin recovery
// ============================================================================

/// On startup, for any pinned offering, trigger a re-election.
/// Score 1001 guarantees victory.
async fn pin_recovery(state: &AppState) {
    let offerings = state.get_offerings().await;

    for offering in &offerings {
        let orch = match &offering.orchestration {
            Some(o) if o.pinned => o,
            _ => continue,
        };

        let fqn = &offering.name;
        tracing::info!(
            offering = %fqn,
            pinned_since = ?orch.pin_timestamp,
            "Pin recovery: triggering re-election for pinned offering"
        );

        let election_id = generate_guidv7();
        if let Err(e) = state
            .election_service
            .start_election(
                election_id,
                ElectionType::OfferingPrimary(fqn.clone()),
                serde_json::Value::Null,
                (FITNESS_HARD_CAP_MS / 1000).max(1),
                ScoreMechanism::Fitness,
            )
            .await
        {
            tracing::error!(offering = %fqn, error = ?e, "Pin recovery election failed");
        }
    }
}

// ============================================================================
// Main tick — state machine dispatch
// ============================================================================

/// One tick of the orchestration loop.
///
/// Iterates all offerings with orchestration state and dispatches by role.
async fn orchestration_tick(state: &AppState) -> Result<()> {
    let offerings = state.get_offerings().await;

    for offering in &offerings {
        let orch = match &offering.orchestration {
            Some(o) => o,
            None => continue,
        };

        let fqn = &offering.name;
        let offering_id = &offering.offering_id;

        match orch.role {
            OfferingRole::Primary => {
                dispatch_primary(state, offering_id, fqn, orch).await?;
            }
            OfferingRole::Dormant => {
                dispatch_dormant(state, offering_id, fqn, orch).await?;
            }
            OfferingRole::Joining => {
                // No-op until Phase 5 (sync). Joining implies the offering is
                // bootstrapping and not yet ready to participate.
            }
            OfferingRole::Degraded => {
                // Degraded stone waits. A dormant replica will detect the
                // degradation via chirps and trigger a fitness election.
                // No action needed here — the election result handler promotes
                // the winner.
            }
        }
    }

    Ok(())
}

// ============================================================================
// Role dispatchers
// ============================================================================

/// Primary: check for dual-primary conflicts.
async fn dispatch_primary(
    state: &AppState,
    offering_id: &str,
    fqn: &str,
    _orch: &OrchestrationState,
) -> Result<()> {
    // Dual-primary resolution: if another stone also claims Primary for this FQN,
    // the stone with the lexicographically lower stone_id yields.
    if let Some(other_primary_id) = find_remote_primary(state, fqn).await {
        if state.stone_id < other_primary_id {
            tracing::warn!(
                offering = %fqn,
                self_id = %state.stone_id,
                other_id = %other_primary_id,
                "Dual-primary detected: yielding (lower stone_id)"
            );
            transition_role(state, offering_id, fqn, OfferingRole::Dormant).await?;
        } else {
            tracing::debug!(
                offering = %fqn,
                other_id = %other_primary_id,
                "Dual-primary detected: retaining (higher stone_id)"
            );
        }
    }
    Ok(())
}

/// Dormant: watch primary heartbeat via topology cache.
async fn dispatch_dormant(
    state: &AppState,
    offering_id: &str,
    fqn: &str,
    orch: &OrchestrationState,
) -> Result<()> {
    // Find the primary's stone in topology
    let primary_stone_id = match &orch.primary_stone_id {
        Some(id) => id.clone(),
        None => {
            // No known primary — check topology for one
            if let Some(pid) = find_remote_primary(state, fqn).await {
                // Update our local state with the discovered primary
                update_primary_stone_id(state, offering_id, &pid).await?;
                pid
            } else {
                // No primary anywhere — promote self
                tracing::info!(
                    offering = %fqn,
                    "No primary found in topology; self-promoting"
                );
                transition_role(state, offering_id, fqn, OfferingRole::Primary).await?;
                return Ok(());
            }
        }
    };

    // Check primary staleness via topology cache
    let cache = state.topology_cache.read().await;
    if let Some(primary_entry) = cache.get(&primary_stone_id) {
        let staleness_ms = (Utc::now() - primary_entry.last_seen)
            .num_milliseconds()
            .unsigned_abs();

        // Also check if the primary's role for this FQN is degraded
        let primary_degraded = primary_entry
            .services
            .iter()
            .any(|svc| svc.name == fqn && svc.role.as_deref() == Some("degraded"));

        if staleness_ms > PRIMARY_STALE_THRESHOLD_MS || primary_degraded {
            drop(cache); // Release lock before election

            let reason = if primary_degraded {
                "primary degraded"
            } else {
                "primary stale (heartbeat timeout)"
            };

            tracing::warn!(
                offering = %fqn,
                primary_stone_id = %primary_stone_id,
                staleness_ms = staleness_ms,
                reason = reason,
                "Primary absent/degraded — triggering election"
            );

            let election_id = generate_guidv7();
            match state
                .election_service
                .start_election(
                    election_id,
                    ElectionType::OfferingPrimary(fqn.to_string()),
                    serde_json::Value::Null,
                    (FITNESS_HARD_CAP_MS / 1000).max(1),
                    ScoreMechanism::Fitness,
                )
                .await
            {
                Ok(Some(winner)) => {
                    handle_election_result(state, offering_id, fqn, &winner).await?;
                }
                Ok(None) => {
                    tracing::warn!(offering = %fqn, "Election completed with no winner");
                }
                Err(e) => {
                    tracing::error!(offering = %fqn, error = ?e, "Election failed");
                }
            }
        }
    } else {
        drop(cache);

        // Primary stone not in topology at all — trigger election
        tracing::warn!(
            offering = %fqn,
            primary_stone_id = %primary_stone_id,
            "Primary stone absent from topology — triggering election"
        );

        let election_id = generate_guidv7();
        match state
            .election_service
            .start_election(
                election_id,
                ElectionType::OfferingPrimary(fqn.to_string()),
                serde_json::Value::Null,
                (FITNESS_HARD_CAP_MS / 1000).max(1),
                ScoreMechanism::Fitness,
            )
            .await
        {
            Ok(Some(winner)) => {
                handle_election_result(state, offering_id, fqn, &winner).await?;
            }
            Ok(None) => {
                tracing::warn!(offering = %fqn, "Election completed with no winner");
            }
            Err(e) => {
                tracing::error!(offering = %fqn, error = ?e, "Election failed");
            }
        }
    }

    Ok(())
}

// ============================================================================
// Election result handling
// ============================================================================

/// Process an election result for this offering.
async fn handle_election_result(
    state: &AppState,
    offering_id: &str,
    fqn: &str,
    winner: &garden_common::election::ElectionWinner,
) -> Result<()> {
    if winner.stone_id == state.stone_id {
        tracing::info!(offering = %fqn, "Election won — promoting to Primary");
        transition_role(state, offering_id, fqn, OfferingRole::Primary).await?;
    } else {
        tracing::info!(
            offering = %fqn,
            winner_id = %winner.stone_id,
            "Election lost — transitioning to Dormant"
        );
        transition_role(state, offering_id, fqn, OfferingRole::Dormant).await?;
        update_primary_stone_id(state, offering_id, &winner.stone_id).await?;
    }
    Ok(())
}

// ============================================================================
// Role transition — single point of truth
// ============================================================================

/// Transition an offering's orchestration role.
///
/// Updates the in-memory state, persists, and emits `OfferingEvent::RoleChanged`.
/// All downstream effects (chirps, presence events, tools projection) are driven
/// by the event bus — zero manual wiring.
async fn transition_role(
    state: &AppState,
    offering_id: &str,
    fqn: &str,
    new_role: OfferingRole,
) -> Result<()> {
    let old_role = {
        let mut offerings = state.offerings.write().await;
        let offering = offerings.iter_mut().find(|o| o.offering_id == offering_id);

        match offering {
            Some(o) => {
                let orch = o
                    .orchestration
                    .get_or_insert_with(OrchestrationState::default);
                let old = orch.role.clone();

                if old == new_role {
                    return Ok(()); // No-op
                }

                orch.role = new_role.clone();

                // If becoming Primary, record self as primary_stone_id
                if new_role == OfferingRole::Primary {
                    orch.primary_stone_id = Some(state.stone_id.clone());
                }

                o.touch();
                old
            }
            None => {
                tracing::warn!(offering_id = %offering_id, "Offering not found for role transition");
                return Ok(());
            }
        }
    };

    // Persist outside the lock
    if let Err(e) = state.persist_offerings().await {
        tracing::error!(error = ?e, "Failed to persist offerings after role transition");
    }

    // Sync topology self-entry to reflect the new role in chirps
    state.sync_self_services(true).await;

    tracing::info!(
        offering = %fqn,
        old_role = %old_role,
        new_role = %new_role,
        "Role transition complete"
    );

    // Emit event — drives chirps, presence stream, tools projector
    state.event_bus.emit(OfferingEvent::role_changed(
        offering_id,
        fqn,
        &state.stone_id,
        old_role,
        new_role,
    ));

    Ok(())
}

// ============================================================================
// Helpers
// ============================================================================

/// Update the `primary_stone_id` on an offering's orchestration state.
async fn update_primary_stone_id(
    state: &AppState,
    offering_id: &str,
    primary_id: &str,
) -> Result<()> {
    let mut offerings = state.offerings.write().await;
    if let Some(o) = offerings.iter_mut().find(|o| o.offering_id == offering_id) {
        if let Some(ref mut orch) = o.orchestration {
            orch.primary_stone_id = Some(primary_id.to_string());
        }
    }
    Ok(())
}

/// Find a remote stone (not self) that claims Primary for the given FQN.
///
/// Scans the topology cache for online stones with a matching service entry
/// whose role is "primary". Returns the stone_id of the first match.
async fn find_remote_primary(state: &AppState, fqn: &str) -> Option<String> {
    let cache = state.topology_cache.read().await;

    for (stone_id, entry) in cache.iter() {
        // Skip self
        if stone_id == &state.stone_id {
            continue;
        }
        // Only consider online stones
        if entry.status != StoneStatus::Online {
            continue;
        }
        // Check if this stone has the offering with role "primary"
        for svc in &entry.services {
            if svc.name == fqn && svc.role.as_deref() == Some("primary") {
                return Some(stone_id.clone());
            }
        }
    }

    None
}

/// Assign initial orchestration state for a newly deployed offering.
///
/// Call this after deployment completes (offering status = Running).
/// - If no other stone has the same FQN as Primary → Primary
/// - If another stone already has it → Joining
pub async fn assign_initial_role(state: &AppState, offering_id: &str, fqn: &str) -> Result<()> {
    let new_role = if find_remote_primary(state, fqn).await.is_some() {
        OfferingRole::Joining
    } else {
        OfferingRole::Primary
    };

    tracing::info!(
        offering = %fqn,
        role = %new_role,
        "Assigning initial orchestration role"
    );

    // Set orchestration state directly (no event for initial assignment — deploy event suffices)
    {
        let mut offerings = state.offerings.write().await;
        if let Some(o) = offerings.iter_mut().find(|o| o.offering_id == offering_id) {
            let primary_id = if new_role == OfferingRole::Primary {
                Some(state.stone_id.clone())
            } else {
                find_remote_primary(state, fqn).await
            };

            o.orchestration = Some(OrchestrationState {
                role: new_role,
                primary_stone_id: primary_id,
                pinned: false,
                pin_timestamp: None,
            });
            o.touch();
        }
    }

    if let Err(e) = state.persist_offerings().await {
        tracing::error!(error = ?e, "Failed to persist offerings after initial role assignment");
    }
    state.sync_self_services(true).await;

    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primary_stale_threshold() {
        // Primary stale threshold should be 2× the hard cap
        assert_eq!(PRIMARY_STALE_THRESHOLD_MS, FITNESS_HARD_CAP_MS * 2);
        assert_eq!(PRIMARY_STALE_THRESHOLD_MS, 6_000);
    }

    #[test]
    fn test_startup_reconciliation_window() {
        // Reconciliation window equals the hard cap
        assert_eq!(STARTUP_RECONCILIATION_MS, FITNESS_HARD_CAP_MS);
        assert_eq!(STARTUP_RECONCILIATION_MS, 3_000);
    }

    // Note: Full state-machine tests require AppState mocking which is
    // covered by integration tests. Unit tests here validate constants
    // and pure logic. The resolve_fitness_election tests live in
    // election_service.rs. Fitness scoring tests live in domain/fitness.rs.
}
