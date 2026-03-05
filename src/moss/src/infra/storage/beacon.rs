//! Storage beacon broadcasting
//!
//! Broadcasts STORAGE_BEACON announcements to notify other stones
//! about this stone's storage capabilities.
//!
//! See docs/decisions/STORAGE-0003-beacon-protocol.md

use anyhow::{Context, Result};
use chrono::Utc;
use garden_common::infra::communications::{announcement_types, p2p};
use garden_common::storage::{SeedBankAnnouncement, SeedBankRole, StorageBeacon};
use std::collections::HashMap;
use tracing::{debug, info, warn};

use super::SeedBankRegistry;

/// Build a storage beacon for this stone.
///
/// `roles` maps seed bank name (FQN) → runtime role. When provided, each
/// announcement's role is stamped accordingly. When absent, all roles
/// default to Primary (backward compat).
///
/// `pins` is the set of seed bank names whose Primary role is pinned.
pub async fn build_beacon(
    stone_id: &str,
    stone_name: &str,
    endpoint: &str,
    roles: Option<&HashMap<String, SeedBankRole>>,
    pins: Option<&HashMap<String, String>>,
) -> Result<StorageBeacon> {
    // Scan current seed banks
    let registry = SeedBankRegistry::scan()
        .await
        .context("Failed to scan seed banks")?;

    let seed_banks: Vec<SeedBankAnnouncement> = registry
        .list()
        .iter()
        .map(|info| {
            let mut ann = SeedBankAnnouncement::from_info(info);
            stamp_announcement(&mut ann, &info.name, roles, pins);
            ann
        })
        .collect();

    Ok(StorageBeacon {
        stone_id: stone_id.to_string(),
        stone_name: stone_name.to_string(),
        endpoint: endpoint.to_string(),
        seed_banks,
        timestamp: Utc::now(),
    })
}

/// Broadcast a storage beacon to all stones
///
/// Called on:
/// - Seed bank mount/unmount
/// - Visibility change
/// - Stone online (triggered by STONE_CHIRP from new stone)
pub async fn broadcast_beacon(
    stone_id: &str,
    stone_name: &str,
    endpoint: &str,
    roles: Option<&HashMap<String, SeedBankRole>>,
    pins: Option<&HashMap<String, String>>,
) -> Result<()> {
    let beacon = build_beacon(stone_id, stone_name, endpoint, roles, pins).await?;

    let seed_bank_count = beacon.seed_banks.len();

    // Skip broadcast if no storage capability (reduces noise)
    // Exception: Always broadcast if called explicitly (e.g., mount → empty after unmount)
    debug!(
        stone = %stone_name,
        seed_banks = seed_bank_count,
        "Broadcasting storage beacon"
    );

    p2p::send_announcement(announcement_types::STORAGE_BEACON, &beacon)
        .await
        .context("Failed to send STORAGE_BEACON")?;

    info!(
        stone = %stone_name,
        seed_banks = seed_bank_count,
        "Storage beacon broadcast complete"
    );

    Ok(())
}

/// Broadcast beacon if this stone has storage
///
/// Use this when responding to a new stone coming online.
/// Only broadcasts if local stone has seed banks.
pub async fn broadcast_if_has_storage(
    stone_id: &str,
    stone_name: &str,
    endpoint: &str,
    roles: Option<&HashMap<String, SeedBankRole>>,
    pins: Option<&HashMap<String, String>>,
) -> Result<bool> {
    // Quick check if we have any storage
    let registry = match SeedBankRegistry::scan().await {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, "Failed to scan seed banks for beacon check");
            return Ok(false);
        }
    };

    if registry.list().is_empty() {
        debug!(stone = %stone_name, "No seed banks, skipping beacon");
        return Ok(false);
    }

    // We have storage, broadcast beacon
    broadcast_beacon(stone_id, stone_name, endpoint, roles, pins).await?;
    Ok(true)
}

/// Update local storage cache with this stone's storage
///
/// Called at startup and after mount/unmount events to ensure storage_cache
/// reflects the local stone's storage capabilities. This makes storage_cache
// TOOLS-0003: update_local_storage_cache and update_and_broadcast removed.
// Local storage is now projected into the registry via refresh_local_tools_projection().
// Network broadcast is done via broadcast_beacon() directly.

