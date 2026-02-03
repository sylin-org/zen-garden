//! Auto-adoption background task
//!
//! Continuous adoption loop that:
//! - Scans immediately on startup, then at 30-second intervals for fast initial detection
//! - Switches to 5-minute intervals after stability is established
//! - Detects services configured for adopted mode
//! - Adopts stable detected services automatically
//! - Respects stability thresholds and exclusion rules
//!
//! This is a non-blocking background task that runs for the lifetime of the daemon.

use crate::AppState;
use crate::infra::config::AdoptionConfig;
use crate::domain::DetectionOrchestrator;
use garden_common::{AdoptedOfferingInfo, ServiceLocation, ServiceHealthStatus, OfferingMode};

/// Background auto-adoption loop
///
/// This task should be spawned with tokio::spawn() at daemon startup.
/// Uses fast initial detection (30s intervals) then switches to normal (5min).
///
/// # Non-Blocking
/// This function never returns - it's designed to run in the background
/// for the entire daemon lifetime. Spawn it and forget it.
///
/// # Detection Strategy
/// - First 6 scans: 30-second intervals (allows 2+ stability checks in ~2 minutes)
/// - After that: 5-minute intervals for steady-state monitoring
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
    // Fast initial detection: 30 seconds for first 6 scans, then 5 minutes
    const FAST_INTERVAL_SECS: u64 = 30;
    const NORMAL_INTERVAL_SECS: u64 = 300;
    const FAST_SCAN_COUNT: u32 = 6;

    // Keep orchestrator persistent across scans to maintain stability tracking
    let orchestrator = DetectionOrchestrator::new(state.docker.clone());

    let mut scan_count: u32 = 0;

    loop {
        // Use fast interval for initial scans, then switch to normal
        let interval_secs = if scan_count < FAST_SCAN_COUNT {
            FAST_INTERVAL_SECS
        } else {
            NORMAL_INTERVAL_SECS
        };

        // First scan runs immediately (no sleep), subsequent scans wait
        if scan_count > 0 {
            tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs)).await;
        }
        scan_count = scan_count.saturating_add(1);

        // Get manifests that support adopted mode
        let adoptable_manifests = state.manifest_registry.offerings_by_mode(&OfferingMode::Adopted);
        let mode = if scan_count <= FAST_SCAN_COUNT { "fast" } else { "normal" };
        tracing::info!(
            count = adoptable_manifests.len(),
            scan = scan_count,
            mode = mode,
            "Running auto-adoption scan"
        );

        for manifest in adoptable_manifests {
            // Check exclusion list
            if config.is_excluded(&manifest.name) {
                tracing::debug!(offering = %manifest.name, "Skipping excluded offering");
                continue;
            }

            // Check if already adopted
            {
                let adopted = state.adopted_offerings.read().await;
                if adopted.iter().any(|a| a.offering == manifest.name) {
                    continue; // Already adopted
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

                    // Create adopted offering info
                    // TODO: Extract actual location from detection result
                    let location = ServiceLocation {
                        host: "localhost".to_string(),
                        port: manifest.default_host_port(),
                        protocol: manifest.category.clone(),
                    };

                    // Get control config from adopted mode
                    let control = manifest.get_control_config();

                    let adopted_info = AdoptedOfferingInfo {
                        name: format!("{}@adopted", manifest.name),
                        offering: manifest.name.clone(),
                        mode: OfferingMode::Adopted,
                        location,
                        control_level,
                        health: ServiceHealthStatus::Healthy,
                        detected_at: chrono::Utc::now().to_rfc3339(),
                        version: result.version,
                        start_command: control.and_then(|c| c.start_command.clone()),
                        stop_command: control.and_then(|c| c.stop_command.clone()),
                        restart_command: control.and_then(|c| c.restart_command.clone()),
                        health_check_url: control.and_then(|c| c.health_check_url.clone()),
                        container_name: None,
                    };

                    // Add to adopted registry
                    {
                        let mut adopted = state.adopted_offerings.write().await;
                        adopted.push(adopted_info.clone());
                    }

                    // Emit console event
                    state.console.emit(garden_common::console::ConsoleEvent::new(
                        garden_common::console::EventCategory::Services,
                        garden_common::console::EventStatus::Healthy,
                        format!("Auto-adopted {}", manifest.name),
                    ));

                    // TODO: Persist adopted offerings registry to disk
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

        tracing::debug!("Auto-adoption scan complete");
    }
}
