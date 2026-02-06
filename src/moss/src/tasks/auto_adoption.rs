//! Auto-adoption background task
//!
//! Continuous adoption loop that:
//! - Scans immediately on startup with aggressive intervals for fast detection
//! - Gradually transitions to longer intervals as system stabilizes
//! - Detects services configured for adopted mode
//! - Adopts stable detected services automatically
//! - Validates health of already-adopted offerings (marks unhealthy/healthy)
//! - Respects stability thresholds and exclusion rules
//!
//! This is a non-blocking background task that runs for the lifetime of the daemon.

use crate::domain::connection;
use crate::domain::DetectionOrchestrator;
use crate::infra::config::AdoptionConfig;
use crate::AppState;
use garden_common::{OfferingMode, ServiceHealthStatus};
use std::time::Instant;

/// Background auto-adoption loop
///
/// This task should be spawned with tokio::spawn() at daemon startup.
/// Uses configurable tiered intervals that start aggressive and gradually relax.
///
/// # Non-Blocking
/// This function never returns - it's designed to run in the background
/// for the entire daemon lifetime. Spawn it and forget it.
///
/// # Detection Strategy (configurable)
/// Default schedule: `[[10, 600], [30, -1]]`
/// - Phase 1: 10-second intervals for first 10 minutes
/// - Phase 2: 30-second intervals forever after
///
/// Schedule format: `[(interval_secs, duration_secs), ...]`
/// - `duration_secs = -1` means "forever" (final phase)
///
/// # What It Does
/// 1. Scans all manifests with adopted mode support
/// 2. Runs detection for offerings not yet adopted
/// 3. Adopts offerings that pass stability threshold
/// 4. Respects exclusion patterns from configuration
/// 5. Persists adopted offerings to registry
///
/// # Example
/// ```rust,ignore
/// // At daemon startup
/// let state_clone = state.clone();
/// let config_clone = adoption_config.clone();
/// tokio::spawn(async move {
///     auto_adoption_task(state_clone, config_clone).await;
/// });
/// // Task runs forever in background
/// ```
pub async fn auto_adoption_task(state: AppState, config: AdoptionConfig) {
    // Keep orchestrator persistent across scans to maintain stability tracking
    let orchestrator = DetectionOrchestrator::new(state.docker.clone());

    // Track elapsed time for schedule phases
    let start_time = Instant::now();
    let mut scan_count: u32 = 0;

    // Log the schedule being used
    let schedule = config.scan_schedule();
    tracing::info!(
        schedule = ?schedule,
        "Auto-adoption task starting with scan schedule"
    );

    loop {
        // Get current interval based on elapsed time
        let elapsed_secs = start_time.elapsed().as_secs();
        let interval_secs = config.current_scan_interval(elapsed_secs);

        // First scan runs immediately (no sleep), subsequent scans wait
        if scan_count > 0 {
            tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs)).await;
        }
        scan_count = scan_count.saturating_add(1);

        // Get manifests that support adopted mode
        let adoptable_manifests = state
            .manifest_registry
            .offerings_by_mode(&OfferingMode::Adopted);
        tracing::info!(
            count = adoptable_manifests.len(),
            scan = scan_count,
            elapsed_secs = elapsed_secs,
            interval_secs = interval_secs,
            "Running auto-adoption scan"
        );

        // Track if we need to persist changes
        let mut state_changed = false;

        // Phase 1: Validate health of already-adopted offerings
        {
            let mut offerings = state.offerings.write().await;
            for offering in offerings.iter_mut().filter(|o| o.is_adopted()) {
                // Find the manifest for this adopted offering
                let manifest = state.manifest_registry.get_offering(&offering.offering);
                if manifest.is_none() {
                    continue; // Manifest not found, skip validation
                }
                let manifest = manifest.unwrap();

                // Run detection to check if still available
                match orchestrator.detect(manifest).await {
                    Ok(result) if result.detected => {
                        // Offering is available
                        if offering.health != ServiceHealthStatus::Healthy {
                            tracing::info!(
                                offering = %offering.offering,
                                "Adopted offering came back online, marking healthy"
                            );
                            offering.health = ServiceHealthStatus::Healthy;
                            state_changed = true;

                            state
                                .console
                                .emit(garden_common::console::ConsoleEvent::new(
                                    garden_common::console::EventCategory::Services,
                                    garden_common::console::EventStatus::Healthy,
                                    format!("{} is back online", offering.offering),
                                ));
                        }
                    }
                    Ok(_) | Err(_) => {
                        // Offering not detected - mark unhealthy (but don't remove)
                        if offering.health == ServiceHealthStatus::Healthy {
                            tracing::warn!(
                                offering = %offering.offering,
                                "Adopted offering not responding, marking offline"
                            );
                            offering.health = ServiceHealthStatus::Offline;
                            state_changed = true;

                            state
                                .console
                                .emit(garden_common::console::ConsoleEvent::new(
                                    garden_common::console::EventCategory::Services,
                                    garden_common::console::EventStatus::Disconnected,
                                    format!("{} is offline", offering.offering),
                                ));
                        }
                    }
                }
            }
        }

        // Phase 2: Discover and adopt new offerings
        for manifest in adoptable_manifests {
            // Check exclusion list
            if config.is_excluded(&manifest.name) {
                tracing::debug!(offering = %manifest.name, "Skipping excluded offering");
                continue;
            }

            // Check if already adopted
            {
                let offerings = state.offerings.read().await;
                if offerings
                    .iter()
                    .any(|o| o.offering == manifest.name && o.is_adopted())
                {
                    continue; // Already adopted (handled in Phase 1)
                }
            }

            // Try detection
            match orchestrator.detect(manifest).await {
                Ok(result) if result.detected && result.stable => {
                    tracing::info!(
                        offering = %manifest.name,
                        version = ?result.version,
                        "Auto-adopting detected offering"
                    );

                    // Parse default control level
                    let control_level = match config.default_control_level() {
                        "full" => garden_common::AdoptedControlLevel::Full,
                        "announce" => garden_common::AdoptedControlLevel::Announce,
                        _ => garden_common::AdoptedControlLevel::Monitor, // Default safe
                    };

                    // Create unified offering for adopted service
                    let protocol = connection::infer_protocol_from_manifest_metadata(
                        &manifest.name,
                        &manifest.category,
                        manifest.connection_template.as_deref(),
                    );
                    let location = garden_common::OfferingLocation {
                        host: "localhost".to_string(),
                        port: manifest.default_host_port(),
                        protocol,
                        agnostic_port: None,
                    };

                    // Get control config from adopted mode
                    let control = manifest.get_control_config();

                    let adopted_offering = garden_common::Offering {
                        offering_id: garden_common::utils::ids::generate_guidv7(),
                        name: format!(
                            "{}{}{}",
                            manifest.name,
                            garden_common::constants::OFFERING_FQN_SEPARATOR,
                            garden_common::constants::OFFERING_ADOPTED_INSTANCE
                        ),
                        offering: manifest.name.clone(),
                        version: result.version.unwrap_or_else(|| "unknown".to_string()),
                        status: garden_common::OfferingStatus::Running,
                        health: ServiceHealthStatus::Healthy,
                        sub_capabilities: Vec::new(), // Populated by capabilities discovery task
                        location,
                        mode_data: garden_common::OfferingModeData::Adopted(
                            garden_common::AdoptedData {
                                control_level,
                                start_command: control
                                    .as_ref()
                                    .and_then(|c| c.start_command.clone()),
                                stop_command: control.as_ref().and_then(|c| c.stop_command.clone()),
                                restart_command: control
                                    .as_ref()
                                    .and_then(|c| c.restart_command.clone()),
                                health_check_url: control
                                    .as_ref()
                                    .and_then(|c| c.health_check_url.clone()),
                                container_name: None,
                                detected_at: chrono::Utc::now(),
                            },
                        ),
                        registered_at: chrono::Utc::now(),
                        updated_at: None,
                    };

                    // Add to unified offerings registry
                    state.upsert_offering(adopted_offering, true).await;
                    state_changed = true;

                    // Emit console event
                    state
                        .console
                        .emit(garden_common::console::ConsoleEvent::new(
                            garden_common::console::EventCategory::Services,
                            garden_common::console::EventStatus::Healthy,
                            format!("Auto-adopted {}", manifest.name),
                        ));
                }
                Ok(result) if result.detected && !result.stable => {
                    tracing::debug!(
                        offering = %manifest.name,
                        "Detected but not yet stable (waiting for stability threshold)"
                    );
                }
                Ok(result) => {
                    tracing::debug!(
                        offering = %manifest.name,
                        detected = result.detected,
                        methods_tried = result.methods_tried,
                        details = %result.details,
                        "Detection completed (not detected)"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        offering = %manifest.name,
                        error = ?e,
                        "Detection failed for offering"
                    );
                }
            }
        }

        // Persist changes if anything changed
        if state_changed {
            if let Err(e) = state.persist_offerings().await {
                tracing::error!(error = ?e, "Failed to persist offerings");
            }
        }

        tracing::debug!("Auto-adoption scan complete");
    }
}
