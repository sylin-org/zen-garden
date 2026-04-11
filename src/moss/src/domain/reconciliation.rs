//! Service reconciliation domain logic
//!
//! Handles reconciliation of container state with registry:
//! - Discovers unregistered containers
//! - Adopts valid containers into registry
//! - Optionally removes invalid containers (no matching template)
//!
//! This is pure domain logic - delegates I/O to infra layer.

use crate::domain::adopt_offering_container;
use crate::AppState;
use garden_common::console;

/// Reconcile container state with the registry
///
/// This function:
/// 1. Lists all zen-offering-* containers
/// 2. Identifies containers not in registry
/// 3. Attempts to adopt each unregistered container
/// 4. Optionally drops containers without matching templates
///
/// # Parameters
/// - `state`: Application state with registry and Docker access
/// - `drop_invalid`: If true, removes containers with no matching template
///
/// # Returns
/// `ReconciliationResult` containing:
/// - `adopted`: Successfully adopted offerings
/// - `dropped_invalid`: Containers removed (only if drop_invalid=true)
/// - `skipped_existing`: Containers already in registry
/// - `left_unregistered`: Containers left alone (no template, drop_invalid=false)
///
/// # Composability
/// This function modifies state (adds to registry, removes containers).
/// Callers are responsible for:
/// - Persisting registry changes (if adopted or dropped any)
/// - Emitting events
/// - HTTP response formatting
pub async fn reconcile_services(state: &AppState, drop_invalid: bool) -> ReconciliationResult {
    let existing = match state.platform.docker.list_zen_containers().await {
        Ok(list) => list,
        Err(e) => {
            tracing::error!(error = ?e, "Failed to list zen containers during reconciliation");
            return ReconciliationResult {
                error: Some(format!("Failed to list zen containers: {}", e)),
                ..Default::default()
            };
        }
    };

    // Snapshot cached capabilities once for all adoptions
    let cached_caps = state.current.capabilities.read().await.clone();
    let cached_caps_ref = cached_caps.as_ref();

    let mut adopted = Vec::new();
    let mut dropped_invalid = Vec::new();
    let mut skipped_existing = Vec::new();
    let mut left_unregistered = Vec::new();

    for offering in existing {
        // Check if already in registry (read lock, brief)
        {
            let offerings = state.offerings.read().await;
            if offerings.iter().any(|o| o.name.to_string() == offering) {
                skipped_existing.push(offering);
                continue;
            }
        }

        // Attempt adoption outside the lock (I/O-heavy)
        match adopt_offering_container(
            &state.platform.docker,
            &state.manifest_registry,
            &offering,
            &state.current.stone.name,
            cached_caps_ref,
        )
        .await
        {
            Ok(Some(adopted_offering)) => {
                // upsert_offering handles TOCTOU internally (idempotent insert)
                tracing::info!(offering = %offering, "Reconciliation: adopting unregistered container");
                state.offerings.upsert(adopted_offering).await;
                adopted.push(offering);
            }
            Ok(None) => {
                // "Invalid" in this context means: zen-offering-* container exists, but we have
                // no known template/manifest mapping for that offering.
                if drop_invalid {
                    tracing::warn!(offering = %offering, "Reconciliation: dropping invalid container (no matching template)");
                    match state
                        .platform
                        .docker
                        .remove_service(&offering, Some(&state.console))
                        .await
                    {
                        Ok(_) => {
                            dropped_invalid.push(offering.clone());
                            // Emit console event for dropped container
                            state.console.emit(console::ConsoleEvent::new(
                                console::EventCategory::Services,
                                console::EventStatus::Stopped,
                                format!("Dropped invalid: {}", offering),
                            ));
                        }
                        Err(e) => {
                            tracing::warn!(offering = %offering, error = ?e, "Failed to drop invalid container; leaving it alone");
                            left_unregistered.push(offering);
                        }
                    }
                } else {
                    tracing::debug!(offering = %offering, "Reconciliation: leaving unregistered container (no template, drop_invalid=false)");
                    left_unregistered.push(offering);
                }
            }
            Err(e) => {
                tracing::warn!(offering = %offering, error = ?e, "Reconciliation: adoption failed; leaving container alone");
                left_unregistered.push(offering);
            }
        }
    }

    ReconciliationResult {
        adopted,
        dropped_invalid,
        skipped_existing,
        left_unregistered,
        error: None,
    }
}

/// Result of service reconciliation operation
#[derive(Debug, Default)]
pub struct ReconciliationResult {
    /// Successfully adopted offerings
    pub adopted: Vec<String>,
    /// Containers removed (no matching template)
    pub dropped_invalid: Vec<String>,
    /// Containers already in registry (skipped)
    pub skipped_existing: Vec<String>,
    /// Containers left unregistered (no template, not dropped)
    pub left_unregistered: Vec<String>,
    /// Error message if reconciliation failed
    pub error: Option<String>,
}

impl ReconciliationResult {
    /// Check if any changes were made (adopted or dropped)
    pub fn has_changes(&self) -> bool {
        !self.adopted.is_empty() || !self.dropped_invalid.is_empty()
    }

    /// Check if reconciliation encountered an error
    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }
}
