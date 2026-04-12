//! Nourishment API - Software and firmware update management
//!
//! Provides unified update checking and execution for:
//! - Software offerings (Docker images)
//! - Hardware firmware (via fwupd/LVFS)
//!
//! ## Garden endpoints (Rake → tended Moss, orchestrated):
//! - GET  /api/v1/garden/nourishment - Aggregates updates from all stones
//! - POST /api/v1/garden/nourishment/execute - Dispatches to affected stones
//!
//! ## Stone endpoints (local or Moss → Moss):
//! - GET  /api/v1/stone/nourishment - This stone's pending updates
//! - POST /api/v1/stone/nourishment/execute - Execute on this stone
//! - GET  /api/v1/stone/nourishment/stream/:job_id - SSE status stream

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
};
use std::convert::Infallible;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::AppState;
use crate::api::responses::ApiResponse;
use garden_common::HardwareCapabilities;
use garden_common::console::{self, EventCategory, EventStatus};
use garden_common::nourishment::*;

// ============================================================================
// Endpoint: GET /api/v1/stone/nourishment
// ============================================================================

/// Stone-local update check
///
/// Queries Docker registry for offering updates and detects firmware updates.
/// Validates hardware constraints and returns available/blocked updates.
pub async fn check_stone(
    State(state): State<AppState>,
) -> crate::api::ApiResult<NourishmentCheckResponse> {
    // Get hardware capabilities for constraint checking
    let caps_guard = state.current.capabilities.read().await;
    let capabilities = caps_guard.as_ref().ok_or_else(|| {
        crate::unavailable(
            "HARDWARE_NOT_DETECTED",
            "Hardware capabilities not yet detected",
        )
    })?;

    // Check offering updates
    let offering_updates = check_offering_updates(&state, capabilities)
        .await
        .map_err(|e| {
            crate::internal(
                "REGISTRY_ERROR",
                format!("Failed to check offering updates: {}", e),
            )
        })?;

    // Check firmware updates (with manifest support)
    let firmware_updates = check_firmware_updates(&state, capabilities)
        .await
        .map_err(|e| {
            crate::internal(
                "FIRMWARE_ERROR",
                format!("Failed to check firmware updates: {}", e),
            )
        })?;

    // Combine updates
    let mut available = Vec::new();
    let mut blocked = Vec::new();

    for update in offering_updates {
        match update {
            Ok(u) => available.push(u),
            Err(b) => blocked.push(b),
        }
    }

    for update in firmware_updates {
        match update {
            Ok(u) => available.push(u),
            Err(b) => blocked.push(b),
        }
    }

    let response = NourishmentCheckResponse {
        stone_name: state.current.stone.name.clone(),
        updates: Updates { available, blocked },
    };

    crate::api::ok(response)
}

// ============================================================================
// Endpoint: GET /api/v1/garden/nourishment
// ============================================================================

/// Garden-wide update check (orchestrated)
///
/// Queries all stones in parallel for updates, following the observe pattern.
pub async fn check_garden(
    State(state): State<AppState>,
) -> crate::api::ApiResult<GardenNourishmentResponse> {
    // Get topology from this stone's cache
    let entries = state.topology.all_stones().await;

    // Query all stones in parallel
    let tasks: Vec<_> = entries
        .iter()
        .map(|entry| {
            let endpoint = entry.address.http_base();
            let stone_name = entry.stone_name.clone();
            let client = crate::http::HTTP.clone();

            tokio::spawn(
                async move { query_stone_nourishment(&client, &endpoint, &stone_name).await },
            )
        })
        .collect();

    // Collect results
    let mut stones = Vec::new();
    for task in tasks {
        if let Ok(Some(response)) = task.await {
            stones.push(response);
        }
    }

    let response = GardenNourishmentResponse { stones };

    crate::api::ok(response)
}

/// Query single stone for updates
async fn query_stone_nourishment(
    client: &reqwest::Client,
    endpoint: &str,
    stone_name: &str,
) -> Option<NourishmentCheckResponse> {
    let url = format!("{}/api/v1/stone/updates", endpoint.trim_end_matches('/'));

    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<ApiResponse<NourishmentCheckResponse>>().await {
                Ok(api_response) => Some(api_response.data),
                Err(e) => {
                    tracing::warn!(stone = %stone_name, error = ?e, "Failed to parse nourishment response");
                    None
                }
            }
        }
        Ok(resp) => {
            tracing::warn!(stone = %stone_name, status = ?resp.status(), "Stone returned error");
            None
        }
        Err(e) => {
            tracing::warn!(stone = %stone_name, error = ?e, "Failed to reach stone");
            None
        }
    }
}

// ============================================================================
// Endpoint: POST /api/v1/garden/nourishment/execute
// ============================================================================

