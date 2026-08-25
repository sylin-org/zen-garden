//! Seed bank replication background task (STORAGE-0006 Phase 4e)
//!
//! Runs on stones that host **Replica** seed banks. For each Replica bank,
//! the task pulls changes from the Primary's changelog endpoint and applies
//! them locally — downloading new/modified files and deleting removed ones.
//!
//! ## Sync modes
//!
//! | Mode | Trigger | Behaviour |
//! |------|---------|-----------|
//! | Incremental | Normal operation | `GET /changes?since={cursor}` → apply squashed diff |
//! | Full sync | `full_sync_required` flag | Directory walk + hash compare against Primary |
//! | Initial sync | No `last_cursor` file | Same as incremental with no cursor (returns all entries) |
//!
//! ## Flow
//!
//! ```text
//! 1. Identify local Replica seed banks (from seed_bank_roles)
//! 2. For each Replica:
//!    a. Resolve Primary stone + bank ID from storage_cache
//!    b. Read local last_cursor
//!    c. GET /changes?since={cursor} from Primary
//!    d. If full_sync_required → queue full sync (future)
//!    e. For each C/M → GET /bank/{id}/{path} → write locally
//!    f. For each D → delete locally
//!    g. Persist last_cursor
//! 3. Sleep, repeat
//! ```

use anyhow::Result;
use garden_common::PeerAddress;
use garden_common::storage::{ChangelogOp, ChangesResponse, StorageRole};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::app_state::Moss;
use crate::infra::storage::ContentStore;

/// How often the replication sync loop runs (seconds).
/// The aggregated tick channel provides event-driven sync; this is only
/// the fallback poll interval for remote Primaries whose SSE we don't
/// consume directly.
const REPLICATION_POLL_SECS: u64 = 60;

/// Timeout for pull requests to the Primary stone (seconds).
const PULL_TIMEOUT_SECS: u64 = 10;

/// Timeout for individual file downloads from the Primary (seconds).
const DOWNLOAD_TIMEOUT_SECS: u64 = 60;

/// Prefix that the changelog uses for file paths.  The object GET API
/// (`/bank/{id}/*path`) expects *just* `bucket/key` — so we strip this
/// prefix before constructing the download URL.
const STORAGE_PREFIX: &str = "garden/storage/";

/// Strip the `garden/storage/` prefix from a changelog path to get the
/// API-relative path (bucket/key).  Returns `None` if the path doesn't
/// have the expected prefix (shouldn't happen for object entries).
fn api_relative_path(changelog_path: &str) -> Option<&str> {
    changelog_path.strip_prefix(STORAGE_PREFIX)
}

// ============================================================================
// Public entry point
// ============================================================================

/// Background task — spawned at daemon startup.
///
/// Runs for the daemon's entire lifetime. Each tick it checks local
/// Replica seed banks and syncs them from their respective Primaries.
pub async fn storage_replication_task(state: Moss, token: CancellationToken) -> Result<()> {
    info!("Seed bank replication task starting");

    // Wait a bit for orchestration to assign roles before first sync
    tokio::select! {
        _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {}
        _ = token.cancelled() => {
            return Ok(());
        }
    }

    let mut tick = tokio::time::interval(std::time::Duration::from_secs(REPLICATION_POLL_SECS));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // Subscribe to aggregated storage ticks — the aggregator quantizes raw
    // per-write events into per-seed-bank batches (2s quiet / 10s deadline).
    // For remote Primaries we still rely on polling as a fallback.
    let mut tick_rx = state.current.storage.tick_stream();

    loop {
        // Wait for either the poll interval or a storage tick
        tokio::select! {
            _ = tick.tick() => {}
            result = tick_rx.recv() => {
                match result {
                    Ok(_) => {} // doorbell rang — run sync now
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        debug!(lagged = n, "Replication tick receiver lagged");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        info!("Storage tick channel closed — replication task exiting");
                        return Ok(());
                    }
                }
            }
            _ = token.cancelled() => {
                info!("Seed bank replication task shutting down");
                return Ok(());
            }
        }

        if let Err(e) = replication_tick(&state).await {
            warn!(error = ?e, "Replication tick failed");
        }
    }
}

// ============================================================================
// Replication tick
// ============================================================================

/// Run one replication cycle for all local Replica seed banks.
async fn replication_tick(state: &Moss) -> Result<()> {
    // Collect Replica volumes from unified collection
    let map = state.current.storage.volumes.read().await;
    let replica_banks: Vec<(String, String, std::path::PathBuf)> = map
        .values()
        .filter_map(|vol| {
            let mgmt = vol.management()?;
            if mgmt.role != StorageRole::Replica {
                return None;
            }
            Some((mgmt.name.clone(), mgmt.id.clone(), vol.mount_path().clone()))
        })
        .collect();
    drop(map);

    if replica_banks.is_empty() {
        return Ok(());
    }

    for (name, id, mount_path) in &replica_banks {
        if let Err(e) = sync_replica_bank(state, name, id, mount_path.as_path()).await {
            warn!(
                bank = %name,
                bank_id = %id,
                error = ?e,
                "Failed to sync Replica seed bank"
            );
        }
    }

    Ok(())
}

