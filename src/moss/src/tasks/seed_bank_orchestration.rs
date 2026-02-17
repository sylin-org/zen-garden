//! Seed bank orchestration background task (STORAGE-0006)
//!
//! Assigns Primary / Dormant roles to seed banks based on garden-wide
//! storage beacons. Mirrors the offering orchestration pattern:
//!
//! - Startup reconciliation: wait 3 s before asserting Primary
//! - First-online-wins: first stone to announce a name becomes Primary
//! - Dual-primary resolution: lower stone_id yields to higher
//! - 6 s stale detection (2 × reconciliation window)
//!
//! The task updates `state.seed_bank_roles` which the beacon builder
//! reads when constructing `SeedBankAnnouncement::role`.

use anyhow::Result;
use garden_common::storage::SeedBankRole;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::app_state::AppState;
use crate::infra::storage::SeedBankRegistry;

/// Startup reconciliation window — wait before asserting Primary (ms).
const STARTUP_RECONCILIATION_MS: u64 = 3_000;

/// How often the orchestration tick runs (seconds).
const ORCHESTRATION_TICK_SECS: u64 = 3;

/// How often changelog compaction runs (seconds). Once per hour.
const COMPACTION_INTERVAL_SECS: u64 = 3_600;

/// Changelog retention window. Entries older than this are pruned.
/// A Dormant whose cursor falls behind this window triggers full sync.
const CHANGELOG_RETENTION: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 3600);

// ============================================================================
// Public entry point
// ============================================================================

/// Background task — spawned at daemon startup.
///
/// Runs for the daemon's entire lifetime. Every tick it scans local seed banks,
/// compares against garden-wide beacons, and assigns Primary/Dormant per FQN.
pub async fn seed_bank_orchestration_task(state: AppState, token: CancellationToken) -> Result<()> {
    info!("Seed bank orchestration task starting");

    // Phase 1: Startup reconciliation — wait before asserting Primary
    startup_reconciliation(&state, &token).await?;

    // Phase 2: Main loop
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(ORCHESTRATION_TICK_SECS));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let mut compaction_tick =
        tokio::time::interval(std::time::Duration::from_secs(COMPACTION_INTERVAL_SECS));
    compaction_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = tick.tick() => {
                if let Err(e) = orchestration_tick(&state).await {
                    warn!(error = ?e, "Seed bank orchestration tick failed");
                }
            }
            _ = state.orchestration_nudge.notified() => {
                debug!("Orchestration nudge received — running immediate tick");
                if let Err(e) = orchestration_tick(&state).await {
                    warn!(error = ?e, "Seed bank orchestration tick (nudge) failed");
                }
            }
            _ = compaction_tick.tick() => {
                compact_primary_changelogs(&state).await;
            }
            _ = token.cancelled() => {
                info!("Seed bank orchestration task shutting down");
                return Ok(());
            }
        }
    }
}

// ============================================================================
// Startup reconciliation
// ============================================================================

/// Wait one reconciliation window before asserting Primary.
///
/// During this window, beacons from other stones may arrive declaring
/// themselves Primary for a given seed bank name. If that happens, the
/// local bank yields to Dormant.
async fn startup_reconciliation(state: &AppState, token: &CancellationToken) -> Result<()> {
    info!(
        window_ms = STARTUP_RECONCILIATION_MS,
        "Startup reconciliation: waiting before asserting Primary for seed banks"
    );

    tokio::select! {
        _ = tokio::time::sleep(std::time::Duration::from_millis(STARTUP_RECONCILIATION_MS)) => {}
        _ = token.cancelled() => {
            return Ok(());
        }
    }

    // After the window, run one tick to assign initial roles
    if let Err(e) = orchestration_tick(state).await {
        warn!(error = ?e, "Initial seed bank role assignment failed");
    }

    info!("Startup reconciliation complete for seed banks");
    Ok(())
}

// ============================================================================
// Main orchestration tick
// ============================================================================