/// Garden-wide update execution (orchestrated)
///
/// Receives scope from Rake, queries all stones for pending updates,
/// then dispatches execute requests to each affected stone.
pub async fn execute_garden(
    State(state): State<AppState>,
    Json(request): Json<ExecuteRequest>,
) -> crate::api::ApiResult<GardenExecuteResponse> {
    use garden_common::nourishment::UpdateScope;
    use garden_common::utils::ids::generate_guidv7;

    // Step 1: Query all stones for their pending updates
    let entries = state.topology.all_stones().await;

    // Query each stone for updates
    let check_tasks: Vec<_> = entries
        .iter()
        .map(|entry| {
            let client = crate::http::HTTP.clone();
            let endpoint = entry.address.http_base();
            let stone_name = entry.stone_name.clone();

            tokio::spawn(async move {
                let resp = query_stone_nourishment(&client, &endpoint, &stone_name).await;
                (stone_name, endpoint, resp)
            })
        })
        .collect();

    // Collect results and filter to affected stones
    let mut affected_stones = Vec::new();
    for task in check_tasks {
        if let Ok((stone_name, endpoint, Some(check_response))) = task.await {
            // Check if this stone has updates matching the scope
            let has_matching_updates =
                check_response
                    .updates
                    .available
                    .iter()
                    .any(|update: &Update| {
                        matches!(
                            (&request.scope, update),
                            (UpdateScope::All, _)
                                | (UpdateScope::Offerings, Update::Offering { .. })
                                | (UpdateScope::Firmware, Update::Firmware { .. })
                                | (UpdateScope::Moss, Update::Moss { .. })
                        )
                    });

            if has_matching_updates {
                affected_stones.push((stone_name, endpoint));
            }
        }
    }

    if affected_stones.is_empty() {
        return crate::api::ok_with(
            GardenExecuteResponse {
                job_id: generate_guidv7(),
                stone_jobs: Vec::new(),
            },
            vec!["No stones have matching updates".to_string()],
        );
    }

    // Step 2: Dispatch execute request to each affected stone
    let garden_job_id = generate_guidv7();

    let dispatch_tasks: Vec<_> = affected_stones
        .iter()
        .map(|(stone_name, endpoint)| {
            let client = crate::http::HTTP.clone();
            let stone_name = stone_name.clone();
            let endpoint = endpoint.clone();
            let request = request.clone();

            tokio::spawn(async move {
                dispatch_execute_to_stone(&client, &stone_name, &endpoint, &request).await
            })
        })
        .collect();

    // Collect results
    let mut stone_jobs = Vec::new();
    for task in dispatch_tasks {
        match task.await {
            Ok(status) => stone_jobs.push(status),
            Err(e) => {
                tracing::error!(error = ?e, "Task join error during garden execute");
            }
        }
    }

    let response = GardenExecuteResponse {
        job_id: garden_job_id,
        stone_jobs,
    };

    crate::api::ok(response)
}

/// Dispatch execute request to a single stone
async fn dispatch_execute_to_stone(
    client: &reqwest::Client,
    stone_name: &str,
    endpoint: &str,
    request: &ExecuteRequest,
) -> StoneJobStatus {
    let url = format!(
        "{}/api/v1/stone/updates/execute",
        endpoint.trim_end_matches('/')
    );

    match client.post(&url).json(request).send().await {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<ApiResponse<ExecuteResponse>>().await {
                Ok(api_response) => {
                    tracing::info!(stone = %stone_name, job_id = %api_response.data.job_id, "Update dispatched");
                    StoneJobStatus {
                        stone_name: stone_name.to_string(),
                        job_id: Some(api_response.data.job_id),
                        state: StoneJobState::Running,
                        message: None,
                        endpoint: Some(endpoint.to_string()),
                    }
                }
                Err(e) => {
                    tracing::warn!(stone = %stone_name, error = ?e, "Failed to parse execute response");
                    StoneJobStatus {
                        stone_name: stone_name.to_string(),
                        job_id: None,
                        state: StoneJobState::Failed,
                        message: Some(format!("Parse error: {}", e)),
                        endpoint: None,
                    }
                }
            }
        }
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::warn!(stone = %stone_name, status = ?status, body = %body, "Stone returned error");
            StoneJobStatus {
                stone_name: stone_name.to_string(),
                job_id: None,
                state: StoneJobState::Failed,
                message: Some(format!("HTTP {}: {}", status, body)),
                endpoint: None,
            }
        }
        Err(e) => {
            tracing::warn!(stone = %stone_name, error = ?e, "Failed to reach stone");
            StoneJobStatus {
                stone_name: stone_name.to_string(),
                job_id: None,
                state: StoneJobState::Unreachable,
                message: Some(format!("Connection error: {}", e)),
                endpoint: None,
            }
        }
    }
}

