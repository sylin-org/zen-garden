//! Tools projector — builds GardenTool snapshots from service discovery and storage.
//!
//! TOOLS-0002: Offerings are projected through `find_services()` (same path as
//! garden-rake find) to get gateway/orchestrator-aware resolution.
//! Seed-banks are projected directly from the seed bank lifecycle objects.

use crate::domain::service_discovery::{self, FoundService};
use crate::domain::storage::VolumeState;
use crate::AppState;
use garden_common::offerings::OfferingFqn;
use garden_common::tools::{Capability, GardenTool, ServiceInfo, Stone, ToolIdentity};
use std::collections::BTreeSet;

/// Project all local tools (offerings + seed-banks) as GardenTool instances.
///
/// Offerings go through the services path (gateway/orchestrator aware).
/// Seed-banks are projected directly from the storage beacon cache.
pub async fn project_local_tools(state: &AppState) -> Vec<GardenTool> {
    let mut tools = Vec::new();

    // ── Offerings via service discovery (TOOLS-0002) ─────────────
    let svc_response = service_discovery::list_all_local_services(state).await;
    for svc in svc_response.services {
        tools.push(found_service_to_garden_tool(svc));
    }

    // ── Gateway / orchestrator entries ───────────────────────────
    // Gateways are written directly to the registry with EntryOrigin::Registered
    // by the gateway API (PUT /api/v1/garden/gateway). They are NOT projected
    // here — the reconcile_local() call skips Registered entries.

    // ── Managed storages from unified volumes ────────────────────
    let endpoint = state.current.topology.self_entry.read().await.address.http_base();
    let managed_vols: Vec<_> = {
        let map = state.current.storage.volumes.read().await;
        map.values()
            .filter(|v| v.is_managed())
            .cloned()
            .collect()
    };

    for vol in &managed_vols {
        let mgmt = vol.management.as_ref().unwrap(); // safe: filtered above
        let (status, ready) = volume_state_to_readiness(&vol.state);
        let visibility_str = mgmt.visibility.to_string();

        // fqid = replica set display name (used for grouping replicas and Explorer folders).
        // Users see replica set names, not individual volume names.
        // The stable GUID lives in StorageMetadata.replica_set_id.
        let fqid = mgmt.display_name().to_string();

        // Local storages always support s3 + storage protocols
        let protocols = vec![garden_common::constants::PROTOCOL_S3.to_string(), garden_common::constants::PROTOCOL_STORAGE.to_string()];
        let protocol = garden_common::constants::PROTOCOL_S3.to_string();

        let mut uris = Vec::new();
        uris.push(format!(
            "{}/api/v1/storage",
            endpoint.trim_end_matches('/')
        ));
        uris.push(format!(
            "{}/api/v1/storage/s3",
            endpoint.trim_end_matches('/')
        ));
        uris = uris
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();

        tools.push(GardenTool {
            fqid,
            tool: ToolIdentity {
                name: mgmt.name.clone(),
                tool_type: garden_common::constants::TOOL_TYPE_SEED_BANK.to_string(),
                category: garden_common::constants::CATEGORY_STORAGE.to_string(),
                id: mgmt.id.clone(),
                tags: Vec::new(),
            },
            stone: Stone {
                id: state.current.stone.id.clone(),
                name: state.current.stone.name.clone(),
                endpoint: endpoint.clone(),
            },
            service: ServiceInfo {
                status: status.to_string(),
                ready,
                protocol,
                uris,
                hostname: None,
                ip: None,
                port: None,
                uri_template: None,
            },
            capabilities: Vec::new(),
            storage: Some(garden_common::tools::StorageMetadata {
                replica_set_id: mgmt.replica_set_id.clone(),
                replica_set_name: mgmt.replica_set_name.clone(),
                role: Some(mgmt.role.to_string().to_ascii_lowercase()),
                capacity_bytes: vol.capacity_bytes,
                used_bytes: vol.used_bytes,
                visibility: visibility_str,
                encrypted: mgmt.encrypted,
                pin_id: vol.pin_id().map(|s| s.to_string()),
                protocols,
                roles: mgmt.roles.clone(),
            }),
        });
    }

    tools
}