/// Evaluate and assign roles for all local seed banks.
///
/// For each local seed bank name:
/// 1. Check if any remote stone already claims Primary for that name.
/// 2. If no remote Primary → assert Primary.
/// 3. If remote Primary exists → become Dormant.
/// 4. If both local and remote claim Primary (dual-primary) → lower stone_id yields.
/// 5. Pinned Primary always wins — never reassigned by orchestration.
async fn orchestration_tick(state: &AppState) -> Result<()> {
    // Scan local seed banks
    let registry = match SeedBankRegistry::scan().await {
        Ok(r) => r,
        Err(e) => {
            debug!(error = %e, "Skipping seed bank orchestration — scan failed");
            return Ok(());
        }
    };

    let local_banks = registry.list();
    if local_banks.is_empty() {
        return Ok(());
    }

    // Collect unique local names
    let mut local_names: Vec<String> = local_banks.iter().map(|b| b.name.clone()).collect();
    local_names.sort();
    local_names.dedup();

    let my_stone_id = &state.stone_id;
    let cache = state.storage_cache.read().await;
    let pins = state.seed_bank_pins.read().await;

    let mut new_roles = std::collections::HashMap::new();
    let mut any_changed = false;
    let mut auto_unpin: Vec<String> = Vec::new();

    for name in &local_names {
        let local_pin_id = pins.get(name).cloned();

        // Find remote beacons that have this name as Primary
        let remote_primary = find_remote_primary_with_pin(&cache, name, my_stone_id);

        let current_role = {
            let roles = state.seed_bank_roles.read().await;
            roles.get(name).copied().unwrap_or(SeedBankRole::Primary)
        };

        let new_role = {
            let remote = remote_primary
                .as_ref()
                .map(|(sid, _, pin_id)| (sid.as_str(), pin_id.as_deref()));
            resolve_role(local_pin_id.as_deref(), current_role, remote, my_stone_id)
        };

        if new_role != current_role {
            any_changed = true;
            info!(
                name = %name,
                old_role = %current_role,
                new_role = %new_role,
                "Seed bank role changed"
            );
        }

        // Auto-unpin: if we had a pin but lost to a higher remote pin_id,
        // clear the local pin so the winner is undisputed.
        if let Some(ref local_pid) = local_pin_id {
            if new_role == SeedBankRole::Dormant {
                if let Some((_, _, Some(ref remote_pid))) = remote_primary {
                    if remote_pid > local_pid {
                        info!(
                            name = %name,
                            local_pin = %local_pid,
                            remote_pin = %remote_pid,
                            "Auto-unpinning: remote pin_id is newer"
                        );
                        auto_unpin.push(name.clone());
                    }
                }
            }
        }

        if let Some((ref remote_stone_id, _, _)) = remote_primary {
            if current_role == SeedBankRole::Primary && new_role != current_role {
                debug!(
                    name = %name,
                    remote_stone = %remote_stone_id,
                    locally_pinned = local_pin_id.is_some(),
                    "Role decision: yielding to remote"
                );
            }
        }

        new_roles.insert(name.clone(), new_role);
    }

    drop(pins);
    drop(cache);

    // Apply auto-unpin for pins that lost to a newer remote pin_id
    if !auto_unpin.is_empty() {
        let mut pins = state.seed_bank_pins.write().await;
        for name in &auto_unpin {
            pins.remove(name);
            // Delete persisted pin file from disk
            if let Some(bank) = local_banks.iter().find(|b| &b.name == name) {
                let store =
                    crate::infra::storage::SeedBankStore::new_public(&bank.mount_path);
                if let Err(e) = store.delete_pin().await {
                    warn!(name = %name, error = %e, "Failed to delete auto-unpinned pin file");
                }
            }
        }
        any_changed = true;
    }

    // Write all roles at once
    {
        let mut roles = state.seed_bank_roles.write().await;
        for (name, role) in new_roles {
            roles.insert(name, role);
        }

        // Prune roles for seed banks no longer present locally
        let before = roles.len();
        roles.retain(|name, _| local_names.contains(name));
        if roles.len() != before {
            any_changed = true;
        }
    }

    // If any role changed, update local beacon cache and broadcast to network
    // so other stones see the resolved roles (not default Primary).
    if any_changed {
        let endpoint = state.self_entry.read().await.address.http_base();
        let roles = state.seed_bank_roles.read().await;
        let pins = state.seed_bank_pins.read().await;
        if let Err(e) = crate::infra::storage::update_and_broadcast(
            &state.storage_cache,
            &state.stone_id,
            &state.stone_name,
            &endpoint,
            Some(&roles),
            Some(&pins),
        )
        .await
        {
            warn!(error = ?e, "Failed to broadcast beacon after role change");
        }
    }

    Ok(())
}