// ============================================================================
// Endpoint: POST /api/v1/stone/nourishment/execute
// ============================================================================

/// Execute updates on THIS stone based on scope
///
/// Interprets the scope, fetches this stone's pending updates, and executes matching ones.
/// - scope: "all" → apply all pending updates
/// - scope: "offerings" → apply only offering updates
/// - scope: "firmware" → apply only firmware updates
/// - items: ["offering:x", "firmware:y"] → apply specific items (future V1+)
pub async fn execute_stone(
    State(state): State<AppState>,
    Json(request): Json<ExecuteRequest>,
) -> crate::api::ApiResult<ExecuteResponse> {
    use garden_common::nourishment::UpdateScope;
    use garden_common::utils::ids::generate_guidv7;

    // Get this stone's pending updates
    let (offering_updates, firmware_updates) = {
        let caps_guard = state.current.capabilities.read().await;
        let capabilities = caps_guard.as_ref().ok_or_else(|| {
            crate::unavailable(
                "HARDWARE_NOT_DETECTED",
                "Hardware capabilities not yet detected",
            )
        })?;

        // Check what updates are available
        let offerings = check_offering_updates(&state, capabilities)
            .await
            .map_err(|e| {
                crate::internal(
                    "REGISTRY_ERROR",
                    format!("Failed to check offering updates: {}", e),
                )
            })?;

        let firmware = check_firmware_updates(&state, capabilities)
            .await
            .map_err(|e| {
                crate::internal(
                    "FIRMWARE_ERROR",
                    format!("Failed to check firmware updates: {}", e),
                )
            })?;

        (offerings, firmware)
    };

    // Filter based on scope
    let (apply_offerings, apply_firmware) = match request.scope {
        UpdateScope::All => (true, true),
        UpdateScope::Offerings => (true, false),
        UpdateScope::Firmware => (false, true),
        UpdateScope::Moss => (false, false), // Moss self-update handled separately
    };

    // Collect updates to execute
    let mut pending_offerings = Vec::new();
    let mut pending_firmware = Vec::new();

    if apply_offerings {
        for update in &offering_updates {
            if let Ok(Update::Offering { name, .. }) = update {
                pending_offerings.push(name.clone());
            }
        }
    }

    if apply_firmware {
        for update in &firmware_updates {
            if let Ok(Update::Firmware { device_id, .. }) = update {
                pending_firmware.push(device_id.clone());
            }
        }
    }

    // Granular selection: if items are specified, filter to only those
    if !request.items.is_empty() {
        let selected_offerings: std::collections::HashSet<&str> = request
            .items
            .iter()
            .filter_map(|item| item.strip_prefix("offering:"))
            .collect();
        let selected_firmware: std::collections::HashSet<&str> = request
            .items
            .iter()
            .filter_map(|item| item.strip_prefix("firmware:"))
            .collect();

        if !selected_offerings.is_empty() {
            pending_offerings.retain(|name| selected_offerings.contains(name.as_str()));
        }
        if !selected_firmware.is_empty() {
            pending_firmware.retain(|id| selected_firmware.contains(id.as_str()));
        }
    }

    if pending_offerings.is_empty() && pending_firmware.is_empty() {
        return Err(crate::infra::error_response(
            StatusCode::OK,
            "NO_UPDATES",
            "No matching updates pending for this stone".to_string(),
            None,
        ));
    }

    // Generate job ID
    let job_id = generate_guidv7();

    // Log job creation with scope
    tracing::info!(
        job_id = %job_id,
        scope = ?request.scope,
        pending_offerings = pending_offerings.len(),
        pending_firmware = pending_firmware.len(),
        "Nourishment job created"
    );

    // Mark stone as nourishing and chirp immediately
    crate::domain::topology::composition::update_stone_health(
        &state,
        garden_common::constants::STONE_NOURISHING.to_string(),
        true,
    )
    .await;

    // Create broadcast channel for status updates
    let (tx, _rx) = broadcast::channel::<String>(100);

    // Store channel in state
    {
        let mut jobs = state.orchestration.nourishment.jobs.write().await;
        jobs.insert(job_id.clone(), tx.clone());
    }

    // Spawn background task
    let state = state.clone();
    let task_job_id = job_id.clone();
    let console = state.console.clone();

    tokio::spawn(async move {
        execute_updates_background(
            state,
            pending_offerings,
            pending_firmware,
            task_job_id,
            tx,
            &console,
        )
        .await;
    });

    let response = ExecuteResponse { job_id };

    crate::api::ok(response)
}

