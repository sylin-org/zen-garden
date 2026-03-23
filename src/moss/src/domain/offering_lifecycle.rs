//! Offering lifecycle domain service (ARCH-0005 Issue 3).
//!
//! Single mutation gateway for all offering state changes. Every write to the
//! offerings registry flows through this module. Benefits:
//!
//! - **Consistency**: lock → mutate → persist → sync → event in one call
//! - **Observability**: every mutation emits a domain event
//! - **Testability**: thin methods that compose gateway + event
//! - **Locality**: all offering query patterns in one place
//!
//! ## Usage
//!
//! Handlers and tasks call these functions instead of `state.offerings.read()` /
//! `state.update_offering()` directly. The functions take `&AppState` because
//! the offering registry lives there (ARCH-0004).

use garden_common::{Offering, OfferingStatus};

use crate::domain::events::OfferingEvent;
use crate::AppState;

// ============================================================================
// Queries (read-only — no persistence, no events)
// ============================================================================

/// Find an offering by ID. Returns a clone (snapshot).
pub async fn find_by_id(state: &AppState, offering_id: &str) -> Option<Offering> {
    let offerings = state.offerings.read().await;
    offerings
        .iter()
        .find(|o| o.offering_id == offering_id)
        .cloned()
}

/// Find an offering by service name (FQN). Returns a clone (snapshot).
pub async fn find_by_name(state: &AppState, name: &str) -> Option<Offering> {
    let offerings = state.offerings.read().await;
    offerings.iter().find(|o| o.name.fqn_eq(name)).cloned()
}

/// Find a managed offering by service name. Returns (offering_id, offering clone).
pub async fn find_managed(state: &AppState, name: &str) -> Option<Offering> {
    let offerings = state.offerings.read().await;
    offerings
        .iter()
        .find(|o| o.name.fqn_eq(name) && o.is_managed())
        .cloned()
}

/// Snapshot the offering ID for a service name.
pub async fn id_for_name(state: &AppState, name: &str) -> Option<String> {
    let offerings = state.offerings.read().await;
    offerings
        .iter()
        .find(|o| o.name.fqn_eq(name))
        .map(|o| o.offering_id.clone())
}

/// Snapshot the offering ID for a managed service name.
pub async fn id_for_managed(state: &AppState, name: &str) -> Option<String> {
    let offerings = state.offerings.read().await;
    offerings
        .iter()
        .find(|o| o.name.fqn_eq(name) && o.is_managed())
        .map(|o| o.offering_id.clone())
}

/// List all offerings (snapshot).
pub async fn list_all(state: &AppState) -> Vec<Offering> {
    state.offerings.read().await.clone()
}

/// Check if an offering with the given name exists.
pub async fn exists(state: &AppState, name: &str) -> bool {
    let offerings = state.offerings.read().await;
    offerings.iter().any(|o| o.name.fqn_eq(name))
}

/// Check if an offering is in a given status.
pub async fn has_status(state: &AppState, name: &str, status: OfferingStatus) -> bool {
    let offerings = state.offerings.read().await;
    offerings
        .iter()
        .any(|o| o.name.fqn_eq(name) && o.status == status)
}

// ============================================================================
// Mutations (lock + mutate + persist + sync + event)
// ============================================================================

/// Insert or update an offering. Emits a domain event based on whether this
/// is a new registration or an update to an existing one.
pub async fn upsert(state: &AppState, offering: Offering) {
    let is_new = {
        let offerings = state.offerings.read().await;
        !offerings
            .iter()
            .any(|o| o.offering_id == offering.offering_id)
            && !offerings.iter().any(|o| o.name == offering.name)
    };

    let offering_id = offering.offering_id.clone();
    let name = offering.name.to_string();

    state.upsert_offering(offering, true).await;

    if is_new {
        state.event_bus.emit(OfferingEvent::deployed(
            &offering_id,
            &name,
            state.stone_name(),
            "", // image not known at upsert time
        ));
    }
}

/// Insert or update an offering without emitting an event.
/// Use for intermediate states (e.g., Installing placeholder before job starts).
pub async fn upsert_quiet(state: &AppState, offering: Offering) {
    state.upsert_offering(offering, true).await;
}

/// Remove an offering by ID. Emits `OfferingEvent::removed`.
pub async fn remove(state: &AppState, offering_id: &str, name: &str) {
    state.remove_offering(offering_id, true).await;

    state.event_bus.emit(OfferingEvent::removed(
        offering_id,
        name,
        state.stone_name(),
    ));
}

/// Remove an offering by name. Emits `OfferingEvent::removed`.
pub async fn remove_by_name(state: &AppState, name: &str) {
    if let Some(offering_id) = id_for_name(state, name).await {
        state.remove_service(name, true).await;

        state.event_bus.emit(OfferingEvent::removed(
            &offering_id,
            name,
            state.stone_name(),
        ));
    }
}

/// Update a single offering by ID. Returns true if changed.
/// The event must be emitted by the caller (operation-specific).
pub async fn update<F>(state: &AppState, offering_id: &str, mutator: F) -> bool
where
    F: FnOnce(&mut Offering) -> bool,
{
    state.update_offering(offering_id, true, mutator).await
}

/// Update a single offering by name. Returns true if changed.
/// The event must be emitted by the caller (operation-specific).
pub async fn update_by_name<F>(state: &AppState, name: &str, mutator: F) -> bool
where
    F: FnOnce(&mut Offering) -> bool,
{
    state.update_offering_by_name(name, true, mutator).await
}

/// Batch-update offerings. Returns count of changed offerings.
pub async fn batch_update<F>(state: &AppState, mutator: F) -> usize
where
    F: FnOnce(&mut Vec<Offering>) -> usize,
{
    state.update_offerings_batch(mutator, true).await
}

// ============================================================================
// Status transitions (mutation + event in one call)
// ============================================================================

/// Transition an offering to Running status. Emits `OfferingEvent::started`.
pub async fn mark_running(state: &AppState, offering_id: &str, name: &str) {
    let changed = state
        .update_offering(offering_id, true, |o| {
            o.status = OfferingStatus::Running;
            true
        })
        .await;

    if changed {
        state.event_bus.emit(OfferingEvent::started(
            offering_id,
            name,
            state.stone_name(),
        ));
    }
}

/// Transition an offering to Stopped status. Emits `OfferingEvent::stopped`.
pub async fn mark_stopped(state: &AppState, offering_id: &str, name: &str) {
    let changed = state
        .update_offering(offering_id, true, |o| {
            o.status = OfferingStatus::Stopped;
            true
        })
        .await;

    if changed {
        state.event_bus.emit(OfferingEvent::stopped(
            offering_id,
            name,
            state.stone_name(),
        ));
    }
}
