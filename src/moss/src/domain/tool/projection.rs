//! Tools projector — builds GardenTool snapshots from offerings and
//! storage, plus the helper functions that drive full "project →
//! reconcile → publish" cycles.
//!
//! Reads `state.offerings` directly and calls `connection::resolve_connection()`
//! for URI composition. No FoundService intermediate.
//! Seed-banks are projected directly from the seed bank lifecycle objects.

use super::event::ToolChanged;
use crate::Moss;
use crate::domain::connection;
use crate::domain::storage::VolumeState;
use garden_common::Offering;
use garden_common::offerings::OfferingFqn;
use garden_common::tools::{Capability, GardenTool, ServiceInfo, Stone, ToolDelta, ToolIdentity};
use std::collections::BTreeSet;

/// Project all local tools (offerings + seed-banks) as GardenTool instances.
///
/// Three sources feed the projection:
/// 1. **Offerings** — read directly from `state.offerings`.
/// 2. **Gateways** — written directly to `tool.registry` with `EntryOrigin::Gateway`
///    by the gateway API. Not projected here — `reconcile_local` only touches
///    `Local` entries, so `Gateway` entries survive.
/// 3. **Seed-banks** — from the unified volume store.
pub async fn project_local_tools(state: &Moss) -> Vec<GardenTool> {
    let mut tools = Vec::new();

    let endpoint = state.current.address.read().await.http_base();
    let stone = Stone {
        id: state.current.stone.id.clone(),
        name: state.current.stone.name.clone(),
        endpoint: endpoint.clone(),
    };

    // ── Offerings (direct read, no service_discovery) ─────────
    {
        let offerings = state.offerings.snapshot().await;
        for offering in &offerings {
            tools.push(offering_to_garden_tool(offering, &stone, state).await);
        }
    }

    // ── Managed storages from unified volumes ────────────────────
    let managed_vols: Vec<_> = {
        let map = state.current.storage.volumes.read().await;
        map.values().filter(|v| v.is_managed()).cloned().collect()
    };

    for vol in &managed_vols {
        let Some(mgmt) = vol.management() else {
            continue;
        };
        let (status, ready) = volume_state_to_readiness(vol.state());
        let visibility_str = mgmt.visibility.to_string();

        let fqid = mgmt.display_name().to_string();

        let protocols = vec![
            garden_common::constants::PROTOCOL_S3.to_string(),
            garden_common::constants::PROTOCOL_STORAGE.to_string(),
        ];
        let protocol = garden_common::constants::PROTOCOL_S3.to_string();

        let base = endpoint.trim_end_matches('/');
        let uris = vec![
            format!("{base}/api/v1/storage"),
            format!("{base}/api/v1/storage/s3"),
        ];

        tools.push(GardenTool {
            fqid,
            tool: ToolIdentity {
                name: mgmt.name.clone(),
                tool_type: garden_common::constants::TOOL_TYPE_SEED_BANK.to_string(),
                category: garden_common::constants::CATEGORY_STORAGE.to_string(),
                id: mgmt.id.clone(),
                tags: Vec::new(),
                source: String::new(),
            },
            stone: stone.clone(),
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
                capacity_bytes: vol.capacity_bytes(),
                used_bytes: vol.used_bytes(),
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

/// Build a GardenTool directly from an Offering.
///
/// Calls `connection::infer_protocol()` and `connection::resolve_connection()`
/// for URI composition. Category comes from `offering.category` (Phase 0).
async fn offering_to_garden_tool(
    offering: &Offering,
    stone: &Stone,
    state: &Moss,
) -> GardenTool {
    let fqn = parse_fqn_for_fqid(&offering.name.to_string(), &offering.offering);
    let fqid = fqn.fqn();

    let category = if offering.category.is_empty() {
        offering.offering.clone()
    } else {
        offering.category.clone()
    };

    let protocol = connection::infer_protocol(&offering.offering, &category, state).await;
    let port = offering.location.port;

    let manifest_entry = state.catalog.get_manifest(&offering.offering);
    let connection_profile = manifest_entry
        .as_ref()
        .and_then(|entry| entry.connection.as_ref());
    let template = connection::select_uri_template(connection_profile, &category);

    let conn = connection::resolve_connection(
        &stone.name,
        &stone.endpoint,
        port,
        &protocol,
        template.as_deref(),
    );

    let capabilities: Vec<Capability> = offering
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

    let status_str = offering.status.to_string().to_ascii_lowercase();

    GardenTool {
        fqid,
        tool: ToolIdentity {
            name: fqn.instance.clone().unwrap_or_default(),
            tool_type: offering.offering.to_ascii_lowercase(),
            category: category.to_ascii_lowercase(),
            id: offering.offering_id.clone(),
            tags: Vec::new(),
            source: String::new(),
        },
        stone: stone.clone(),
        service: ServiceInfo {
            status: status_str.clone(),
            ready: status_str == garden_common::constants::SERVICE_RUNNING,
            protocol: conn.protocol,
            uris: conn.uris,
            hostname: Some(conn.hostname),
            ip: Some(conn.ip),
            port: Some(conn.port),
            uri_template: None,
        },
        capabilities,
        storage: None,
    }
}

/// Parse an FQN from a service name and offering type.
fn parse_fqn_for_fqid(name: &str, offering: &str) -> OfferingFqn {
    let name_lower = name.to_ascii_lowercase();
    let offering_lower = offering.to_ascii_lowercase();

    if name_lower == offering_lower || name_lower.is_empty() {
        return OfferingFqn::new(&offering_lower).unwrap_or(OfferingFqn {
            source: None,
            offering: offering_lower,
            instance: None,
            image_ref: None,
        });
    }

    if let Ok(fqn) = OfferingFqn::parse(&name_lower)
        && fqn.offering == offering_lower
    {
        return fqn;
    }

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

// ── Reconciliation + publication helpers ────────────────────────────────
//
// These free functions compose the aggregate's typed commands with the
// Moss-dependent projection step and the transport publish step.
// The projection task calls `reproject_and_publish`; gateway handlers
// and the reaper task call `publish_events_for_state` after their own
// typed commands.
//
// Keeping these as free functions in the `projection` module (rather
// than methods on `Tool`) preserves the aggregate's freedom from an
// `Moss` back-reference per ARCH-0019.

/// Reproject local tools from `Moss` and reconcile them into the
/// Tool aggregate; publish any resulting wire deltas via the injected
/// beacon transport. Best-effort — failures are logged as warnings.
///
/// Called by:
/// - The `offerings-projection` background task on every `OfferingsChanged`.
/// - The storage-mutation path in `app_state.rs` after a volume change.
pub async fn reproject_and_publish(state: &Moss) {
    let projections = project_local_tools(state).await;
    let stone_id = state.current.stone.id.clone();
    let events = state.tool.reconcile_local(&stone_id, projections).await;
    publish_events_for_state(state, &events).await;
}

/// Publish wire deltas for the given domain events via the injected
/// beacon transport. Batch events (`Reaped`, `BeaconApplied`,
/// `StoneRemoved`) are filtered out by `ToolChanged::as_delta`; the
/// per-entry `Upserted` / `Removed` events they accompany carry the
/// wire delta and are the ones that fire.
///
/// Best-effort — failures are logged as warnings. Empty delta batches
/// are skipped without calling the transport.
pub async fn publish_events_for_state(state: &Moss, events: &[ToolChanged]) {
    let deltas: Vec<ToolDelta> = events
        .iter()
        .filter_map(|e| e.as_delta().cloned())
        .collect();
    if deltas.is_empty() {
        return;
    }
    let endpoint = state.current.address.read().await.http_base();
    if endpoint.trim().is_empty() {
        return;
    }
    if let Err(e) = state
        .tool
        .publish_incremental(
            &state.current.stone.id,
            &state.current.stone.name,
            &endpoint,
            deltas,
        )
        .await
    {
        tracing::warn!(error = %e, "Failed to publish tools beacon");
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
        let fqn = parse_fqn_for_fqid("prod", "mongodb");
        assert_eq!(fqn.fqn(), "mongodb::prod");
        assert_eq!(fqn.instance, Some("prod".to_string()));

        let fqn = parse_fqn_for_fqid("mongodb::prod", "mongodb");
        assert_eq!(fqn.fqn(), "mongodb::prod");
        assert_eq!(fqn.instance, Some("prod".to_string()));

        let fqn = parse_fqn_for_fqid("mongodb:prod", "mongodb");
        assert_eq!(fqn.fqn(), "mongodb::prod");
        assert_eq!(fqn.instance, Some("prod".to_string()));
    }
}
