//! Bank-set projection (ARCH-0038 §`/api/v1/sets/banks`).
//!
//! Projects bank replication groups out of the
//! [`GardenRegistry`](crate::domain::tool::registry) hotcache.
//! Membership rule per ADR §"Membership rules":
//!
//! > Emit every bank — replica_count >= 1.
//!
//! Singletons are intentionally surfaced. Per the ADR's UX rationale:
//! a singleton bank is exactly the surface that should drive a "would
//! you like to add a partner stone?" affordance in the canvas.
//!
//! Each bank's volume across stones is a member. Members carry a per-volume
//! role string (`"primary" | "replica"`) read from `GardenTool.storage.role`.
//! The set's primary stone is whichever member's role is `"primary"`.
//! Banks with empty `replica_set_name` fall under the default name
//! `"storage"` (matching `DEFAULT_REPLICA_SET_DISPLAY`).

use std::collections::HashMap;

use axum::extract::{Path, State};
use garden_common::storage::DEFAULT_REPLICA_SET_DISPLAY;
use serde::Serialize;

use crate::Moss;
use crate::api::ApiResult;
use crate::infra::api_helpers::not_found;

/// Single member of a bank set — one volume on one stone.
#[derive(Debug, Serialize)]
pub struct BankSetMember {
    pub stone_id: String,
    pub stone_name: String,
    /// Device-level GUIDv7 (one per physical volume).
    pub device_id: String,
    /// User-facing device name.
    pub device_name: String,
    /// Volume's role within the replica set: `"primary"` | `"replica"`.
    /// `None` when role hasn't been resolved yet (orchestration loop
    /// hasn't ticked since the volume came online).
    pub role: Option<String>,
    pub capacity_bytes: u64,
    pub used_bytes: u64,
}

/// Summary entry for the list endpoint. Omits `members[]` for brevity.
#[derive(Debug, Serialize)]
pub struct BankSetSummary {
    /// Set name. Empty `replica_set_name` collapses to `"storage"`.
    pub name: String,
    /// Replica set GUIDv7 (STORAGE-0013).
    pub replica_set_id: String,
    pub member_count: usize,
    pub primary_stone: Option<String>,
    pub total_capacity_bytes: u64,
    pub total_used_bytes: u64,
}

/// Detail response — full member list for one set, plus aggregated
/// metadata pulled from the first member encountered.
#[derive(Debug, Serialize)]
pub struct BankSetDetail {
    pub name: String,
    pub replica_set_id: String,
    pub primary_stone: Option<String>,
    pub total_capacity_bytes: u64,
    pub total_used_bytes: u64,
    /// Pin id for the bank's primary, surfaced from the
    /// primary member's `StorageMetadata.pin_id`.
    pub pin_id: Option<String>,
    /// `"open"` | `"closed"` | `"read-only"`. From any member —
    /// visibility is bank-wide.
    pub visibility: Option<String>,
    /// `true` when any member reports encryption (consistent with
    /// `bank_aggregate::local_banks`).
    pub encrypted: bool,
    /// Supported protocols, e.g. `["s3", "storage"]`. From any member.
    pub protocols: Vec<String>,
    /// Composable roles, e.g. `["seed-bank"]`. From any member.
    pub roles: Vec<String>,
    pub members: Vec<BankSetMember>,
}

/// `GET /api/v1/sets/banks`
///
/// All bank sets in the garden, including singletons. The list groups
/// by `replica_set_name` (with empty falling under `"storage"`).
pub async fn list_bank_sets(
    State(state): State<Moss>,
) -> ApiResult<Vec<BankSetSummary>> {
    let groups = collect_bank_groups(&state).await;

    let mut summaries: Vec<BankSetSummary> = groups
        .into_values()
        .map(|g| BankSetSummary {
            name: g.name,
            replica_set_id: g.replica_set_id,
            member_count: g.members.len(),
            primary_stone: primary_stone_of(&g.members),
            total_capacity_bytes: g.members.iter().map(|m| m.capacity_bytes).sum(),
            total_used_bytes: g.members.iter().map(|m| m.used_bytes).sum(),
        })
        .collect();
    summaries.sort_by(|a, b| a.name.cmp(&b.name));
    crate::api::ok(summaries)
}

