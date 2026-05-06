//! Offering-set projection (ARCH-0038 §`/api/v1/sets/offerings`).
//!
//! Projects elected-offering replication groups out of the
//! [`GardenRegistry`](crate::domain::tool::registry) hotcache.
//! Membership rule per ADR §"Membership rules":
//!
//! > `coordination = Elected` AND at least one running instance.
//!
//! Independent offerings (Ollama, proxies, stateless services) do not
//! appear here — they live on `/garden/services` / `/stone/services`.
//!
//! Role per member is read from `GardenTool.service.role`, populated by
//! the projection at [`crate::domain::tool::projection`] from the
//! offering's `OrchestrationState.role` and propagated cross-stone via
//! tool beacons. See [ARCH-0038](../../../../../docs/decisions/ARCH-0038-logical-sets-as-first-class-surface.md)
//! Phase C for the data flow.

use std::collections::HashMap;

use axum::extract::{Path, State};
use garden_common::CoordinationMode;
use garden_common::tools::GardenTool;

use crate::domain::tool::registry::ToolQuery;
use serde::Serialize;

use crate::Moss;
use crate::api::ApiResult;
use crate::infra::api_helpers::not_found;

/// Single member of an offering set — one running instance on one stone.
#[derive(Debug, Serialize)]
pub struct OfferingSetMember {
    pub stone_id: String,
    pub stone_name: String,
    pub endpoint: String,
    /// Role string from `OfferingRole`'s `to_string()` —
    /// `"primary" | "replica" | "joining" | "degraded"`. `None` when
    /// the offering hasn't reported orchestration state yet.
    pub role: Option<String>,
    /// Service status: `"running"` | `"stopped"` | etc.
    pub status: String,
    /// Whether the service is ready for traffic.
    pub ready: bool,
}

/// List-level summary entry — what `GET /api/v1/sets/offerings` returns
/// per set. Omits `members[]` for brevity; clients wanting the full
/// shape call the per-FQN detail endpoint.
#[derive(Debug, Serialize)]
pub struct OfferingSetSummary {
    /// Set name — the FQN every member shares.
    pub name: String,
    /// Always `"elected"` here (membership rule excludes Independent).
    pub coordination: &'static str,
    /// Number of running instances across all stones.
    pub member_count: usize,
    /// Stone hosting the current Primary member, if any member's
    /// reported role is `"primary"`.
    pub primary_stone: Option<String>,
}

/// Detail response — full member list for one set.
#[derive(Debug, Serialize)]
pub struct OfferingSetDetail {
    pub name: String,
    pub coordination: &'static str,
    pub primary_stone: Option<String>,
    /// URI template for connecting to the set, if the offering's
    /// gateway publishes one (e.g. mongo's
    /// `"mongodb://{host}:{port}/?replicaSet=zen-garden"`). Sourced
    /// from any member with a non-empty `uri_template`; all members
    /// of a set should agree.
    pub uri_template: Option<String>,
    /// Connection URIs for every member, in member order.
    pub connection_uris: Vec<String>,
    pub members: Vec<OfferingSetMember>,
}

/// `GET /api/v1/sets/offerings`
///
/// All elected-offering sets in the garden. Independent offerings are
/// excluded — see ADR §"Membership rules". The list is keyed by FQN
/// (`GardenTool.fqid`); same-FQN entries across stones merge into one
/// set entry.
pub async fn list_offering_sets(
    State(state): State<Moss>,
) -> ApiResult<Vec<OfferingSetSummary>> {
    let groups = collect_offering_groups(&state).await;

    let mut summaries: Vec<OfferingSetSummary> = groups
        .into_iter()
        .map(|(name, members)| {
            let primary_stone = primary_stone_of(&members);
            OfferingSetSummary {
                name,
                coordination: "elected",
                member_count: members.len(),
                primary_stone,
            }
        })
        .collect();
    summaries.sort_by(|a, b| a.name.cmp(&b.name));
    crate::api::ok(summaries)
}