/// Background task for executing updates.
///
/// Emits both SSE broadcast messages (for Rake streaming) and structured
/// ConsoleEvents (for tty1 visibility). The console events are classified
/// as critical tty1 events so operators see real-time progress on the
/// physical console during updates.
async fn execute_updates_background(
    state: AppState,
    offerings: Vec<String>,
    firmware: Vec<String>,
    job_id: String,
    tx: broadcast::Sender<String>,
    console: &std::sync::Arc<console::ConsolePrinter>,
) {
    tracing::info!(job_id = %job_id, "Nourishment job starting execution");
    let _ = tx.send(format!("Starting nourishment job {}", job_id));

    let total = offerings.len() + firmware.len();
    console.emit(console::ConsoleEvent::new(
        EventCategory::Jobs,
        EventStatus::Started,
        format!("Nourishment: {} update(s)", total),
    ));

    // Phase 1: Software updates (offerings)
    if !offerings.is_empty() {
        let _ = tx.send(format!(
            "📦 Phase 1: Updating {} offering(s)",
            offerings.len()
        ));

        for (idx, name) in offerings.iter().enumerate() {
            let progress = format!("[{}/{}]", idx + 1, offerings.len());
            let _ = tx.send(format!("  {} Updating {}", progress, name));
            console.emit(console::ConsoleEvent::new(
                EventCategory::Services,
                EventStatus::Upgrading,
                format!("{} {} pulling image", progress, name),
            ));

            match execute_offering_update(&state, name, &tx, console).await {
                Ok(()) => {
                    tracing::info!(job_id = %job_id, offering = %name, "Offering updated successfully");
                    let _ = tx.send(format!("    ✓ {} updated successfully", name));
                    console.emit(console::ConsoleEvent::new(
                        EventCategory::Services,
                        EventStatus::Upgraded,
                        format!("{} {}", progress, name),
                    ));
                }
                Err(e) => {
                    tracing::warn!(job_id = %job_id, offering = %name, error = %e, "Offering update failed");
                    let _ = tx.send(format!("    ✗ {} failed: {}", name, e));
                    console.emit(console::ConsoleEvent::new(
                        EventCategory::Services,
                        EventStatus::UpgradeError,
                        format!("{} {} — {}", progress, name, e),
                    ));
                }
            }
        }
    }

    // Phase 2: Hardware updates (firmware)
    let mut needs_reboot = false;
    if !firmware.is_empty() {
        let _ = tx.send(format!(
            "🔧 Phase 2: Updating {} firmware device(s)",
            firmware.len()
        ));

        for (idx, device_id) in firmware.iter().enumerate() {
            let progress = format!("[{}/{}]", idx + 1, firmware.len());
            let _ = tx.send(format!("  {} Updating firmware: {}", progress, device_id));
            console.emit(console::ConsoleEvent::new(
                EventCategory::Ops,
                EventStatus::Active,
                format!("Firmware {} {}", progress, device_id),
            ));

            match execute_firmware_update(device_id, &tx).await {
                Ok(requires_reboot) => {
                    if requires_reboot {
                        needs_reboot = true;
                        tracing::info!(job_id = %job_id, device = %device_id, "Firmware updated (reboot required)");
                        let _ = tx.send(format!("    ✓ {} updated (reboot required)", device_id));
                    } else {
                        tracing::info!(job_id = %job_id, device = %device_id, "Firmware updated successfully");
                        let _ = tx.send(format!("    ✓ {} updated successfully", device_id));
                    }
                    console.emit(console::ConsoleEvent::new(
                        EventCategory::Ops,
                        EventStatus::Staged,
                        format!("Firmware {} {}", progress, device_id),
                    ));
                }
                Err(e) => {
                    tracing::warn!(job_id = %job_id, device = %device_id, error = %e, "Firmware update failed");
                    let _ = tx.send(format!("    ✗ {} failed: {}", device_id, e));
                    console.emit(console::ConsoleEvent::new(
                        EventCategory::Ops,
                        EventStatus::RestartError,
                        format!("Firmware {} {} — {}", progress, device_id, e),
                    ));
                }
            }
        }
    }

    // Cleanup job from state before potential reboot
    {
        let mut jobs = state.orchestration.nourishment.jobs.write().await;
        jobs.remove(&job_id);
    }

    // Restore stone health based on service state
    let health_status = {
        let offerings = state.offerings.read().await;
        let has_degraded = offerings
            .iter()
            .any(|o| matches!(o.status, garden_common::OfferingStatus::Degraded));

        if has_degraded {
            garden_common::constants::STONE_DEGRADED.to_string()
        } else {
            garden_common::constants::STONE_THRIVING.to_string()
        }
    };

    // Update health and chirp
    crate::domain::topology::composition::update_stone_health(&state, health_status, true).await;

    // Phase 3: Reboot immediately if firmware updates require it
    if needs_reboot {
        tracing::info!(job_id = %job_id, "Nourishment complete, initiating immediate reboot");
        let _ = tx.send("🔄 Firmware updates require reboot. Rebooting now...".to_string());
        console.emit(console::ConsoleEvent::new(
            EventCategory::Ops,
            EventStatus::RestartTriggered,
            "Rebooting for firmware updates".to_string(),
        ));

        #[cfg(target_os = "linux")]
        {
            tracing::info!(job_id = %job_id, "Executing systemctl reboot");
            let _ = tokio::process::Command::new("systemctl")
                .args(["reboot"])
                .spawn();
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = tx.send("⚠ Reboot not supported on this platform".to_string());
        }
    } else {
        tracing::info!(job_id = %job_id, "Nourishment job complete");
        let _ = tx.send("✅ Nourishment complete".to_string());
        console.emit(console::ConsoleEvent::new(
            EventCategory::Jobs,
            EventStatus::Completed,
            format!("Nourishment: {} update(s) applied", total),
        ));
    }
}