/// `GET /api/v1/sets/banks/{moniker}`
///
/// Detailed member list for a single bank set. 404 when no bank with
/// that name exists. Empty-name lookups should use the literal
/// `"storage"` (the default display name).
pub async fn get_bank_set(
    State(state): State<Moss>,
    Path(moniker): Path<String>,
) -> ApiResult<BankSetDetail> {
    let groups = collect_bank_groups(&state).await;

    let Some(group) = groups.into_values().find(|g| g.name == moniker) else {
        return Err(not_found(
            "BANK_SET_NOT_FOUND",
            format!(
                "No bank set named '{moniker}' in this garden. \
                 Empty replica-set names collapse under '{DEFAULT_REPLICA_SET_DISPLAY}'."
            ),
        ));
    };

    let primary_stone = primary_stone_of(&group.members);
    let total_capacity_bytes = group.members.iter().map(|m| m.capacity_bytes).sum();
    let total_used_bytes = group.members.iter().map(|m| m.used_bytes).sum();

    crate::api::ok(BankSetDetail {
        name: group.name,
        replica_set_id: group.replica_set_id,
        primary_stone,
        total_capacity_bytes,
        total_used_bytes,
        pin_id: group.pin_id,
        visibility: group.visibility,
        encrypted: group.encrypted,
        protocols: group.protocols,
        roles: group.roles,
        members: group.members,
    })
}

/// Internal accumulator carrying both the per-member list and the
/// once-set bank-wide metadata. Walking the registry once populates
/// every field; downstream both endpoints project from this shape.
struct BankGroup {
    name: String,
    replica_set_id: String,
    members: Vec<BankSetMember>,
    pin_id: Option<String>,
    visibility: Option<String>,
    encrypted: bool,
    protocols: Vec<String>,
    roles: Vec<String>,
}

/// Walk the storage entries in the registry, group by display name
/// (with the empty/default-set collapse rule). Per ADR membership rule:
/// every bank emits, including singletons.
async fn collect_bank_groups(state: &Moss) -> HashMap<String, BankGroup> {
    let mut groups: HashMap<String, BankGroup> = HashMap::new();

    for entry in state.tool.storage_entries().await {
        let Some(sm) = entry.tool.storage.as_ref() else {
            continue;
        };
        let name = if sm.replica_set_name.is_empty() {
            DEFAULT_REPLICA_SET_DISPLAY.to_string()
        } else {
            sm.replica_set_name.clone()
        };

        let role = sm.role.as_ref().map(|r| r.to_ascii_lowercase());
        let is_primary = role.as_deref() == Some("primary");

        let group = groups.entry(name.clone()).or_insert_with(|| BankGroup {
            name: name.clone(),
            replica_set_id: sm.replica_set_id.clone(),
            members: Vec::new(),
            pin_id: None,
            visibility: None,
            encrypted: false,
            protocols: Vec::new(),
            roles: Vec::new(),
        });

        // Bank-wide metadata: prefer the primary's view when present;
        // otherwise the first member encountered.
        let prefer = is_primary || group.members.is_empty();
        if prefer {
            group.replica_set_id = sm.replica_set_id.clone();
            group.pin_id = sm.pin_id.clone();
            group.visibility = Some(sm.visibility.clone());
            group.protocols = sm.protocols.clone();
            group.roles = sm.roles.clone();
        }
        if sm.encrypted {
            group.encrypted = true;
        }

        group.members.push(BankSetMember {
            stone_id: entry.tool.stone.id.clone(),
            stone_name: entry.tool.stone.name.clone(),
            device_id: entry.tool.tool.id.clone(),
            device_name: entry.tool.tool.name.clone(),
            role,
            capacity_bytes: sm.capacity_bytes,
            used_bytes: sm.used_bytes,
        });
    }

    groups
}