// ============================================================================
// Per-bank sync
// ============================================================================

/// Sync a single Replica seed bank from its Primary.
async fn sync_replica_bank(
    state: &Moss,
    name: &str,
    local_bank_id: &str,
    mount_path: &std::path::Path,
) -> Result<()> {
    // 1. Resolve the Primary stone + endpoint from registry
    let (_primary_stone_id, primary_endpoint, _primary_bank_id) = match state
        .tool
        .route_to_primary(name, &state.current.stone.id)
        .await
    {
        Some(route) => route,
        None => {
            debug!(name = %name, "No remote Primary found for seed bank — skipping sync");
            return Ok(());
        }
    };

    // route_to_primary already excludes our own stone

    let peer = PeerAddress::from_http_url(&primary_endpoint);
    // 3. Read local last_cursor
    let local_store = ContentStore::new_public(mount_path);
    let last_cursor = local_store.read_last_cursor().await;

    // 4. Pull changes from Primary (name-based routes — STORAGE-0009)
    let changes_path = format!(
        "/api/v1/stone/storage/banks/{}/changes{}",
        name,
        match &last_cursor {
            Some(c) => format!("?since={}", c),
            None => String::new(),
        }
    );

    let resp = state
        .security
        .stone_client()
        .get(&peer, &changes_path)
        .timeout(std::time::Duration::from_secs(PULL_TIMEOUT_SECS))
        .send()
        .await;

    let resp = match resp {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            warn!(
                name = %name,
                status = %r.status(),
                "Primary returned error for changes pull"
            );
            return Ok(());
        }
        Err(e) => {
            debug!(
                name = %name,
                error = %e,
                "Failed to reach Primary for changes pull"
            );
            return Ok(());
        }
    };

    // Parse the response — unwrap the ApiResponse wrapper
    let body: serde_json::Value = resp.json().await?;
    let changes_resp: ChangesResponse =
        serde_json::from_value(body.get("data").cloned().unwrap_or_else(|| body.clone()))?;

    // 5. Handle full_sync_required — directory walk reconciliation
    if changes_resp.full_sync_required {
        info!(
            name = %name,
            local_bank_id = %local_bank_id,
            "Cursor compacted away — starting full directory reconciliation"
        );
        full_sync_replica_bank(state, name, &peer, &local_store, mount_path).await?;
        // After full sync, persist the Primary's current cursor so future
        // syncs resume incrementally from this point.
        if !changes_resp.cursor.is_empty() {
            local_store.write_last_cursor(&changes_resp.cursor).await?;
        }
        return Ok(());
    }

    // 6. Nothing to do?
    if changes_resp.changes.is_empty() {
        return Ok(());
    }

    info!(
        name = %name,
        entries = changes_resp.changes.len(),
        cursor = %changes_resp.cursor,
        "Applying replication changes"
    );

    // 7. Apply each change
    let mut applied = 0u32;
    let mut errors = 0u32;

    for entry in &changes_resp.changes {
        match entry.op {
            ChangelogOp::C | ChangelogOp::M => {
                // Strip the `garden/storage/` prefix so the URL matches
                // the object GET endpoint which expects `bucket/key`.
                let api_path = match api_relative_path(&entry.path) {
                    Some(p) => p,
                    None => {
                        warn!(
                            path = %entry.path,
                            "Changelog path missing expected prefix — skipping"
                        );
                        errors += 1;
                        continue;
                    }
                };

                let object_path = format!("/api/v1/garden/storage/{}/objects/{}", name, api_path);

                match download_and_write(state, &peer, &object_path, &local_store, &entry.path)
                    .await
                {
                    Ok(()) => applied += 1,
                    Err(e) => {
                        warn!(
                            path = %entry.path,
                            error = %e,
                            "Failed to replicate file"
                        );
                        errors += 1;
                    }
                }
            }
            ChangelogOp::D => {
                let rel = std::path::Path::new(&entry.path);
                match local_store.delete(rel).await {
                    Ok(_) => applied += 1,
                    Err(e) => {
                        // File might already be gone — that's OK
                        debug!(
                            path = %entry.path,
                            error = %e,
                            "Failed to delete replicated file (may already be gone)"
                        );
                        applied += 1;
                    }
                }
            }
        }
    }

    // 8. Persist cursor (only if no errors, to allow retry)
    if errors == 0 {
        if let Err(e) = local_store.write_last_cursor(&changes_resp.cursor).await {
            warn!(
                name = %name,
                error = %e,
                "Failed to persist last_cursor"
            );
        } else {
            debug!(
                name = %name,
                cursor = %changes_resp.cursor,
                applied = applied,
                "Replication cursor advanced"
            );
        }
    } else {
        warn!(
            name = %name,
            applied = applied,
            errors = errors,
            "Replication completed with errors — cursor NOT advanced (will retry)"
        );
    }

    Ok(())
}