/// Execute offering (Docker container) update
async fn execute_offering_update(
    state: &AppState,
    name: &str,
    tx: &broadcast::Sender<String>,
    console: &console::ConsolePrinter,
) -> anyhow::Result<()> {
    // Mark service as updating in registry via gateway (syncs self_entry + chirps)
    state
        .offerings
        .update_by_name(name, |o| {
            if o.is_managed() {
                o.status = garden_common::OfferingStatus::Maintenance;
                true
            } else {
                false
            }
        })
        .await;

    // Get the target image from the Docker service
    let target_image = state
        .platform
        .docker
        .get_service_image(name)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get image for service '{}': {}", name, e))?;

    // Step 1: Pull the latest image
    let _ = tx.send(format!("    Pulling image: {}", target_image));
    console.emit(console::ConsoleEvent::new(
        EventCategory::Docker,
        EventStatus::ImagePull,
        format!("{} → {}", name, target_image),
    ));
    state
        .platform
        .docker
        .pull_image(&target_image, None)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to pull image '{}': {}", target_image, e))?;

    // Step 2: Stop and remove the existing container
    let _ = tx.send(format!("    Stopping service: {}", name));
    console.emit(console::ConsoleEvent::new(
        EventCategory::Services,
        EventStatus::Stopping,
        name.to_string(),
    ));
    state
        .platform
        .docker
        .stop_service(name, None)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to stop service '{}': {}", name, e))?;

    let _ = tx.send(format!("    Removing old container: {}", name));
    state
        .platform
        .docker
        .remove_service(name, None)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to remove service '{}': {}", name, e))?;

    // Step 3: Recreate the service with the new image, composing any config patches
    let _ = tx.send(format!(
        "    Creating new container with image: {}",
        target_image
    ));
    console.emit(console::ConsoleEvent::new(
        EventCategory::Services,
        EventStatus::Creating,
        format!("{} → {}", name, target_image),
    ));

    // Build spec via CompiledOffering (hardware-resolved) + config patches,
    // then override the image with the target upgrade image.
    let mut spec = crate::domain::services_internal::build_spec_from_manifest(state, name).await?;
    spec.image = target_image.clone();

    state
        .platform
        .docker
        .install_service(name, &spec, None)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to recreate service '{}': {}", name, e))?;

    // Step 4: Verify service health
    tokio::time::sleep(Duration::from_secs(2)).await; // Allow startup
    match state.platform.docker.get_service_status(name).await {
        Ok(garden_common::ServiceStatus::Running) => {
            let _ = tx.send(format!("    ✅ Service {} updated and running", name));
        }
        Ok(status) => {
            tracing::warn!(service = %name, status = ?status, "Service not running after update");
            let _ = tx.send(format!(
                "    ⚠ Service {} updated but status: {:?}",
                name, status
            ));
        }
        Err(e) => {
            tracing::error!(service = %name, error = ?e, "Failed to verify service status");
            let _ = tx.send(format!(
                "    ⚠ Service {} updated but status verification failed",
                name
            ));
        }
    }

    // Mark service as running again via gateway (syncs self_entry + chirps)
    state
        .offerings
        .update_by_name(name, |o| {
            if o.is_managed() {
                o.status = garden_common::OfferingStatus::Running;
                true
            } else {
                false
            }
        })
        .await;

    Ok(())
}