/// Stamp role and pin_id onto a seed bank announcement.
///
/// Pure helper extracted from `build_beacon()` for testability (STORAGE-0006).
fn stamp_announcement(
    ann: &mut SeedBankAnnouncement,
    name: &str,
    roles: Option<&HashMap<String, SeedBankRole>>,
    pins: Option<&HashMap<String, String>>,
) {
    if let Some(r) = roles.and_then(|m| m.get(name)) {
        ann.role = *r;
    }
    if let Some(p) = pins {
        ann.pin_id = p.get(name).cloned();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use garden_common::storage::StorageAccess;

    fn make_announcement(name: &str) -> SeedBankAnnouncement {
        SeedBankAnnouncement {
            id: format!("sb-{}", name),
            name: name.to_string(),
            role: SeedBankRole::default(),
            protocols: vec!["storage".to_string()],
            access: StorageAccess::Direct,
            visibility: "open".to_string(),
            health: "healthy".to_string(),
            capacity_bytes: 1_000_000_000,
            used_bytes: 0,
            encrypted: false,
            pin_id: None,
        }
    }

    #[test]
    fn test_beacon_structure() {
        // Just verify the beacon types are correctly importable
        let beacon = StorageBeacon::empty("test-id", "test-name", "http://test:7185");
        assert!(!beacon.has_storage());
        assert!(!beacon.supports_s3());
    }

    #[test]
    fn test_stamp_announcement_role_only() {
        let mut ann = make_announcement("mybank");
        let mut roles = HashMap::new();
        roles.insert("mybank".to_string(), SeedBankRole::Dormant);

        stamp_announcement(&mut ann, "mybank", Some(&roles), None);

        assert_eq!(ann.role, SeedBankRole::Dormant);
        assert!(ann.pin_id.is_none(), "pin_id should remain None when pins is None");
    }

    #[test]
    fn test_stamp_announcement_pin_only() {
        let mut ann = make_announcement("mybank");
        let mut pins = HashMap::new();
        pins.insert("mybank".to_string(), "019c6d5a-0000-7000-8000-000000000001".to_string());

        stamp_announcement(&mut ann, "mybank", None, Some(&pins));

        assert_eq!(
            ann.role,
            SeedBankRole::Primary,
            "role should remain default"
        );
        assert_eq!(ann.pin_id.as_deref(), Some("019c6d5a-0000-7000-8000-000000000001"));
    }

    #[test]
    fn test_stamp_announcement_both() {
        let mut ann = make_announcement("mybank");
        let mut roles = HashMap::new();
        roles.insert("mybank".to_string(), SeedBankRole::Dormant);
        let mut pins = HashMap::new();
        pins.insert("mybank".to_string(), "019c6d5a-0000-7000-8000-000000000001".to_string());

        stamp_announcement(&mut ann, "mybank", Some(&roles), Some(&pins));

        assert_eq!(ann.role, SeedBankRole::Dormant);
        assert!(ann.pin_id.is_some());
    }

    #[test]
    fn test_stamp_announcement_neither() {
        let mut ann = make_announcement("mybank");

        stamp_announcement(&mut ann, "mybank", None, None);

        assert_eq!(ann.role, SeedBankRole::Primary, "role default preserved");
        assert!(ann.pin_id.is_none(), "pin_id default preserved");
    }

    #[test]
    fn test_stamp_announcement_name_not_in_pins() {
        let mut ann = make_announcement("mybank");
        let mut pins = HashMap::new();
        pins.insert("other-bank".to_string(), "019c6d5a-0000-7000-8000-000000000001".to_string());

        stamp_announcement(&mut ann, "mybank", None, Some(&pins));

        assert!(ann.pin_id.is_none(), "pin_id should be None when name not in map");
    }

    #[test]
    fn test_stamp_announcement_name_not_in_roles() {
        let mut ann = make_announcement("mybank");
        let mut roles = HashMap::new();
        roles.insert("other-bank".to_string(), SeedBankRole::Dormant);

        stamp_announcement(&mut ann, "mybank", Some(&roles), None);

        assert_eq!(
            ann.role,
            SeedBankRole::Primary,
            "role should stay default when name not in roles map"
        );
    }
}