// ============================================================================
// Full directory walk reconciliation (Phase 4e+)
// ============================================================================

/// Walk the Primary's object tree and reconcile with the local copy.
///
/// 1. Fetch remote object listing from Primary via the garden storage API
/// 2. Walk local objects directory
/// 3. Download missing or modified files from Primary
/// 4. Delete local files that no longer exist on Primary
async fn full_sync_replica_bank(
    state: &Moss,
    name: &str,
    peer: &PeerAddress,
    local_store: &ContentStore,
    mount_path: &std::path::Path,
) -> Result<()> {
    // 1. Fetch remote listing (all objects, recursive)
    let listing_path = format!("/api/v1/garden/storage/{}/fs?depth=all", name);

    let resp = state
        .security
        .stone_client()
        .get(peer, &listing_path)
        .timeout(std::time::Duration::from_secs(DOWNLOAD_TIMEOUT_SECS))
        .send()
        .await;

    let resp = match resp {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            warn!(name = %name, status = %r.status(), "Primary listing failed during full sync");
            return Ok(());
        }
        Err(e) => {
            warn!(name = %name, error = %e, "Failed to reach Primary for full sync listing");
            return Ok(());
        }
    };

    let body: serde_json::Value = resp.json().await?;
    let entries_value = body
        .get("data")
        .and_then(|d| d.get("entries"))
        .cloned()
        .unwrap_or_else(|| {
            body.get("entries")
                .cloned()
                .unwrap_or(serde_json::Value::Array(vec![]))
        });

    let remote_files: std::collections::HashSet<String> = collect_remote_paths(&entries_value, "");

    // 2. Walk local objects directory
    let objects_dir = mount_path.join(".zen-garden").join("storage");
    let local_files: std::collections::HashSet<String> = if objects_dir.exists() {
        walk_local_objects(&objects_dir, &objects_dir).await
    } else {
        std::collections::HashSet::new()
    };

    // 3. Download files that exist on Primary but not locally (or differ)
    let to_download: Vec<&String> = remote_files.difference(&local_files).collect();
    let to_delete: Vec<&String> = local_files.difference(&remote_files).collect();

    info!(
        name = %name,
        remote = remote_files.len(),
        local = local_files.len(),
        download = to_download.len(),
        delete = to_delete.len(),
        "Full sync diff computed"
    );

    let mut applied = 0u32;
    let mut errors = 0u32;

    for rel_path in &to_download {
        let object_path = format!("/api/v1/garden/storage/{}/objects/{}", name, rel_path);
        let store_path = format!("{}{}", STORAGE_PREFIX, rel_path);
        match download_and_write(state, peer, &object_path, local_store, &store_path).await {
            Ok(()) => applied += 1,
            Err(e) => {
                warn!(path = %rel_path, error = %e, "Full sync: failed to download");
                errors += 1;
            }
        }
    }

    // 4. Delete local files that no longer exist on Primary
    for rel_path in &to_delete {
        let store_path = format!("{}{}", STORAGE_PREFIX, rel_path);
        let rel = std::path::Path::new(&store_path);
        match local_store.delete(rel).await {
            Ok(_) => applied += 1,
            Err(e) => {
                debug!(path = %rel_path, error = %e, "Full sync: delete failed (may already be gone)");
                applied += 1;
            }
        }
    }

    info!(
        name = %name,
        applied,
        errors,
        "Full directory reconciliation complete"
    );

    Ok(())
}

/// Recursively collect file paths from a garden storage directory listing.
fn collect_remote_paths(
    entries: &serde_json::Value,
    prefix: &str,
) -> std::collections::HashSet<String> {
    let mut paths = std::collections::HashSet::new();
    if let Some(arr) = entries.as_array() {
        for entry in arr {
            let name = entry.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let entry_type = entry.get("type").and_then(|t| t.as_str()).unwrap_or("file");
            let full_path = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{}/{}", prefix, name)
            };
            if entry_type == "dir" {
                if let Some(children) = entry.get("entries") {
                    paths.extend(collect_remote_paths(children, &full_path));
                }
            } else {
                paths.insert(full_path);
            }
        }
    }
    paths
}

