//! Storage background tasks
//!
//! - S3 listener lifecycle: arm/disarm per-storage S3 ports
//! - Storage console: render connected/released ribbons to physical console
//!
//! Storage lifecycle (auto-mount, health ticks) is handled by
//! `StorageLifecycleTask` (BackgroundTask, ARCH-0015).
//! Storage announcements are event-driven via `emit_storage_changed()`.

use crate::AppState;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// S3 listener lifecycle (STORAGE-0016): react to storage events.
///
/// - On `Added`/`Connected`/`Reclassified`: scan volumes and arm S3 listeners
///   for managed Primary storage that doesn't have a listener yet.
/// - On `Removed`/`Released`: disarm the S3 listener for the replica set.
/// - On `RoleChanged`: arm if promoted to Primary, disarm if demoted.
pub fn start_s3_listener_lifecycle(state: AppState, token: CancellationToken) {
    tokio::spawn(async move {
        let mut rx = state.subscribe_storage_changed();

        tracing::info!("S3 listener lifecycle task started (STORAGE-0016)");

        // Initial arm: scan all volumes and arm listeners for existing primaries
        arm_s3_for_all_primaries(&state).await;

        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    tracing::debug!("S3 listener lifecycle shutting down");
                    break;
                }
                result = rx.recv() => match result {
                    Ok(garden_common::storage::StorageChanged::Added { .. })
                    | Ok(garden_common::storage::StorageChanged::Connected { .. })
                    | Ok(garden_common::storage::StorageChanged::Reclassified) => {
                        arm_s3_for_all_primaries(&state).await;
                    }
                    Ok(garden_common::storage::StorageChanged::Removed { .. })
                    | Ok(garden_common::storage::StorageChanged::Released { .. }) => {
                        // Re-scan: disarm any listeners for replica sets that no longer have a local primary
                        reconcile_s3_listeners(&state).await;
                    }
                    Ok(garden_common::storage::StorageChanged::RoleChanged { .. }) => {
                        // Role change could promote or demote — full reconcile
                        reconcile_s3_listeners(&state).await;
                        arm_s3_for_all_primaries(&state).await;
                    }
                    Ok(_) => {} // Renamed, PinChanged, Sensed — no S3 impact
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        // Missed events — do a full reconcile
                        reconcile_s3_listeners(&state).await;
                        arm_s3_for_all_primaries(&state).await;
                    }
                },
            }
        }
    });
}

/// Arm S3 listeners for all managed Primary volumes that don't have one yet.
pub(crate) async fn arm_s3_for_all_primaries(state: &AppState) {
    let volumes = state.current.storage.volumes.read().await;
    for vol in volumes.values() {
        if let Some(mgmt) = vol.management()
            && mgmt.role == garden_common::storage::StorageRole::Primary
        {
            let name = mgmt.display_name().to_string();
            let storage_id = mgmt.id.clone();
            let s3 = &state.orchestration.storage.s3_listeners;
            if let Some(port) = s3.arm(&name, &storage_id, state.clone()).await {
                tracing::debug!(
                    replica_set = %name,
                    port,
                    "S3 listener armed for primary storage"
                );
            }
        }
    }
}

/// Remove S3 listeners for replica sets that no longer have a local Primary volume.
pub(crate) async fn reconcile_s3_listeners(state: &AppState) {
    let s3 = &state.orchestration.storage.s3_listeners;
    let assignments = s3.assignments().await;
    let volumes = state.current.storage.volumes.read().await;

    for assignment in &assignments {
        let has_local_primary = volumes.values().any(|v| {
            v.management()
                .map(|m| {
                    m.display_name() == assignment.replica_set_name
                        && m.role == garden_common::storage::StorageRole::Primary
                })
                .unwrap_or(false)
        });

        if !has_local_primary {
            if volumes.values().any(|v| {
                v.management()
                    .map(|m| m.display_name() == assignment.replica_set_name)
                    .unwrap_or(false)
            }) {
                // Volume exists but is offline/dormant — 503 degradation
                s3.set_offline(&assignment.replica_set_name).await;
            } else {
                // Volume completely gone — disarm
                s3.disarm(&assignment.replica_set_name).await;
            }
        } else if !assignment.online {
            // Volume is back as Primary — resume
            s3.set_online(&assignment.replica_set_name).await;
        }
    }
}

/// Subscribe to `StorageChanged` and render storage ribbons to the physical console.
///
/// Delegates to `PlatformRuntime` so output goes to the appropriate destination
/// on each platform (TTY1 on Linux, stdout on Windows).
pub fn start_storage_console_task(
    runtime: Arc<dyn garden_common::PlatformRuntime>,
    rx: tokio::sync::broadcast::Receiver<garden_common::storage::StorageChanged>,
    token: CancellationToken,
) {
    use garden_common::storage::StorageChanged;

    tokio::spawn(async move {
        let mut rx = rx;
        loop {
            tokio::select! {
                _ = token.cancelled() => break,
                result = rx.recv() => match result {
                    Ok(StorageChanged::Sensed { .. }) => {}
                    Ok(StorageChanged::Connected { name, roles, used_bytes, capacity_bytes }) => {
                        runtime.print_storage_connected(&name, &roles, used_bytes, capacity_bytes);
                    }
                    Ok(StorageChanged::Released { name }) => {
                        runtime.print_storage_released(&name);
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                },
            }
        }
    });
}
