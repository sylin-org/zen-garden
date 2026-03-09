//! Storage beacon broadcasting (STORAGE-0011)
//!
//! Broadcasts STORAGE_BEACON announcements to notify other stones
//! about this stone's storage capabilities.
//!
//! See docs/decisions/STORAGE-0003-beacon-protocol.md
//!
//! STORAGE-0011: reads from the unified `Volumes` collection instead of
//! scanning the filesystem via `StorageRegistry`.

use anyhow::{Context, Result};
use chrono::Utc;
use garden_common::infra::communications::{announcement_types, p2p};
use garden_common::storage::{StorageAnnouncement, StorageBeacon, StorageRole};
use std::collections::HashMap;
use tracing::{debug, info};

use crate::domain::storage::Volumes;

/// Build a storage beacon for this stone.
///
/// Reads managed volumes from the unified `Volumes` collection.
/// `roles` and `pins` maps override the defaults when provided.
pub async fn build_beacon(
    stone_id: &str,
    stone_name: &str,
    endpoint: &str,
    volumes: &Volumes,
    roles: Option<&HashMap<String, StorageRole>>,
    pins: Option<&HashMap<String, String>>,
) -> Result<StorageBeacon> {
    let map = volumes.read().await;

    let storages: Vec<StorageAnnouncement> = map
        .values()
        .filter_map(|vol| {
            let info = vol.to_storage_info()?;
            let mut ann = StorageAnnouncement::from_info(&info);
            stamp_announcement(&mut ann, &info.name, roles, pins);
            Some(ann)
        })
        .collect();

    Ok(StorageBeacon {
        stone_id: stone_id.to_string(),
        stone_name: stone_name.to_string(),
        endpoint: endpoint.to_string(),
        storages,
        timestamp: Utc::now(),
    })
}

/// Broadcast a storage beacon to all stones.
///
/// Called on:
/// - Volume mount/unmount
/// - Visibility change
/// - Stone online (triggered by STONE_CHIRP from new stone)
pub async fn broadcast_beacon(
    stone_id: &str,
    stone_name: &str,
    endpoint: &str,
    volumes: &Volumes,
    roles: Option<&HashMap<String, StorageRole>>,
    pins: Option<&HashMap<String, String>>,
) -> Result<()> {
    let beacon = build_beacon(stone_id, stone_name, endpoint, volumes, roles, pins).await?;

    let storage_count = beacon.storages.len();

    debug!(
        stone = %stone_name,
        storages = storage_count,
        "Broadcasting storage beacon"
    );

    p2p::send_announcement(announcement_types::STORAGE_BEACON, &beacon)
        .await
        .context("Failed to send STORAGE_BEACON")?;

    info!(
        stone = %stone_name,
        storages = storage_count,
        "Storage beacon broadcast complete"
    );

    Ok(())
}

/// Broadcast beacon if this stone has managed storage.
///
/// Use this when responding to a new stone coming online.
/// Only broadcasts if local stone has managed volumes.
pub async fn broadcast_if_has_storage(
    stone_id: &str,
    stone_name: &str,
    endpoint: &str,
    volumes: &Volumes,
    roles: Option<&HashMap<String, StorageRole>>,
    pins: Option<&HashMap<String, String>>,
) -> Result<bool> {
    let has_managed = {
        let map = volumes.read().await;
        map.values().any(|v| v.is_managed())
    };

    if !has_managed {
        debug!(stone = %stone_name, "No managed storage, skipping beacon");
        return Ok(false);
    }

    broadcast_beacon(stone_id, stone_name, endpoint, volumes, roles, pins).await?;
    Ok(true)
}

/// Stamp role and pin_id onto a storage announcement.
fn stamp_announcement(
    ann: &mut StorageAnnouncement,
    name: &str,
    roles: Option<&HashMap<String, StorageRole>>,
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

    fn make_announcement(name: &str) -> StorageAnnouncement {
        StorageAnnouncement {
            id: format!("sb-{}", name),
            name: name.to_string(),
            role: StorageRole::default(),
            protocols: vec!["storage".to_string()],
            access: StorageAccess::Direct,
            visibility: "open".to_string(),
            health: "healthy".to_string(),
            capacity_bytes: 1_000_000_000,
            used_bytes: 0,
            encrypted: false,
            pin_id: None,
            roles: vec![garden_common::storage::ROLE_SEED_BANK.to_string()],
        }
    }

    #[test]
    fn test_beacon_structure() {
        let beacon = StorageBeacon::empty("test-id", "test-name", "http://test:7185");
        assert!(!beacon.has_storage());
        assert!(!beacon.supports_s3());
    }

    #[test]
    fn test_stamp_announcement_role_only() {
        let mut ann = make_announcement("mybank");
        let mut roles = HashMap::new();
        roles.insert("mybank".to_string(), StorageRole::Dormant);

        stamp_announcement(&mut ann, "mybank", Some(&roles), None);

        assert_eq!(ann.role, StorageRole::Dormant);
        assert!(ann.pin_id.is_none());
    }

    #[test]
    fn test_stamp_announcement_pin_only() {
        let mut ann = make_announcement("mybank");
        let mut pins = HashMap::new();
        pins.insert(
            "mybank".to_string(),
            "019c6d5a-0000-7000-8000-000000000001".to_string(),
        );

        stamp_announcement(&mut ann, "mybank", None, Some(&pins));

        assert_eq!(ann.role, StorageRole::Primary);
        assert_eq!(
            ann.pin_id.as_deref(),
            Some("019c6d5a-0000-7000-8000-000000000001")
        );
    }

    #[test]
    fn test_stamp_announcement_both() {
        let mut ann = make_announcement("mybank");
        let mut roles = HashMap::new();
        roles.insert("mybank".to_string(), StorageRole::Dormant);
        let mut pins = HashMap::new();
        pins.insert(
            "mybank".to_string(),
            "019c6d5a-0000-7000-8000-000000000001".to_string(),
        );

        stamp_announcement(&mut ann, "mybank", Some(&roles), Some(&pins));

        assert_eq!(ann.role, StorageRole::Dormant);
        assert!(ann.pin_id.is_some());
    }

    #[test]
    fn test_stamp_announcement_neither() {
        let mut ann = make_announcement("mybank");
        stamp_announcement(&mut ann, "mybank", None, None);
        assert_eq!(ann.role, StorageRole::Primary);
        assert!(ann.pin_id.is_none());
    }

    #[test]
    fn test_stamp_announcement_name_not_in_maps() {
        let mut ann = make_announcement("mybank");
        let mut pins = HashMap::new();
        pins.insert(
            "other-bank".to_string(),
            "019c6d5a-0000-7000-8000-000000000001".to_string(),
        );
        let mut roles = HashMap::new();
        roles.insert("other-bank".to_string(), StorageRole::Dormant);

        stamp_announcement(&mut ann, "mybank", Some(&roles), Some(&pins));

        assert_eq!(ann.role, StorageRole::Primary);
        assert!(ann.pin_id.is_none());
    }
}
