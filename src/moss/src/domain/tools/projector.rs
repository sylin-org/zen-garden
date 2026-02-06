use crate::domain::connection;
use crate::domain::tools::readiness::{offering_readiness, seed_bank_readiness};
use crate::AppState;
use chrono::Utc;
use garden_common::storage::DEFAULT_SEED_BANK_NAME;
use garden_common::tools::{build_tool_fqid, ToolConnection, ToolProjection, ToolType};
use std::collections::{BTreeMap, BTreeSet};

pub async fn project_local_tools(state: &AppState) -> Vec<ToolProjection> {
    let endpoint = state.self_entry.read().await.endpoint.clone();
    let offerings = state.offerings.read().await.clone();
    let local_storage = {
        let cache = state.storage_cache.read().await;
        cache.get_beacon(&state.stone_id).cloned()
    };

    let mut projections = Vec::new();

    for offering in &offerings {
        let Ok(tool_fqid) = build_tool_fqid(ToolType::Offering, &offering.name) else {
            continue;
        };
        let (tool_state, ready) = offering_readiness(offering);

        let protocol = if offering.location.protocol.trim().is_empty() {
            "http".to_string()
        } else {
            offering.location.protocol.trim().to_ascii_lowercase()
        };
        let connection = if offering.location.port > 0 {
            let resolved = connection::resolve_connection(
                &state.stone_name,
                &endpoint,
                offering.location.port,
                &protocol,
                None,
            );
            Some(ToolConnection {
                protocol: resolved.protocol,
                hostname: Some(resolved.hostname),
                ip: Some(resolved.ip),
                port: resolved.port,
                uris: resolved.uris,
            })
        } else {
            None
        };

        let mut aliases = vec![format!(
            "offering:{}",
            offering.offering.to_ascii_lowercase()
        )];
        if offering.name != offering.offering {
            aliases.push(format!("offering:{}", offering.name.to_ascii_lowercase()));
        }

        projections.push(ToolProjection {
            tool_fqid,
            tool_uid: offering.offering_id.clone(),
            tool_type: ToolType::Offering,
            state: tool_state,
            ready,
            revision: 0,
            stone_id: state.stone_id.clone(),
            stone_name: state.stone_name.clone(),
            aliases: normalize_aliases(aliases),
            connection,
            capabilities: to_capability_map(&offering.sub_capabilities),
            capability_revision: 0,
            capability_delta: None,
            job_id: None,
            request_id: None,
            updated_at: Utc::now(),
        });
    }

    if let Some(beacon) = local_storage {
        for seed_bank in beacon.seed_banks {
            let canonical_name = canonical_seed_bank_name(&seed_bank.name);
            let Ok(tool_fqid) = build_tool_fqid(ToolType::SeedBank, &canonical_name) else {
                continue;
            };
            let (tool_state, ready) = seed_bank_readiness(&seed_bank);
            let protocol = seed_bank
                .protocols
                .iter()
                .find(|proto| proto.eq_ignore_ascii_case("s3"))
                .cloned()
                .or_else(|| seed_bank.protocols.first().cloned())
                .unwrap_or_else(|| "storage".to_string())
                .to_ascii_lowercase();
            let port = parse_port_from_endpoint(&beacon.endpoint).unwrap_or(0);

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

            let connection = ToolConnection {
                protocol,
                hostname: Some(connection::build_hostname(&beacon.stone_name)),
                ip: Some(connection::extract_ip(&beacon.endpoint)),
                port,
                uris,
            };

            let mut aliases = vec![format!("seed-bank:{}", seed_bank.name.to_ascii_lowercase())];
            if canonical_name == "default" {
                aliases.push(format!(
                    "seed-bank:{}",
                    DEFAULT_SEED_BANK_NAME.to_ascii_lowercase()
                ));
            }

            projections.push(ToolProjection {
                tool_fqid,
                tool_uid: seed_bank.id.clone(),
                tool_type: ToolType::SeedBank,
                state: tool_state,
                ready,
                revision: 0,
                stone_id: state.stone_id.clone(),
                stone_name: state.stone_name.clone(),
                aliases: normalize_aliases(aliases),
                connection: Some(connection),
                capabilities: BTreeMap::new(),
                capability_revision: 0,
                capability_delta: None,
                job_id: None,
                request_id: None,
                updated_at: Utc::now(),
            });
        }
    }

    projections
}

fn normalize_aliases(aliases: Vec<String>) -> Vec<String> {
    aliases
        .into_iter()
        .map(|alias| alias.trim().to_ascii_lowercase())
        .filter(|alias| !alias.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn canonical_seed_bank_name(name: &str) -> String {
    if name.eq_ignore_ascii_case(DEFAULT_SEED_BANK_NAME) {
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

fn to_capability_map(
    sub_capabilities: &[garden_common::SubCapability],
) -> BTreeMap<String, Vec<String>> {
    let mut caps = BTreeMap::new();

    for cap in sub_capabilities {
        let key = cap.cap_type.trim().to_ascii_lowercase();
        if key.is_empty() {
            continue;
        }
        let entry = caps.entry(key).or_insert_with(Vec::new);
        entry.extend(
            cap.items
                .iter()
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty()),
        );
    }

    for values in caps.values_mut() {
        let normalized: Vec<String> = values
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        *values = normalized;
    }

    caps
}
