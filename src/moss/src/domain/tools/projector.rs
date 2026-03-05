//! Tools projector — builds GardenTool snapshots from service discovery and storage.
//!
//! TOOLS-0002: Offerings are projected through `find_services()` (same path as
//! garden-rake find) to get gateway/orchestrator-aware resolution.
//! Seed-banks are projected directly from the storage cache.

use crate::domain::connection;
use crate::domain::service_discovery::{self, FoundService};
use crate::domain::tools::readiness::seed_bank_readiness;
use crate::AppState;
use garden_common::storage::DEFAULT_PUBLIC_SEED_BANK_NAME;
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
    // Gateways registered via Koi mDNS that handle offerings (e.g. MongoDB orchestrator).
    // These appear as category=orchestrator tools.
    {
        let gateways = state.gateways.read().await;
        for (offering, gw) in gateways.iter() {
            let category = gw.category.as_deref().unwrap_or("orchestrator");
            let tags = if gw.tags.is_empty() {
                vec!["orchestrator".to_string()]
            } else {
                gw.tags.clone()
            };

            let fqn = parse_fqn_for_fqid(&gw.fqn, offering);
            let fqid = fqn.fqn();

            let conn = connection::resolve_connection(
                &gw.hostname,
                &format!("http://{}:{}", gw.ip, gw.port),
                gw.port,
                &gw.protocol,
                gw.uri_template.as_deref(),
            );

            tools.push(GardenTool {
                fqid: fqid.clone(),
                tool: ToolIdentity {
                    name: fqn.instance.clone().unwrap_or_default(),
                    tool_type: offering.to_ascii_lowercase(),
                    category: category.to_string(),
                    id: String::new(),
                    tags,
                },
                stone: Stone {
                    id: state.stone_id.clone(),
                    name: state.stone_name.clone(),
                    endpoint: state.self_entry.read().await.address.http_base(),
                },
                service: ServiceInfo {
                    status: garden_common::SERVICE_RUNNING.to_string(),
                    ready: true,
                    protocol: conn.protocol.clone(),
                    uris: conn.uris,
                },
                capabilities: Vec::new(),
                storage: None,
            });
        }
    }

    // ── Seed-banks via storage cache ─────────────────────────────
    let local_storage = {
        let cache = state.storage_cache.read().await;
        cache.get_beacon(&state.stone_id).cloned()
    };

    if let Some(beacon) = local_storage {
        for seed_bank in beacon.seed_banks {
            let canonical = canonical_seed_bank_name(&seed_bank.name);
            let (status, ready) = seed_bank_readiness(&seed_bank);
            let protocol = seed_bank
                .protocols
                .iter()
                .find(|p| p.eq_ignore_ascii_case("s3"))
                .cloned()
                .or_else(|| seed_bank.protocols.first().cloned())
                .unwrap_or_else(|| "storage".to_string())
                .to_ascii_lowercase();
            let _port = parse_port_from_endpoint(&beacon.endpoint).unwrap_or(0);

            let mut uris = Vec::new();
            uris.push(format!(
                "{}/api/v1/storage",
                beacon.endpoint.trim_end_matches('/')
            ));
            if protocol == "s3" {
                uris.push(format!(
                    "{}/api/v1/storage/s3",
                    beacon.endpoint.trim_end_matches('/')
                ));
            }
            uris = uris
                .into_iter()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();

            tools.push(GardenTool {
                fqid: canonical.clone(),
                tool: ToolIdentity {
                    name: String::new(),
                    tool_type: "seed-bank".to_string(),
                    category: "storage".to_string(),
                    id: seed_bank.id.clone(),
                    tags: Vec::new(),
                },
                stone: Stone {
                    id: state.stone_id.clone(),
                    name: state.stone_name.clone(),
                    endpoint: beacon.endpoint.clone(),
                },
                service: ServiceInfo {
                    status: status.to_string(),
                    ready,
                    protocol,
                    uris,
                },
                capabilities: Vec::new(),
                storage: None,
            });
        }
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

fn canonical_seed_bank_name(name: &str) -> String {
    if name.eq_ignore_ascii_case(DEFAULT_PUBLIC_SEED_BANK_NAME) {
        "default".to_string()
    } else {
        name.trim().to_ascii_lowercase()
    }
}

fn parse_port_from_endpoint(endpoint: &str) -> Option<u16> {
    let trimmed = endpoint.trim().trim_end_matches('/');
    let without_scheme = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .unwrap_or(trimmed);
    let host_port = without_scheme.split('/').next().unwrap_or(without_scheme);
    host_port
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse().ok())
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

    #[test]
    fn parse_port_examples() {
        assert_eq!(
            parse_port_from_endpoint("http://192.168.1.20:7185"),
            Some(7185)
        );
        assert_eq!(
            parse_port_from_endpoint("http://192.168.1.20:7185/"),
            Some(7185)
        );
        assert_eq!(parse_port_from_endpoint("http://localhost"), None);
    }

    #[test]
    fn canonical_seed_bank() {
        assert_eq!(
            canonical_seed_bank_name(DEFAULT_PUBLIC_SEED_BANK_NAME),
            "default"
        );
        assert_eq!(canonical_seed_bank_name("custom-bank"), "custom-bank");
    }
}
