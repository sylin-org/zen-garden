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

            let fqid = build_offering_fqid(&gw.fqn, offering);

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
                    name: instance_name_from_fqid(&fqid),
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
            });
        }
    }

    tools
}

/// Convert a `FoundService` (from service discovery) into a `GardenTool`.
fn found_service_to_garden_tool(svc: FoundService) -> GardenTool {
    let fqid = build_offering_fqid(&svc.name, &svc.offering);

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
            name: instance_name_from_fqid(&fqid),
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
    }
}

/// Build the bare fqid from a service name and offering type.
///
/// If the name and offering are the same (default instance), fqid is just the offering.
/// If different (named instance like "mongodb:prod"), fqid is "offering:name".
fn build_offering_fqid(name: &str, offering: &str) -> String {
    let name_lower = name.to_ascii_lowercase();
    let offering_lower = offering.to_ascii_lowercase();

    if name_lower == offering_lower || name_lower.is_empty() {
        offering_lower
    } else if name_lower.starts_with(&format!("{}:", offering_lower)) {
        // Already qualified: "mongodb:prod" → keep as-is
        name_lower
    } else {
        format!("{}:{}", offering_lower, name_lower)
    }
}

/// Extract the instance name from a fqid.
/// `"mongodb:prod"` → `"prod"`, `"mongodb"` → `""`.
fn instance_name_from_fqid(fqid: &str) -> String {
    fqid.split_once(':')
        .map(|(_, name)| name.to_string())
        .unwrap_or_default()
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
    fn build_offering_fqid_default_instance() {
        assert_eq!(build_offering_fqid("mongodb", "mongodb"), "mongodb");
        assert_eq!(build_offering_fqid("MongoDB", "mongodb"), "mongodb");
    }

    #[test]
    fn build_offering_fqid_named_instance() {
        assert_eq!(build_offering_fqid("prod", "mongodb"), "mongodb:prod");
        assert_eq!(
            build_offering_fqid("mongodb:prod", "mongodb"),
            "mongodb:prod"
        );
    }

    #[test]
    fn instance_name_extraction() {
        assert_eq!(instance_name_from_fqid("mongodb"), "");
        assert_eq!(instance_name_from_fqid("mongodb:prod"), "prod");
        assert_eq!(instance_name_from_fqid("ollama:adopted"), "adopted");
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