/// `GET /api/v1/sets/offerings/{fqn}`
///
/// Detailed member list for a single elected-offering set. 404 when no
/// running members exist or the offering's coordination isn't Elected.
pub async fn get_offering_set(
    State(state): State<Moss>,
    Path(fqn): Path<String>,
) -> ApiResult<OfferingSetDetail> {
    let groups = collect_offering_groups(&state).await;

    let Some(members) = groups.get(&fqn).cloned() else {
        return Err(not_found(
            "OFFERING_SET_NOT_FOUND",
            format!(
                "No elected-offering set named '{fqn}' in this garden — \
                 either the offering isn't running anywhere, or its \
                 coordination is Independent"
            ),
        ));
    };

    let primary_stone = primary_stone_of(&members);
    let uri_template = members
        .iter()
        .find_map(|m| m.uri_template.clone());
    let connection_uris: Vec<String> = members
        .iter()
        .flat_map(|m| m.uris.iter().cloned())
        .collect();
    let detail_members: Vec<OfferingSetMember> = members
        .into_iter()
        .map(|m| OfferingSetMember {
            stone_id: m.stone_id,
            stone_name: m.stone_name,
            endpoint: m.endpoint,
            role: m.role,
            status: m.status,
            ready: m.ready,
        })
        .collect();

    crate::api::ok(OfferingSetDetail {
        name: fqn,
        coordination: "elected",
        primary_stone,
        uri_template,
        connection_uris,
        members: detail_members,
    })
}

/// Internal member shape carrying the per-stone fields the summary and
/// detail responses both project from. Avoids walking the registry twice.
#[derive(Clone)]
struct InternalMember {
    stone_id: String,
    stone_name: String,
    endpoint: String,
    role: Option<String>,
    status: String,
    ready: bool,
    uri_template: Option<String>,
    uris: Vec<String>,
}

/// Walk the registry, group offerings by FQN, apply the membership
/// rule. Returns FQN → members for every set that survives the rule.
///
/// Per ADR Phase C the rule is: `coordination = Elected` AND the
/// instance is running. The coordination lookup uses the bare
/// offering type (`tool.tool.tool_type`) because the catalog is
/// keyed by offering name, not by FQN.
///
/// We deliberately do NOT pre-filter by `ToolQuery.category` — in the
/// current registry layout, offering tools carry the manifest's own
/// `category` string (e.g. `"memcached"`, `"vector"`) rather than the
/// literal `"offering"`, so a category filter would silently drop
/// every offering. The `get_compiled` lookup is itself a sufficient
/// gate: storage entries have no catalog row, orchestrator entries
/// resolve to `Independent`, only opted-in elected offerings come
/// through with `Some(Elected)`.
async fn collect_offering_groups(state: &Moss) -> HashMap<String, Vec<InternalMember>> {
    let (_cursor, tools) = state.tool.snapshot(&ToolQuery::default()).await;

    // Cache coordination lookups by tool_type — many tools of the same
    // type avoid repeated `get_compiled` calls.
    let mut coordination_cache: HashMap<String, Option<CoordinationMode>> = HashMap::new();
    let mut groups: HashMap<String, Vec<InternalMember>> = HashMap::new();

    for tool in tools {
        // Storage entries are projected as `/sets/banks`, not here.
        // Skip them defensively even though their catalog lookup
        // would also miss.
        if tool.storage.is_some() {
            continue;
        }

        let tool_type = tool.tool.tool_type.clone();
        let coord = match coordination_cache.get(&tool_type) {
            Some(c) => c.clone(),
            None => {
                let resolved = state
                    .catalog
                    .get_compiled(&tool_type)
                    .await
                    .map(|c| c.coordination);
                coordination_cache.insert(tool_type.clone(), resolved.clone());
                resolved
            }
        };

        // Membership rule: only Elected coordination + at least one
        // running instance.
        if coord != Some(CoordinationMode::Elected) {
            continue;
        }
        if tool.service.status != garden_common::constants::SERVICE_RUNNING {
            continue;
        }

        let key = tool.fqid.clone();
        groups
            .entry(key)
            .or_default()
            .push(internal_member_from(tool));
    }

    groups
}

fn internal_member_from(tool: GardenTool) -> InternalMember {
    InternalMember {
        stone_id: tool.stone.id,
        stone_name: tool.stone.name,
        endpoint: tool.stone.endpoint,
        role: tool.service.role,
        status: tool.service.status,
        ready: tool.service.ready,
        uri_template: tool.service.uri_template,
        uris: tool.service.uris,
    }
}