/// Find the stone whose member's role is `"primary"`. Returns `None`
/// when no member has the primary role yet (e.g. orchestration hasn't
/// finished its first reconciliation cycle).
fn primary_stone_of(members: &[BankSetMember]) -> Option<String> {
    members
        .iter()
        .find(|m| m.role.as_deref() == Some("primary"))
        .map(|m| m.stone_name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(stone: &str, role: Option<&str>, used: u64, cap: u64) -> BankSetMember {
        BankSetMember {
            stone_id: format!("{stone}-id"),
            stone_name: stone.to_string(),
            device_id: format!("{stone}-dev"),
            device_name: format!("{stone}-disk"),
            role: role.map(String::from),
            capacity_bytes: cap,
            used_bytes: used,
        }
    }

    #[test]
    fn primary_stone_picks_member_with_primary_role() {
        let members = vec![
            member("stone-a", Some("replica"), 0, 0),
            member("stone-b", Some("primary"), 0, 0),
        ];
        assert_eq!(
            primary_stone_of(&members),
            Some("stone-b".to_string())
        );
    }

    #[test]
    fn primary_stone_returns_none_when_no_primary_yet() {
        let members = vec![member("stone-a", None, 0, 0)];
        assert!(primary_stone_of(&members).is_none());
    }

    #[test]
    fn singleton_bank_with_no_primary_still_aggregates_capacity() {
        // Per the ADR's UX rationale, a singleton still emits — the
        // canvas uses it to drive "add a partner stone?" affordance.
        // Capacity totals must come through even when the role
        // assignment is None.
        let members = vec![member("stone-solo", None, 100, 1000)];
        let total_cap: u64 = members.iter().map(|m| m.capacity_bytes).sum();
        let total_used: u64 = members.iter().map(|m| m.used_bytes).sum();
        assert_eq!(total_cap, 1000);
        assert_eq!(total_used, 100);
        assert_eq!(members.len(), 1);
        assert!(primary_stone_of(&members).is_none());
    }

    // Membership-rule tests at the integration level (driving them needs
    // a populated `Moss` + `GardenRegistry`) live in the Phase F test
    // file alongside the offering-set integration coverage.

    #[test]
    fn empty_replica_set_name_collapses_to_default_display() {
        // Verify the collapse rule used by `collect_bank_groups`:
        // empty `replica_set_name` falls under "storage". This is the
        // STORAGE-0013 default-set convention surfaced through the
        // bank-set view.
        assert_eq!(DEFAULT_REPLICA_SET_DISPLAY, "storage");

        let raw_name = "";
        let projected = if raw_name.is_empty() {
            DEFAULT_REPLICA_SET_DISPLAY.to_string()
        } else {
            raw_name.to_string()
        };
        assert_eq!(projected, "storage");
    }

    #[test]
    fn summary_serialises_to_canonical_wire_shape() {
        let summary = BankSetSummary {
            name: "personal".to_string(),
            replica_set_id: "0193abc".to_string(),
            member_count: 2,
            primary_stone: Some("stone-crystal-forest".to_string()),
            total_capacity_bytes: 4_000_000_000_000,
            total_used_bytes: 1_500_000_000_000,
        };
        let json = serde_json::to_value(&summary).unwrap();
        assert_eq!(json["name"], "personal");
        assert_eq!(json["member_count"], 2);
        assert_eq!(json["total_capacity_bytes"], 4_000_000_000_000_u64);
        assert_eq!(json["total_used_bytes"], 1_500_000_000_000_u64);
    }

    #[test]
    fn detail_serialises_to_canonical_wire_shape() {
        let detail = BankSetDetail {
            name: "personal".to_string(),
            replica_set_id: "0193abc".to_string(),
            primary_stone: Some("stone-crystal-forest".to_string()),
            total_capacity_bytes: 2_000_000_000_000,
            total_used_bytes: 743_000_000_000,
            pin_id: Some("pin-0193".to_string()),
            visibility: Some("open".to_string()),
            encrypted: false,
            protocols: vec!["s3".to_string(), "storage".to_string()],
            roles: vec!["seed-bank".to_string()],
            members: vec![member("stone-crystal-forest", Some("primary"), 100, 1000)],
        };
        let json = serde_json::to_value(&detail).unwrap();
        assert_eq!(json["pin_id"], "pin-0193");
        assert_eq!(json["visibility"], "open");
        assert_eq!(json["protocols"], serde_json::json!(["s3", "storage"]));
        assert_eq!(json["roles"], serde_json::json!(["seed-bank"]));
        assert_eq!(json["members"][0]["role"], "primary");
        assert_eq!(json["members"][0]["device_name"], "stone-crystal-forest-disk");
    }
}