// ============================================================================
// Changelog compaction
// ============================================================================

/// Compact changelogs on all local Primary seed banks.
///
/// Runs hourly. For each Primary bank, computes a 7-day cutoff cursor
/// from the current time and prunes older entries. A GUIDv7 cursor
/// embeds its timestamp, so the cutoff is a synthetic GUIDv7 at
/// `now - CHANGELOG_RETENTION`.
async fn compact_primary_changelogs(state: &AppState) {
    let registry = match SeedBankRegistry::scan().await {
        Ok(r) => r,
        Err(e) => {
            debug!(error = %e, "Skipping changelog compaction — scan failed");
            return;
        }
    };

    let roles = state.seed_bank_roles.read().await;

    for bank in registry.list() {
        // Only compact Primary banks — Dormant banks don't own the changelog
        let role = roles
            .get(&bank.name)
            .copied()
            .unwrap_or(SeedBankRole::Primary);
        if role != SeedBankRole::Primary {
            continue;
        }

        let store = crate::infra::storage::SeedBankStore::new_public(&bank.mount_path);

        // Build a synthetic GUIDv7-comparable cutoff string.
        // GUIDv7 embeds a 48-bit Unix-ms timestamp in the first 12 hex chars.
        // We compute `now - retention` as milliseconds and format as a GUIDv7 prefix
        // so string comparison works correctly against real GUIDv7 cursors.
        let cutoff_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .saturating_sub(CHANGELOG_RETENTION.as_millis()) as u64;

        let cutoff_cursor = build_cutoff_cursor(cutoff_ms);

        match store.compact_changelog(&cutoff_cursor).await {
            Ok(0) => {} // nothing to prune
            Ok(pruned) => {
                info!(
                    bank = %bank.name,
                    bank_id = %bank.id,
                    pruned = pruned,
                    "Changelog compacted"
                );
            }
            Err(e) => {
                warn!(
                    bank = %bank.name,
                    bank_id = %bank.id,
                    error = %e,
                    "Changelog compaction failed"
                );
            }
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Pure role decision: given current state, what role should this seed bank have?
///
/// Implements last-pin-wins with GUIDv7 comparison:
///
/// 1. No remote Primary → become Primary.
/// 2. Both pinned → higher (later) GUIDv7 pin_id wins.
/// 3. Only local pinned → local wins.
/// 4. Only remote pinned → remote wins.
/// 5. Neither pinned → current Primary with higher stone_id wins.
///
/// A locally-pinned Dormant will promote to Primary (branches 2-3),
/// enabling "claim Primary from any replica" semantics.
///
/// Arguments:
/// - `local_pin_id` — this bank's GUIDv7 pin_id, if pinned locally
/// - `current_role` — the bank's current role
/// - `remote_primary` — if a remote Primary exists: `(stone_id, pin_id)`
/// - `my_stone_id` — this stone's ID (for dual-primary tiebreaker)
fn resolve_role(
    local_pin_id: Option<&str>,
    current_role: SeedBankRole,
    remote_primary: Option<(&str, Option<&str>)>,
    my_stone_id: &str,
) -> SeedBankRole {
    match remote_primary {
        None => SeedBankRole::Primary,
        Some((remote_stone_id, remote_pin_id)) => {
            match (local_pin_id, remote_pin_id) {
                // Both pinned → higher (later) GUIDv7 wins
                (Some(lp), Some(rp)) => {
                    if lp > rp {
                        SeedBankRole::Primary
                    } else {
                        SeedBankRole::Dormant
                    }
                }
                // Only local pinned → local wins (claim Primary)
                (Some(_), None) => SeedBankRole::Primary,
                // Only remote pinned → remote wins
                (None, Some(_)) => SeedBankRole::Dormant,
                // Neither pinned → current Primary with higher stone_id wins
                (None, None) => {
                    if current_role == SeedBankRole::Primary
                        && my_stone_id > remote_stone_id
                    {
                        SeedBankRole::Primary
                    } else {
                        SeedBankRole::Dormant
                    }
                }
            }
        }
    }
}

/// Build a synthetic GUIDv7-comparable cutoff cursor from a Unix-ms timestamp.
///
/// GUIDv7 format: `TTTTTTTT-TTTT-7xxx-yxxx-xxxxxxxxxxxx`
/// First 12 hex digits (8-4) encode the 48-bit Unix-ms timestamp.
/// The remaining fields are zeroed to produce the smallest possible
/// GUIDv7 for that timestamp, so string comparison against real
/// GUIDv7 cursors correctly partitions older-vs-newer entries.
fn build_cutoff_cursor(cutoff_ms: u64) -> String {
    format!(
        "{:08x}-{:04x}-7000-8000-000000000000",
        (cutoff_ms >> 16) as u32,
        (cutoff_ms & 0xFFFF) as u16,
    )
}

/// Find the first remote stone claiming Primary for a given seed bank name.
///
/// Returns `(stone_id, seed_bank_id, pin_id)` of the remote primary, if any.
fn find_remote_primary_with_pin(
    cache: &crate::domain::storage_cache::StorageCacheInner,
    name: &str,
    my_stone_id: &str,
) -> Option<(String, String, Option<String>)> {
    for beacon in cache.all_beacons() {
        if beacon.stone_id == my_stone_id {
            continue;
        }
        for sb in &beacon.seed_banks {
            if sb.name == name && sb.role == SeedBankRole::Primary {
                return Some((beacon.stone_id.clone(), sb.id.clone(), sb.pin_id.clone()));
            }
        }
    }
    None
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::storage_cache::StorageCacheInner;
    use chrono::Utc;
    use garden_common::storage::{SeedBankAnnouncement, StorageAccess, StorageBeacon};

    // -- resolve_role ---------------------------------------------------------

    #[test]
    fn test_resolve_role_no_remote_primary() {
        // No remote primary → always Primary regardless of current role.
        let role = resolve_role(None, SeedBankRole::Primary, None, "stone-aaa");
        assert_eq!(role, SeedBankRole::Primary);
    }

    #[test]
    fn test_resolve_role_no_remote_even_if_dormant() {
        // Was Dormant but remote disappeared → promote to Primary.
        let role = resolve_role(None, SeedBankRole::Dormant, None, "stone-aaa");
        assert_eq!(role, SeedBankRole::Primary);
    }

    #[test]
    fn test_resolve_role_locally_pinned_newer_wins() {
        // Local pin_id is newer (higher) than remote → local wins.
        let role = resolve_role(
            Some("019c6d5a-0000-7000-8000-000000000002"),
            SeedBankRole::Primary,
            Some(("stone-zzz", Some("019c6d5a-0000-7000-8000-000000000001"))),
            "stone-aaa",
        );
        assert_eq!(role, SeedBankRole::Primary);
    }

    #[test]
    fn test_resolve_role_remote_pinned_newer_wins() {
        // Remote pin_id is newer (higher) than local → remote wins.
        let role = resolve_role(
            Some("019c6d5a-0000-7000-8000-000000000001"),
            SeedBankRole::Primary,
            Some(("stone-zzz", Some("019c6d5a-0000-7000-8000-000000000002"))),
            "stone-aaa",
        );
        assert_eq!(role, SeedBankRole::Dormant);
    }

    #[test]
    fn test_resolve_role_only_local_pinned() {
        // Only local is pinned, remote is not → local wins.
        let role = resolve_role(
            Some("019c6d5a-0000-7000-8000-000000000001"),
            SeedBankRole::Primary,
            Some(("stone-zzz", None)),
            "stone-aaa",
        );
        assert_eq!(role, SeedBankRole::Primary);
    }

    #[test]
    fn test_resolve_role_only_remote_pinned() {
        // Remote is pinned, we are not → yield.
        let role = resolve_role(
            None,
            SeedBankRole::Primary,
            Some(("stone-aaa", Some("019c6d5a-0000-7000-8000-000000000001"))),
            "stone-zzz",
        );
        assert_eq!(role, SeedBankRole::Dormant);
    }

    #[test]
    fn test_resolve_role_lower_stone_id_yields() {
        // Neither pinned, lower stone_id yields.
        let role = resolve_role(
            None,
            SeedBankRole::Primary,
            Some(("stone-zzz", None)),
            "stone-aaa",
        );
        assert_eq!(role, SeedBankRole::Dormant);
    }

    #[test]
    fn test_resolve_role_higher_stone_id_keeps() {
        // Neither pinned, higher stone_id keeps Primary.
        let role = resolve_role(
            None,
            SeedBankRole::Primary,
            Some(("stone-aaa", None)),
            "stone-zzz",
        );
        assert_eq!(role, SeedBankRole::Primary);
    }

    #[test]
    fn test_resolve_role_dormant_stays_dormant_unpinned() {
        // Already Dormant with remote Primary, neither pinned → stay Dormant.
        let role = resolve_role(
            None,
            SeedBankRole::Dormant,
            Some(("stone-aaa", None)),
            "stone-zzz",
        );
        assert_eq!(role, SeedBankRole::Dormant);
    }

    #[test]
    fn test_resolve_role_dormant_pinned_promotes() {
        // Dormant + locally pinned, remote not pinned → promote to Primary.
        // This is the "claim Primary from any replica" use case.
        let role = resolve_role(
            Some("019c6d5a-0000-7000-8000-000000000001"),
            SeedBankRole::Dormant,
            Some(("stone-aaa", None)),
            "stone-zzz",
        );
        assert_eq!(role, SeedBankRole::Primary);
    }

    #[test]
    fn test_resolve_role_dormant_pinned_newer_promotes() {
        // Dormant + locally pinned with newer pin_id → promote to Primary.
        let role = resolve_role(
            Some("019c6d5a-0000-7000-8000-000000000002"),
            SeedBankRole::Dormant,
            Some(("stone-aaa", Some("019c6d5a-0000-7000-8000-000000000001"))),
            "stone-zzz",
        );
        assert_eq!(role, SeedBankRole::Primary);
    }

    #[test]
    fn test_resolve_role_dormant_pinned_older_stays() {
        // Dormant + locally pinned but older pin_id → stay Dormant.
        let role = resolve_role(
            Some("019c6d5a-0000-7000-8000-000000000001"),
            SeedBankRole::Dormant,
            Some(("stone-aaa", Some("019c6d5a-0000-7000-8000-000000000002"))),
            "stone-zzz",
        );
        assert_eq!(role, SeedBankRole::Dormant);
    }

    // -- build_cutoff_cursor --------------------------------------------------

    #[test]
    fn test_cutoff_cursor_format() {
        // Known timestamp: 2024-01-01T00:00:00Z = 1_704_067_200_000 ms
        let ts: u64 = 1_704_067_200_000;
        let cursor = build_cutoff_cursor(ts);

        // 48-bit timestamp split: upper 32 = ts >> 16, lower 16 = ts & 0xFFFF
        let upper = (ts >> 16) as u32;
        let lower = (ts & 0xFFFF) as u16;
        let expected = format!("{:08x}-{:04x}-7000-8000-000000000000", upper, lower);
        assert_eq!(cursor, expected);
    }

    #[test]
    fn test_cutoff_cursor_is_valid_uuid_length() {
        let cursor = build_cutoff_cursor(1_704_067_200_000);
        assert_eq!(cursor.len(), 36, "GUIDv7 string must be 36 chars");
        assert_eq!(&cursor[14..15], "7", "version nibble must be 7");
    }

    #[test]
    fn test_cutoff_cursor_ordering() {
        // Earlier timestamp < later timestamp in string comparison.
        let earlier = build_cutoff_cursor(1_000_000_000_000);
        let later = build_cutoff_cursor(1_704_067_200_000);
        assert!(earlier < later, "earlier cursor must sort before later");
    }

    #[test]
    fn test_cutoff_cursor_zero() {
        let cursor = build_cutoff_cursor(0);
        assert_eq!(cursor, "00000000-0000-7000-8000-000000000000");
    }

    // -- find_remote_primary_with_pin -----------------------------------------

    fn make_beacon(
        stone_id: &str,
        banks: Vec<(&str, &str, SeedBankRole, Option<&str>)>,
    ) -> StorageBeacon {
        StorageBeacon {
            stone_id: stone_id.to_string(),
            stone_name: format!("{}-name", stone_id),
            endpoint: format!("http://{}.local:7185", stone_id),
            seed_banks: banks
                .into_iter()
                .map(|(id, name, role, pin_id)| SeedBankAnnouncement {
                    id: id.to_string(),
                    name: name.to_string(),
                    role,
                    protocols: vec!["storage".to_string()],
                    access: StorageAccess::Direct,
                    visibility: "open".to_string(),
                    health: "healthy".to_string(),
                    capacity_bytes: 1_000_000_000,
                    used_bytes: 0,
                    encrypted: false,
                    pin_id: pin_id.map(|s| s.to_string()),
                })
                .collect(),
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_find_remote_primary_returns_none_when_empty() {
        let cache = StorageCacheInner::default();
        assert!(find_remote_primary_with_pin(&cache, "mybank", "stone-1").is_none());
    }

    #[test]
    fn test_find_remote_primary_skips_own_stone() {
        let mut cache = StorageCacheInner::default();
        cache.update(make_beacon(
            "stone-1",
            vec![("sb-1", "mybank", SeedBankRole::Primary, None)],
        ));
        // Searching with own stone_id → None
        assert!(find_remote_primary_with_pin(&cache, "mybank", "stone-1").is_none());
    }

    #[test]
    fn test_find_remote_primary_finds_remote() {
        let mut cache = StorageCacheInner::default();
        cache.update(make_beacon(
            "stone-1",
            vec![("sb-1", "mybank", SeedBankRole::Primary, None)],
        ));
        cache.update(make_beacon(
            "stone-2",
            vec![("sb-2", "mybank", SeedBankRole::Primary, None)],
        ));

        let result = find_remote_primary_with_pin(&cache, "mybank", "stone-1");
        assert!(result.is_some());
        let (stone_id, sb_id, pin_id) = result.unwrap();
        assert_eq!(stone_id, "stone-2");
        assert_eq!(sb_id, "sb-2");
        assert!(pin_id.is_none());
    }

    #[test]
    fn test_find_remote_primary_returns_pin_id() {
        let mut cache = StorageCacheInner::default();
        let pid = "019c6d5a-0000-7000-8000-000000000001";
        cache.update(make_beacon(
            "stone-2",
            vec![("sb-2", "mybank", SeedBankRole::Primary, Some(pid))],
        ));

        let result = find_remote_primary_with_pin(&cache, "mybank", "stone-1");
        assert!(result.is_some());
        assert_eq!(result.unwrap().2.as_deref(), Some(pid));
    }

    #[test]
    fn test_find_remote_primary_ignores_dormant() {
        let mut cache = StorageCacheInner::default();
        cache.update(make_beacon(
            "stone-2",
            vec![("sb-2", "mybank", SeedBankRole::Dormant, None)],
        ));

        assert!(find_remote_primary_with_pin(&cache, "mybank", "stone-1").is_none());
    }

    #[test]
    fn test_find_remote_primary_wrong_name() {
        let mut cache = StorageCacheInner::default();
        cache.update(make_beacon(
            "stone-2",
            vec![("sb-2", "other-bank", SeedBankRole::Primary, None)],
        ));

        assert!(find_remote_primary_with_pin(&cache, "mybank", "stone-1").is_none());
    }
}
