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
use crate::domain::{
    evaluate_compatibility, get_current_compat_capabilities, CompatibilityDecision,
    ConnectivityOrchestrator, ConnectivityStatus, DetectionOrchestrator,
};
use crate::infra::config::AdoptionConfig;
use crate::AppState;
use garden_common::detection::{DetectionPipeline, HealthCheck, PortConfig, ProcessSignature};
use garden_common::{OfferingMode, ServiceHealthStatus};
use std::time::Instant;
use tokio_util::sync::CancellationToken;

/// Result of a detection attempt — unifies the process-based pipeline
/// and the legacy command-based orchestrator into a single return type.
enum DetectOutcome {
    /// Service detected. `port` is populated only by the process pipeline.
    Detected { port: Option<u16> },
    /// Service not detected.
    NotDetected,
}

/// Detect a service using either process-based pipeline (new) or
/// command-based orchestrator (legacy). Process-based takes precedence
/// when the manifest defines a `process` section.
async fn detect_offering(
    manifest: &garden_common::manifests::Offering,
    legacy_orchestrator: &DetectionOrchestrator,
    pipeline: &DetectionPipeline,
    remembered_port: Option<u16>,
) -> DetectOutcome {
    // New path: process-based detection (DETECT-0001)
    if let Some(proc_cfg) = manifest.adopted.as_ref().and_then(|a| a.process.as_ref()) {
        let signature = ProcessSignature {
            executable: proc_cfg.executable.clone(),
            windows_executable: proc_cfg.windows_executable.clone(),
            linux_executable: proc_cfg.linux_executable.clone(),
            cmdline_contains: proc_cfg.cmdline_contains.clone(),
        };

        let health = manifest
            .adopted
            .as_ref()
            .and_then(|a| a.health.as_ref())
            .map(|h| HealthCheck {
                path: h.path.clone(),
                expected_status: h.expected_status,
                response_contains: h.response_contains.clone(),
            });

        let ports = manifest
            .adopted
            .as_ref()
            .and_then(|a| a.ports.as_ref())
            .map(|p| PortConfig {
                default: p.default,
                range: p.range,
                remember: p.remember,
            })
            .unwrap_or(PortConfig {
                default: manifest.default_host_port(),
                range: None,
                remember: true,
            });

        let result = pipeline
            .detect(&signature, health.as_ref(), &ports, remembered_port)
            .await;

        tracing::debug!(
            offering = %manifest.name,
            detected = result.detected,
            port = ?result.port,
            pid = ?result.pid,
            details = %result.details,
            "process-based detection"
        );

        return if result.detected {
            DetectOutcome::Detected { port: result.port }
        } else {
            DetectOutcome::NotDetected
        };
    }

    // Legacy path: command-based detection
    match legacy_orchestrator.detect(manifest).await {
        Ok(result) if result.detected && result.stable => {
            DetectOutcome::Detected { port: None }
        }
        Ok(result) if result.detected => {
            // Detected but not yet stable — treat as not detected for callers
            // that need stable results. Log for observability.
            tracing::trace!(
                offering = %manifest.name,
                "legacy detection: detected but not stable"
            );
            DetectOutcome::NotDetected
        }
        Ok(_) => DetectOutcome::NotDetected,
        Err(e) => {
            tracing::warn!(
                offering = %manifest.name,
                error = ?e,
                "legacy detection failed"
            );
            DetectOutcome::NotDetected
        }
    }
}

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
pub async fn auto_adoption_task(state: AppState, config: AdoptionConfig, token: CancellationToken) {
    // Keep orchestrator persistent across scans to maintain stability tracking
    let detector = std::sync::Arc::new(
        crate::infra::detection::ContainerDetector::new(state.platform.docker.clone()),
    );
    let orchestrator = DetectionOrchestrator::new(detector.clone());
    let connectivity = ConnectivityOrchestrator::new(detector);

    // Process-based detection pipeline (DETECT-0001)
    let process_pipeline = garden_common::detection::DetectionPipeline::new();

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
            tokio::select! {
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs)) => {}
                _ = token.cancelled() => {
                    tracing::info!("Auto-adoption task shutting down (cancellation requested)");
                    return;
                }
            }
        }
        // Check cancellation before starting a potentially long scan
        if token.is_cancelled() {
            tracing::info!("Auto-adoption task shutting down (cancellation requested)");
            return;
        }
        scan_count = scan_count.saturating_add(1);

        // Refresh process snapshot for this scan cycle (DETECT-0001)
        process_pipeline.refresh().await;

        // Get manifests that support adopted mode
        let adoptable_manifests = state
            .manifest_registry
            .offerings_by_mode(&OfferingMode::Adopted);
        tracing::debug!(
            count = adoptable_manifests.len(),
            scan = scan_count,
            elapsed_secs = elapsed_secs,
            interval_secs = interval_secs,
            "Running auto-adoption scan"
        );

        // Track if we need to persist changes
        let mut state_changed = false;

        // Phase 1A: Validate active adopted offerings
        // If detection fails, demote back to candidates.
        let active_adopted: Vec<(String, String, garden_common::OfferingLocation)> = {
            let offerings = state.offerings.read().await;
            offerings
                .iter()
                .filter(|o| o.is_adopted())
                .map(|o| (o.offering_id.clone(), o.offering.clone(), o.location.clone()))
                .collect()
        };

        for (offering_id, offering_name, location) in active_adopted {
            let manifest = match state.manifest_registry.get_offering(&offering_name) {
                Some(m) => m,
                None => continue,
            };

            let outcome = detect_offering(manifest, &orchestrator, &process_pipeline, Some(location.port)).await;

            match outcome {
                DetectOutcome::Detected { port } => {
                    // Update port if the pipeline discovered a different one
                    if let Some(p) = port {
                        if p != location.port {
                            tracing::info!(
                                offering = %offering_name,
                                old_port = location.port,
                                new_port = p,
                                "adopted offering port changed"
                            );
                            state
                                .update_offering(&offering_id, true, |o| {
                                    o.location.port = p;
                                    true
                                })
                                .await;
                            state_changed = true;
                        }
                    }

                    // Connectivity enforcement
                    let connectivity_outcome = connectivity
                        .ensure_connectivity(manifest, Some(&location), &state.current.stone.name)
                        .await
                        .unwrap_or_else(|e| {
                            tracing::warn!(offering = %offering_name, error = %e, "Connectivity enforcement failed");
                            crate::domain::ConnectivityOutcome {
                                status: ConnectivityStatus::Failed,
                                details: format!("Connectivity enforcement error: {e}"),
                            }
                        });

                    if !connectivity_outcome.is_ok() {
                        tracing::warn!(
                            offering = %offering_name,
                            details = %connectivity_outcome.details,
                            "Connectivity checks failed for adopted offering"
                        );
                        state
                            .update_offering(&offering_id, false, |o| {
                                o.health = ServiceHealthStatus::Degraded;
                                true
                            })
                            .await;
                        state_changed = true;
                    }
                }
                DetectOutcome::NotDetected => {
                    // Detection failed — demote back to candidates
                    tracing::warn!(
                        offering = %offering_name,
                        "Adopted offering not detected, demoting to candidates"
                    );
                    state.demote_adopted(&offering_id).await;
                    state_changed = true;

                    state.console.emit(garden_common::console::ConsoleEvent::new(
                        garden_common::console::EventCategory::Services,
                        garden_common::console::EventStatus::Disconnected,
                        format!("{} no longer detected", offering_name),
                    ));
                }
            }
        }

        // Phase 1B: Check adopted candidates — promote if detection succeeds
        let candidate_snapshot: Vec<(String, String, garden_common::OfferingLocation)> = {
            let candidates = state.adopted_candidates.read().await;
            candidates
                .iter()
                .map(|o| (o.offering_id.clone(), o.offering.clone(), o.location.clone()))
                .collect()
        };

        for (offering_id, offering_name, location) in candidate_snapshot {
            let manifest = match state.manifest_registry.get_offering(&offering_name) {
                Some(m) => m,
                None => continue,
            };

            let outcome = detect_offering(manifest, &orchestrator, &process_pipeline, Some(location.port)).await;

            match outcome {
                DetectOutcome::Detected { port } => {
                    // Update port if the pipeline discovered a different one
                    if let Some(p) = port {
                        if p != location.port {
                            tracing::info!(
                                offering = %offering_name,
                                old_port = location.port,
                                new_port = p,
                                "candidate offering port changed"
                            );
                            state
                                .update_offering(&offering_id, true, |o| {
                                    o.location.port = p;
                                    true
                                })
                                .await;
                            // state_changed set below by promote
                        }
                    }

                    // Detected — promote to active pool
                    let connectivity_outcome = connectivity
                        .ensure_connectivity(manifest, Some(&location), &state.current.stone.name)
                        .await
                        .unwrap_or_else(|e| {
                            tracing::warn!(offering = %offering_name, error = %e, "Connectivity enforcement failed");
                            crate::domain::ConnectivityOutcome {
                                status: ConnectivityStatus::Failed,
                                details: format!("Connectivity enforcement error: {e}"),
                            }
                        });

                    state.promote_adopted(&offering_id).await;
                    state_changed = true;

                    state.console.emit(garden_common::console::ConsoleEvent::new(
                        garden_common::console::EventCategory::Services,
                        garden_common::console::EventStatus::Healthy,
                        format!("{} detected, now active", offering_name),
                    ));

                    if !connectivity_outcome.is_ok() {
                        tracing::warn!(
                            offering = %offering_name,
                            details = %connectivity_outcome.details,
                            "Connectivity checks failed for newly promoted offering"
                        );
                        state
                            .update_offering(&offering_id, false, |o| {
                                o.health = ServiceHealthStatus::Degraded;
                                true
                            })
                            .await;
                    }
                }
                DetectOutcome::NotDetected => {
                    // Not detected — stay in candidates silently
                    tracing::trace!(offering = %offering_name, "Adopted candidate not detected, staying in candidates");
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

            // Check if already registered (any mode: managed, adopted, or borrowed)
            {
                let offerings = state.offerings.read().await;
                if offerings.iter().any(|o| o.offering == manifest.name) {
                    continue; // Already in registry (any mode)
                }
            }

            // Try detection
            let outcome = detect_offering(manifest, &orchestrator, &process_pipeline, None).await;

            match outcome {
                DetectOutcome::Detected { port } => {
                    // ── Compatibility gate ────────────────────────────
                    // Check hardware compatibility rules before adopting.
                    // e.g. ollama-cpu must NOT be adopted on GPU-equipped stones.
                    if let Some(rules) = &manifest.compatibility {
                        let cached_caps = state.current.capabilities.read().await;
                        let caps = get_current_compat_capabilities(cached_caps.as_ref());
                        if let CompatibilityDecision::Fail { reason, .. } =
                            evaluate_compatibility(rules, &caps)
                        {
                            tracing::info!(
                                offering = %manifest.name,
                                reason = %reason,
                                "Skipping auto-adoption: compatibility check failed"
                            );
                            continue;
                        }
                    }

                    tracing::info!(
                        offering = %manifest.name,
                        port = ?port,
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
                        manifest.connection.as_ref(),
                    );
                    let detected_port = port.unwrap_or_else(|| manifest.default_host_port());
                    let location = garden_common::OfferingLocation {
                        host: "localhost".to_string(),
                        port: detected_port,
                        protocol,
                        agnostic_port: None,
                        // Propagate detected port so topology consumers know the actual port
                        port_map: {
                            let mut pm = std::collections::HashMap::new();
                            pm.insert("default".to_string(), detected_port);
                            pm
                        },
                    };

                    // Get control config from adopted mode
                    let control = manifest.get_control_config();

                    let connectivity_outcome = connectivity
                        .ensure_connectivity(manifest, Some(&location), &state.current.stone.name)
                        .await
                        .unwrap_or_else(|e| {
                            tracing::warn!(
                                offering = %manifest.name,
                                error = %e,
                                "Connectivity enforcement failed"
                            );
                            crate::domain::ConnectivityOutcome {
                                status: ConnectivityStatus::Failed,
                                details: format!("Connectivity enforcement error: {}", e),
                            }
                        });
                    let health = if connectivity_outcome.is_ok() {
                        ServiceHealthStatus::Healthy
                    } else {
                        ServiceHealthStatus::Degraded
                    };

                    let adopted_fqn =
                        garden_common::offerings::OfferingFqn::adopted(&manifest.name)
                            .unwrap_or_else(|e| {
                                // Should never happen for valid manifest names
                                panic!(
                                    "Invalid manifest name '{}' for adopted FQN: {}",
                                    manifest.name, e
                                );
                            });

                    let guidance = crate::tasks::build_adopted_guidance(
                        &state,
                        &adopted_fqn.to_string(),
                        &manifest.name,
                        location.port,
                        None,
                    );

                    let adopted_offering = garden_common::Offering {
                        offering_id: garden_common::utils::ids::generate_guidv7(),
                        name: adopted_fqn,
                        offering: manifest.name.clone(),
                        category: manifest.category.clone(),
                        version: "unknown".to_string(),
                        status: garden_common::OfferingStatus::Running,
                        health,
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
                                guidance,
                                container_name: None,
                                detected_at: chrono::Utc::now(),
                            },
                        ),
                        registered_at: chrono::Utc::now(),
                        updated_at: None,
                        orchestration: None,
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
                DetectOutcome::NotDetected => {
                    tracing::debug!(
                        offering = %manifest.name,
                        "Detection completed (not detected)"
                    );
                }
            }
        }

        // Sync + chirp if we made changes (gateway methods auto-persist)
        if state_changed {
            state.sync_self_services(true).await;
        }

        tracing::debug!("Auto-adoption scan complete");
    }
}