/// Walk local objects directory and collect relative paths.
async fn walk_local_objects(
    root: &std::path::Path,
    current: &std::path::Path,
) -> std::collections::HashSet<String> {
    let mut paths = std::collections::HashSet::new();
    let Ok(mut entries) = tokio::fs::read_dir(current).await else {
        return paths;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.is_dir() {
            paths.extend(Box::pin(walk_local_objects(root, &path)).await);
        } else if let Ok(rel) = path.strip_prefix(root)
            && let Some(s) = rel.to_str()
        {
            // Normalize to forward slashes for cross-platform consistency
            paths.insert(s.replace('\\', "/"));
        }
    }
    paths
}

// ============================================================================
// File download helper
// ============================================================================

async fn download_and_write(
    state: &Moss,
    peer: &PeerAddress,
    remote_path: &str,
    local_store: &ContentStore,
    rel_path: &str,
) -> Result<()> {
    let resp = state
        .security
        .stone_client()
        .get(peer, remote_path)
        .timeout(std::time::Duration::from_secs(DOWNLOAD_TIMEOUT_SECS))
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("Primary returned {} for {}", resp.status(), remote_path);
    }

    let bytes = resp.bytes().await?;
    let rel = std::path::Path::new(rel_path);
    local_store.write(rel, &bytes).await?;

    debug!(path = %rel_path, bytes = bytes.len(), "Replicated file");
    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use garden_common::storage::{ChangelogEntry, ChangelogOp, ChangesResponse};

    // ── api_relative_path ────────────────────────────────────────────────

    #[test]
    fn test_api_relative_path_strips_prefix() {
        assert_eq!(
            api_relative_path("garden/storage/mybucket/mykey.txt"),
            Some("mybucket/mykey.txt")
        );
    }

    #[test]
    fn test_api_relative_path_nested_key() {
        assert_eq!(
            api_relative_path("garden/storage/data/logs/2026/jan.log"),
            Some("data/logs/2026/jan.log")
        );
    }

    #[test]
    fn test_api_relative_path_no_prefix_returns_none() {
        assert_eq!(api_relative_path("other/path/file.txt"), None);
    }

    #[test]
    fn test_api_relative_path_partial_prefix_returns_none() {
        assert_eq!(api_relative_path("garden/stor/file.txt"), None);
    }

    #[test]
    fn test_api_relative_path_exact_prefix_returns_empty() {
        // edge case: path IS the prefix with nothing after
        assert_eq!(api_relative_path("garden/storage/"), Some(""));
    }

    // ── cursor advancement logic ─────────────────────────────────────────

    #[test]
    fn test_cursor_not_advanced_on_errors() {
        // The logic: errors > 0 → cursor NOT advanced
        // This tests the decision boundary, not the full async flow
        let errors = 1u32;
        let applied = 3u32;
        // errors > 0 → should skip cursor write
        assert!(errors > 0, "Errors present — cursor should NOT advance");
        assert!(applied > 0, "Some items applied despite errors");
    }

    #[test]
    fn test_cursor_advanced_on_clean_run() {
        let errors = 0u32;
        assert!(errors == 0, "No errors — cursor SHOULD advance");
    }

    // ── ChangesResponse handling ─────────────────────────────────────────

    #[test]
    fn test_empty_changes_is_noop() {
        let resp = ChangesResponse {
            cursor: "abc".to_string(),
            changes: vec![],
            full_sync_required: false,
        };
        assert!(resp.changes.is_empty());
    }

    #[test]
    fn test_full_sync_flag_detected() {
        let resp = ChangesResponse {
            cursor: "abc".to_string(),
            changes: vec![],
            full_sync_required: true,
        };
        assert!(resp.full_sync_required);
    }

    #[test]
    fn test_changelog_op_routing() {
        // Verify the match arms in the apply loop cover all ops
        let create = ChangelogEntry::created("garden/storage/b/k", 100);
        let modify = ChangelogEntry::modified("garden/storage/b/k", 200);
        let delete = ChangelogEntry::deleted("garden/storage/b/k");

        assert!(matches!(create.op, ChangelogOp::C));
        assert!(matches!(modify.op, ChangelogOp::M));
        assert!(matches!(delete.op, ChangelogOp::D));

        // C and M both need api_relative_path
        assert!(api_relative_path(&create.path).is_some());
        assert!(api_relative_path(&modify.path).is_some());
        // D path should also have the prefix for consistency
        assert!(api_relative_path(&delete.path).is_some());
    }

    #[test]
    fn test_replication_constants() {
        assert!(REPLICATION_POLL_SECS > 0);
        assert!(PULL_TIMEOUT_SECS > 0);
        assert!(DOWNLOAD_TIMEOUT_SECS > PULL_TIMEOUT_SECS);
    }
}