/// Execute firmware update via fwupdmgr
#[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
async fn execute_firmware_update(
    device_id: &str,
    tx: &broadcast::Sender<String>,
) -> anyhow::Result<bool> {
    #[cfg(target_os = "linux")]
    {
        let _ = tx.send(format!("    Running fwupdmgr update"));

        let output = tokio::process::Command::new("fwupdmgr")
            .args(["update", device_id, "--no-reboot-check", "--assume-yes"])
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to execute fwupdmgr: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("fwupdmgr failed: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let requires_reboot = stdout.contains("reboot") || stdout.contains("restart");

        Ok(requires_reboot)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = tx.send("    Firmware updates not supported on this platform".to_string());
        tokio::time::sleep(Duration::from_secs(1)).await;
        Ok(false)
    }
}

// ============================================================================
// Endpoint: GET /api/v1/nourishment/stream/:job_id
// ============================================================================

/// Stream nourishment job status (Server-Sent Events)
pub async fn stream_status(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<
    Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>,
    (StatusCode, Json<garden_common::api_utils::ApiErrorResponse>),
> {
    // Get broadcast receiver for this job
    let rx = {
        let jobs = state.orchestration.nourishment.jobs.read().await;
        jobs.get(&job_id).map(|tx| tx.subscribe()).ok_or_else(|| {
            crate::not_found(
                "JOB_NOT_FOUND",
                format!("Nourishment job not found: {}", job_id),
            )
        })?
    };

    // Convert broadcast receiver to tokio_stream
    let broadcast_stream = BroadcastStream::new(rx);
    let stream = broadcast_stream.filter_map(|result| {
        match result {
            Ok(message) => Some(Ok(Event::default().data(message))),
            Err(_) => None, // Skip lagged messages
        }
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Check offering updates with constraint validation
async fn check_offering_updates(
    state: &AppState,
    capabilities: &HardwareCapabilities,
) -> anyhow::Result<Vec<Result<Update, BlockedUpdate>>> {
    use crate::domain::constraints::check_constraints;
    use crate::infra::registry_client::{
        RegistryConfig, find_newer_version, get_image_digest, query_image_tags,
    };

    let offerings = state.offerings.read().await;
    let config = RegistryConfig::default();

    let mut results = Vec::new();

    // Only check managed offerings (Docker containers) for updates
    for offering in offerings.iter().filter(|o| o.is_managed()) {
        // Get the template image reference (e.g., "redis:latest")
        let offering_name_str = offering.name.to_string();
        let template_image = match state
            .platform
            .docker
            .get_service_image(&offering_name_str)
            .await
        {
            Ok(img) => img,
            Err(_) => continue,
        };

        // Query registry for available versions
        let available_tags = match query_image_tags(&template_image, &config).await {
            Ok(tags) => tags,
            Err(e) => {
                tracing::warn!(offering = %offering.name, error = ?e, "Failed to query registry");
                continue;
            }
        };

        // Extract base image and current tag from template
        let (base_image, current_tag) = template_image
            .rsplit_once(':')
            .unwrap_or((&template_image, "latest"));

        // Special handling for "latest" tag - can't do version comparison
        // Just check if any available version has a different digest
        let newer_tag = if current_tag == "latest" {
            // Get digest for current "latest"
            let current_digest = match get_image_digest(&template_image, &config).await {
                Ok(digest) => digest,
                Err(e) => {
                    tracing::warn!(offering = %offering.name, error = ?e, "Failed to get digest for latest tag");
                    continue;
                }
            };

            // Check all available tags to find one with different digest
            let mut found_newer = None;
            for tag in &available_tags {
                if tag == "latest" {
                    continue;
                }
                let tag_image = format!("{}:{}", base_image, tag);
                if let Ok(tag_digest) = get_image_digest(&tag_image, &config).await
                    && tag_digest != current_digest
                {
                    found_newer = Some(tag.clone());
                    break;
                }
            }
            found_newer
        } else {
            find_newer_version(current_tag, &available_tags)
        };

        // Find newer version
        if let Some(newer_tag) = newer_tag {
            // Build image references for both current and newer tags
            let current_image = template_image.clone();
            let newer_image = format!("{}:{}", base_image, newer_tag);

            // Get registry digest for BOTH tags
            let current_digest = match get_image_digest(&current_image, &config).await {
                Ok(digest) => digest,
                Err(e) => {
                    tracing::warn!(offering = %offering.name, current_tag, error = ?e, "Failed to get digest for current tag");
                    continue;
                }
            };

            let newer_digest = match get_image_digest(&newer_image, &config).await {
                Ok(digest) => digest,
                Err(e) => {
                    tracing::warn!(offering = %offering.name, newer_tag, error = ?e, "Failed to get digest for newer tag");
                    continue;
                }
            };

            // Only show update if the registry digests are different
            // This handles rolling tags (e.g., "1.6" pointing to "1.6.40")
            if current_digest != newer_digest {
                let update = Update::Offering {
                    name: offering.name.to_string(),
                    current: current_tag.to_string(),
                    available: newer_tag.clone(),
                    age_days: None, // Requires registry API v2 created-timestamp (not available from digest alone)
                };

                // Check constraints (example: MongoDB 5.0+ requires AVX)
                let requirements = get_offering_requirements(&offering_name_str, state);

                match check_constraints(&requirements, capabilities) {
                    Ok(()) => results.push(Ok(update)),
                    Err(violation) => {
                        results.push(Err(BlockedUpdate {
                            update,
                            reason: violation.message(),
                        }));
                    }
                }
            }
        }
    }

    Ok(results)
}

/// Check firmware updates with hardware manifest support
///
/// Enriches fwupd detection with manifest constraints:
/// - Matches fwupd devices against manifest's lvfs_device_id (Tested confidence)
/// - Shows all other fwupd updates as Suggested confidence
/// - Blocks updates if requires_ac_power and not plugged in
/// - Adds version context from manifest (minimum, recommended, latest_known)
async fn check_firmware_updates(
    state: &AppState,
    capabilities: &HardwareCapabilities,
) -> anyhow::Result<Vec<Result<Update, BlockedUpdate>>> {
    use crate::infra::firmware::detect_firmware_updates;
    use garden_common::nourishment::FirmwareConfidence;

    let firmware_list = detect_firmware_updates().await?;

    // Find matching hardware manifest for this system
    let hw_entry = state.catalog.find_hw_manifest(
        capabilities.hardware.system_manufacturer.as_deref(),
        capabilities.hardware.system_product.as_deref(),
    );

    if let Some(entry) = hw_entry.as_ref() {
        tracing::info!(
            manifest = %entry.key(),
            "Matched system to hardware manifest"
        );
    }

    // Get manifest firmware config if available
    let manifest_config = hw_entry
        .as_ref()
        .and_then(|e| e.manifest.as_ref().and_then(|m| m.firmware.as_ref()));

    let mut results: Vec<Result<Update, BlockedUpdate>> = Vec::new();

    for fw in firmware_list {
        // Determine confidence level based on manifest match
        let (confidence, is_manifest_device) = if let Some(firmware_cfg) = manifest_config {
            if let Some(ref manifest_device_id) = firmware_cfg.lvfs_device_id {
                if fw.device_id.contains(manifest_device_id) || manifest_device_id == &fw.device_id
                {
                    (FirmwareConfidence::Tested, true)
                } else {
                    (FirmwareConfidence::Suggested, false)
                }
            } else {
                // Manifest exists but no specific device ID - treat as suggested
                (FirmwareConfidence::Suggested, false)
            }
        } else {
            // No manifest at all - all updates are suggested
            (FirmwareConfidence::Suggested, false)
        };

        let update = Update::Firmware {
            device_id: fw.device_id.clone(),
            name: fw.device_name.clone(),
            vendor: fw.vendor.clone(),
            current: fw.current_version.clone(),
            available: fw.available_version.clone(),
            requires_reboot: fw.requires_reboot,
            description: fw.description.clone(),
            confidence,
        };

        // Apply manifest constraints only to tested (manifest-matched) devices
        if is_manifest_device && let Some(firmware_cfg) = manifest_config {
            // Check AC power requirement
            if firmware_cfg.requires_ac_power.unwrap_or(false) && !is_on_ac_power().await {
                results.push(Err(BlockedUpdate {
                    update,
                    reason:
                        "Firmware update requires AC power. Please plug in the power Companion."
                            .to_string(),
                }));
                continue;
            }

            // Check version constraints
            if let Some(ref versions) = firmware_cfg.versions {
                // Warn if trying to go below minimum
                if let Some(ref minimum) = versions.minimum
                    && version_less_than(&fw.available_version, minimum)
                {
                    results.push(Err(BlockedUpdate {
                        update,
                        reason: format!(
                            "Available version {} is below minimum required version {}",
                            fw.available_version, minimum
                        ),
                    }));
                    continue;
                }

                // Log version context
                if let Some(ref recommended) = versions.recommended
                    && version_less_than(&fw.available_version, recommended)
                {
                    tracing::info!(
                        available = %fw.available_version,
                        recommended = %recommended,
                        "Update available but not yet at recommended version"
                    );
                }
            }
        }

        results.push(Ok(update));
    }

    Ok(results)
}

/// Check if system is running on AC power
async fn is_on_ac_power() -> bool {
    #[cfg(target_os = "linux")]
    {
        // Check via sysfs - common paths for AC Companion status
        let ac_paths = [
            "/sys/class/power_supply/AC/online",
            "/sys/class/power_supply/AC0/online",
            "/sys/class/power_supply/ACAD/online",
            "/sys/class/power_supply/ADP1/online",
        ];

        for path in ac_paths {
            if let Ok(content) = tokio::fs::read_to_string(path).await {
                if content.trim() == "1" {
                    return true;
                }
            }
        }

        // No AC Companion found means desktop (always "on AC")
        // Check if there's any battery - if no battery, assume desktop
        let has_battery = match tokio::fs::read_dir("/sys/class/power_supply").await {
            Ok(mut dir) => {
                let mut found_battery = false;
                while let Ok(Some(entry)) = dir.next_entry().await {
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();
                    if name_str.starts_with("BAT") {
                        found_battery = true;
                        break;
                    }
                }
                found_battery
            }
            Err(_) => false,
        };

        // If no battery detected, assume desktop (always on AC)
        if !has_battery {
            return true;
        }

        false
    }

    #[cfg(not(target_os = "linux"))]
    {
        // Windows: Assume AC power for thin clients
        true
    }
}

/// Simple version comparison (less than)
/// Handles versions like "1.2.3", "1.10.0", etc.
fn version_less_than(a: &str, b: &str) -> bool {
    let parse_version = |s: &str| -> Vec<u32> {
        s.split('.')
            .filter_map(|part| part.parse::<u32>().ok())
            .collect()
    };

    let va = parse_version(a);
    let vb = parse_version(b);

    for i in 0..va.len().max(vb.len()) {
        let part_a = va.get(i).copied().unwrap_or(0);
        let part_b = vb.get(i).copied().unwrap_or(0);

        if part_a < part_b {
            return true;
        }
        if part_a > part_b {
            return false;
        }
    }

    false // Equal
}

/// Get hardware requirements for an offering from its manifest.
///
/// Loads compatibility rules from the manifest registry. Falls back to
/// hardcoded rules for offerings without manifest metadata.
fn get_offering_requirements(
    name: &str,
    state: &crate::AppState,
) -> crate::domain::constraints::Requirements {
    use crate::domain::constraints::Requirements;

    // Try to load from manifest first — parse DSL predicates to extract requirements
    if let Some(offering) = state.catalog.get_manifest(name)
        && let Some(ref compat) = offering.compatibility
    {
        let mut req = Requirements::new();
        for rule in &compat.compatibility_rules {
            for when_str in &rule.when {
                if let Ok(pred) = garden_common::compatibility::Predicate::parse(when_str) {
                    use garden_common::compatibility::{Condition, Fact};
                    match (&pred.fact, &pred.condition) {
                        (Fact::CpuFeatures, Condition::Lacks(feats)) => {
                            for f in feats {
                                req = req.require_cpu_feature(f);
                            }
                        }
                        (Fact::RamTotalMb, Condition::Cmp { value, .. }) => {
                            req = req.require_memory_mb(*value as u64);
                        }
                        (Fact::Architecture, Condition::In(archs)) => {
                            for a in archs {
                                req = req.require_architecture(a);
                            }
                        }
                        (Fact::Architecture, Condition::Is(arch)) => {
                            req = req.require_architecture(arch);
                        }
                        _ => {}
                    }
                }
            }
        }
        return req;
    }

    // Fallback: hardcoded rules for offerings without manifest metadata
    match name {
        "mongodb" => Requirements::new()
            .require_cpu_feature("avx")
            .require_memory_mb(2048),
        "postgres" => Requirements::new().require_memory_mb(1024),
        _ => Requirements::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_serialization() {
        let offering = Update::Offering {
            name: "redis".to_string(),
            current: "7.2.3".to_string(),
            available: "7.2.4".to_string(),
            age_days: Some(45),
        };

        let json = serde_json::to_string(&offering).unwrap();
        assert!(json.contains(r#""type":"offering""#));
        assert!(json.contains(r#""name":"redis""#));
    }

    #[test]
    fn test_firmware_serialization() {
        let firmware = Update::Firmware {
            device_id: "com.dell.bios".to_string(),
            name: "System BIOS".to_string(),
            vendor: "Dell Inc.".to_string(),
            current: "1.2.3".to_string(),
            available: "1.2.4".to_string(),
            requires_reboot: true,
            description: Some("Security fixes".to_string()),
            confidence: garden_common::nourishment::FirmwareConfidence::Tested,
        };

        let json = serde_json::to_string(&firmware).unwrap();
        assert!(json.contains(r#""type":"firmware""#));
        assert!(json.contains(r#""device_id":"com.dell.bios""#));
    }

    #[test]
    fn test_execute_request_deserialization() {
        // items are simple strings like "offering:redis" or "firmware:com.dell.bios"
        let json = r#"{"items":["offering:redis","firmware:com.dell.bios"]}"#;

        let req: ExecuteRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.items.len(), 2);
        assert_eq!(req.items[0], "offering:redis");
        assert_eq!(req.items[1], "firmware:com.dell.bios");
    }
}