/// Find the stone whose member's role is `"primary"`. Returns `None`
/// when no member has a primary role yet (election in progress, or
/// every member reports Joining).
fn primary_stone_of(members: &[InternalMember]) -> Option<String> {
    members
        .iter()
        .find(|m| m.role.as_deref() == Some("primary"))
        .map(|m| m.stone_name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(stone: &str, role: Option<&str>) -> InternalMember {
        InternalMember {
            stone_id: format!("{stone}-id"),
            stone_name: stone.to_string(),
            endpoint: format!("http://{stone}:7185"),
            role: role.map(String::from),
            status: "running".to_string(),
            ready: true,
            uri_template: None,
            uris: vec![],
        }
    }

    #[test]
    fn primary_stone_picks_member_with_primary_role() {
        let members = vec![
            member("stone-a", Some("replica")),
            member("stone-b", Some("primary")),
            member("stone-c", Some("replica")),
        ];
        assert_eq!(
            primary_stone_of(&members),
            Some("stone-b".to_string())
        );
    }

    #[test]
    fn primary_stone_returns_none_when_no_primary_yet() {
        // Election in progress — every member is Joining.
        let members = vec![
            member("stone-a", Some("joining")),
            member("stone-b", Some("joining")),
        ];
        assert!(primary_stone_of(&members).is_none());
    }

    #[test]
    fn primary_stone_returns_none_when_role_unreported() {
        // Pre-orchestration state.
        let members = vec![member("stone-a", None), member("stone-b", None)];
        assert!(primary_stone_of(&members).is_none());
    }

    #[test]
    fn primary_stone_ignores_degraded() {
        // A degraded former-primary doesn't satisfy the primary lookup;
        // the orchestration loop will surface a new Primary on next tick.
        let members = vec![
            member("stone-a", Some("degraded")),
            member("stone-b", Some("replica")),
        ];
        assert!(primary_stone_of(&members).is_none());
    }

    // Membership-rule tests live in `collect_offering_groups`'s integration
    // surface — driving them needs a populated `Moss` and `Catalog`, which
    // pulls in too much setup for a unit test. Those are covered by the
    // Phase F integration test under `tests/sets_integration.rs`.

    #[test]
    fn summary_serialises_to_canonical_wire_shape() {
        // Pin the response shape so accidental field renames or
        // additions show up as a test failure. ADR §"Wire format
        // examples" documents this as the contract.
        let summary = OfferingSetSummary {
            name: "mongodb::prd".to_string(),
            coordination: "elected",
            member_count: 2,
            primary_stone: Some("stone-crystal-forest".to_string()),
        };
        let json = serde_json::to_value(&summary).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "name": "mongodb::prd",
                "coordination": "elected",
                "member_count": 2,
                "primary_stone": "stone-crystal-forest"
            })
        );
    }

    #[test]
    fn detail_serialises_to_canonical_wire_shape() {
        let detail = OfferingSetDetail {
            name: "mongodb::prd".to_string(),
            coordination: "elected",
            primary_stone: Some("stone-crystal-forest".to_string()),
            uri_template: Some(
                "mongodb://{host}:{port}/?replicaSet=zen-garden".to_string(),
            ),
            connection_uris: vec![
                "mongodb://stone-crystal-forest.local:27017".to_string(),
            ],
            members: vec![OfferingSetMember {
                stone_id: "0193abc".to_string(),
                stone_name: "stone-crystal-forest".to_string(),
                endpoint: "http://stone-crystal-forest.local:7185".to_string(),
                role: Some("primary".to_string()),
                status: "running".to_string(),
                ready: true,
            }],
        };
        let json = serde_json::to_value(&detail).unwrap();
        // Spot-check the contract-load-bearing fields.
        assert_eq!(json["coordination"], "elected");
        assert_eq!(json["primary_stone"], "stone-crystal-forest");
        assert_eq!(json["members"][0]["role"], "primary");
        assert_eq!(json["members"][0]["status"], "running");
        assert_eq!(json["members"][0]["ready"], true);
    }
}
