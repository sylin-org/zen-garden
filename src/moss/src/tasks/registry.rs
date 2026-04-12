//! Registry maintenance and startup background tasks
//!
//! - Gateway TTL reaping (periodic)
//! - Offering registry reconciliation with Docker state (startup)
//! - Offerings catalog building from runtime templates (startup)

use crate::tasks::backfill_missing_guidance;
use crate::tasks::task_scheduler::backfill_missing_tasks;
use crate::{AppState, adopt_existing_containers, ensure_offerings_index};
use garden_common::ServiceHealthStatus;
use garden_common::console::{ConsoleEvent, EventCategory, EventStatus};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use garden_common::console::ConsolePrinter;

/// Periodically reaps expired gateway entries from `tool.registry`.
///
/// Runs every 15 seconds (gateway TTL is 60s, orchestrators refresh every 30s).
/// Reaped entries are broadcast via SSE and UDP tools beacon so remote
/// stones learn about the removal promptly.
pub fn start_registry_maintenance(state: AppState, token: CancellationToken) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(15));
        interval.tick().await; // Skip first immediate tick

        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = token.cancelled() => {
                    tracing::debug!("Registry maintenance shutting down");
                    break;
                }
            }
            let events = state.tool.reap_expired_gateways().await;
            if !events.is_empty() {
                let reaped_deltas = events.iter().filter(|e| e.as_delta().is_some()).count();
                tracing::info!(
                    count = reaped_deltas,
                    "Registry maintenance: reaped expired gateway entries"
                );
                for event in &events {
                    if let Some(delta) = event.as_delta() {
                        tracing::info!(
                            fqid = %delta.fqid,
                            "{} gateway expired (stale)",
                            delta.fqid,
                        );
                    }
                }
                crate::domain::tool::projection::publish_events_for_state(&state, &events).await;
            }
        }
    });
}

/// Start registry loading and container adoption
///
/// Reconciles persisted offerings state with actual Docker state on startup.
pub fn start_registry_loader(state: AppState) {
    tokio::spawn(async move {
        // Reconcile existing offerings: if the container no longer exists, mark it offline
        // Snapshot managed offerings to avoid holding write lock during async Docker calls
        let managed_snapshot: Vec<(String, String)> = {
            let offerings = state.offerings.read().await;
            offerings
                .iter()
                .filter(|o| o.is_managed())
                .map(|o| (o.offering_id.clone(), o.name.to_string()))
                .collect()
        };
        let mut any_changed = false;
        for (offering_id, name) in managed_snapshot {
            if !state
                .platform
                .docker
                .zen_container_exists(&name)
                .await
                .unwrap_or(false)
            {
                state
                    .offerings
                    .update(&offering_id, |o| {
                        o.status = garden_common::OfferingStatus::Stopped;
                        o.health = ServiceHealthStatus::Offline;
                        true
                    })
                    .await;
                any_changed = true;
            }
        }
        if any_changed {
            crate::domain::topology::composition::sync_services(&state, true).await;
        }

        // Coalesce any duplicate offerings that accumulated from prior versions
        let coalesced = state.offerings.coalesce_duplicates().await;
        if coalesced > 0 {
            tracing::info!(coalesced, "Startup: removed duplicate offerings by FQN");
        }

        // Backfill missing guidance for services that were installed before guidance caching
        let backfilled = backfill_missing_guidance(&state).await;
        if backfilled > 0 {
            tracing::info!(
                count = backfilled,
                "Backfilled guidance for existing services"
            );
        }

        // Backfill missing scheduled tasks for existing services
        let tasks_backfilled = backfill_missing_tasks(&state).await;
        if tasks_backfilled > 0 {
            tracing::info!(
                count = tasks_backfilled,
                "Backfilled scheduled tasks for existing services"
            );
        }

        // Startup self-heal: adopt any existing zen-offering containers
        adopt_existing_containers(&state).await;
    });
}

/// Start offerings catalog builder
///
/// Builds the offerings index from runtime templates.
pub fn start_catalog_builder(state: AppState, console: Arc<ConsolePrinter>) {
    tokio::spawn(async move {
        tracing::info!("Building offerings catalog...");

        console.emit(ConsoleEvent::new(
            EventCategory::Manifests,
            EventStatus::Scanning,
            "Runtime templates".to_string(),
        ));

        match ensure_offerings_index(&state, false, &crate::domain::FileCatalogCache).await {
            Ok(_) => {
                let idx_guard = state.offerings_index.read().await;
                if let Some(idx) = idx_guard.as_ref() {
                    tracing::info!(
                        offerings_count = idx.offerings.len(),
                        "Offerings catalog loaded successfully"
                    );
                    console.emit(ConsoleEvent::new(
                        EventCategory::Manifests,
                        EventStatus::Loaded,
                        format!("{} manifests", idx.offerings.len()),
                    ));
                }
            }
            Err(e) => {
                tracing::warn!(error = ?e, "Failed to build offerings catalog");
                console.emit(ConsoleEvent::new(
                    EventCategory::Manifests,
                    EventStatus::Invalid,
                    "Catalog build failed".to_string(),
                ));
            }
        }
    });
}