/// Convert a `FoundService` (from service discovery) into a `GardenTool`.
fn found_service_to_garden_tool(svc: FoundService) -> GardenTool {
    let fqn = parse_fqn_for_fqid(&svc.name, &svc.offering);
    let fqid = fqn.fqn();

    let capabilities: Vec<Capability> = svc
        .sub_capabilities
        .iter()
        .filter(|cap| !cap.cap_type.trim().is_empty())
        .map(|cap| Capability {
            cap_type: cap.cap_type.trim().to_ascii_lowercase(),
            items: cap
                .items
                .iter()
                .map(|i| i.trim().to_string())
                .filter(|i| !i.is_empty())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        })
        .filter(|cap| !cap.items.is_empty())
        .collect();

    GardenTool {
        fqid: fqid.clone(),
        tool: ToolIdentity {
            name: fqn.instance.clone().unwrap_or_default(),
            tool_type: svc.offering.to_ascii_lowercase(),
            category: svc.category.to_ascii_lowercase(),
            id: svc.offering_id,
            tags: svc.tags,
        },
        stone: Stone {
            id: svc.stone.id,
            name: svc.stone.name,
            endpoint: svc.stone.endpoint,
        },
        service: ServiceInfo {
            status: svc.status.to_ascii_lowercase(),
            ready: svc.status.eq_ignore_ascii_case(garden_common::SERVICE_RUNNING),
            protocol: svc.connection.protocol,
            uris: svc.connection.uris,
            hostname: Some(svc.connection.hostname),
            ip: Some(svc.connection.ip),
            port: Some(svc.connection.port),
            uri_template: None,
        },
        capabilities,
        storage: None,
    }
}

/// Parse an FQN from a service name and offering type.
///
/// Tries parsing `name` as an FQN first; falls back to constructing
/// `offering::name` if the name is an unqualified instance identifier.
fn parse_fqn_for_fqid(name: &str, offering: &str) -> OfferingFqn {
    let name_lower = name.to_ascii_lowercase();
    let offering_lower = offering.to_ascii_lowercase();

    // Default instance — name matches offering type
    if name_lower == offering_lower || name_lower.is_empty() {
        return OfferingFqn::new(&offering_lower).unwrap_or(OfferingFqn {
            source: None,
            offering: offering_lower,
            instance: None,
            image_ref: None,
        });
    }

    // Already a qualified FQN (V2 "mongodb::prod" or V1 "mongodb:prod") — parse it
    if let Ok(fqn) = OfferingFqn::parse(&name_lower) {
        if fqn.offering == offering_lower {
            return fqn;
        }
    }

    // Bare instance name — construct qualified FQN
    OfferingFqn::with_instance(&offering_lower, &name_lower).unwrap_or(OfferingFqn {
        source: None,
        offering: offering_lower,
        instance: Some(name_lower),
        image_ref: None,
    })
}

/// Map `VolumeState` to `(status, ready)` for tool projection.
fn volume_state_to_readiness(state: &VolumeState) -> (&'static str, bool) {
    match state {
        VolumeState::Online => ("running", true),
        VolumeState::Degraded(_) => ("degraded", false),
        VolumeState::Offline => ("stopped", false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fqid_default_instance() {
        let fqn = parse_fqn_for_fqid("mongodb", "mongodb");
        assert_eq!(fqn.fqn(), "mongodb");
        assert_eq!(fqn.instance, None);

        let fqn = parse_fqn_for_fqid("MongoDB", "mongodb");
        assert_eq!(fqn.fqn(), "mongodb");
    }

    #[test]
    fn fqid_named_instance() {
        // Bare instance name
        let fqn = parse_fqn_for_fqid("prod", "mongodb");
        assert_eq!(fqn.fqn(), "mongodb::prod");
        assert_eq!(fqn.instance, Some("prod".to_string()));

        // V2 qualified
        let fqn = parse_fqn_for_fqid("mongodb::prod", "mongodb");
        assert_eq!(fqn.fqn(), "mongodb::prod");
        assert_eq!(fqn.instance, Some("prod".to_string()));

        // V1 legacy qualified (auto-normalized)
        let fqn = parse_fqn_for_fqid("mongodb:prod", "mongodb");
        assert_eq!(fqn.fqn(), "mongodb::prod");
        assert_eq!(fqn.instance, Some("prod".to_string()));
    }
}
