//! Moss Aggregator — background polling of stone endpoints
//!
//! Connects to all known Moss instances via their REST API,
//! fetches portrait data, and updates the enrichment cache.
//!
//! Polling interval: 15 seconds for full enrichment.

use std::sync::Arc;
use tokio::sync::RwLock;

use crate::domain::topology::{
    EnrichedCompanion, EnrichedOffering, EnrichedResources, EnrichedSeedBank, GardenTopology,
    StoneEnrichment,
};
use crate::infra::event_bus::EventBus;
use crate::infra::moss_client::MossClient;

/// Interval between enrichment polls (seconds)
const ENRICHMENT_INTERVAL_SECS: u64 = 15;

/// Run the aggregation loop.
///
/// Periodically polls each online stone's portrait API for enrichment data.
pub async fn run_aggregation(
    topology: Arc<RwLock<GardenTopology>>,
    client: MossClient,
    _event_bus: EventBus,
) {
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(ENRICHMENT_INTERVAL_SECS)).await;

        // Collect online stone endpoints
        let targets: Vec<(String, String)> = {
            let topo = topology.read().await;
            topo.stones
                .iter()
                .filter(|(_, entry)| entry.status == garden_common::StoneStatus::Online)
                .map(|(key, entry)| (key.clone(), entry.address.http_base()))
                .collect()
        };

        if targets.is_empty() {
            continue;
        }

        tracing::debug!(count = targets.len(), "Enriching stones");

        for (key, endpoint) in targets {
            match fetch_portrait(&client, &endpoint).await {
                Ok(enrichment) => {
                    let mut topo = topology.write().await;
                    topo.enrichment.insert(key, enrichment);
                }
                Err(e) => {
                    tracing::debug!(
                        endpoint = %endpoint,
                        error = %e,
                        "Failed to enrich stone (will retry)"
                    );
                }
            }
        }
    }
}

/// Fetch portrait data from a Moss stone and convert to enrichment.
async fn fetch_portrait(client: &MossClient, endpoint: &str) -> anyhow::Result<StoneEnrichment> {
    let url = format!("{}/api/v1/stone/portrait", endpoint);
    let portrait: serde_json::Value = client.get_json(&url).await?;

    // Parse identity.color from portrait
    let color = portrait
        .get("identity")
        .and_then(|id| id.get("color"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut enrichment = StoneEnrichment {
        color,
        last_enriched: Some(chrono::Utc::now()),
        ..Default::default()
    };

    // Parse offerings from portrait
    if let Some(offerings) = portrait.get("offerings").and_then(|v| v.as_array()) {
        enrichment.offerings = offerings
            .iter()
            .filter_map(|o| {
                Some(EnrichedOffering {
                    offering_id: o
                        .get("offering_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    name: o.get("name").and_then(|v| v.as_str())?.to_string(),
                    offering: o
                        .get("offering")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    category: o
                        .get("category")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    status: o
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    health: o
                        .get("health")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    port: o.get("port").and_then(|v| v.as_u64()).unwrap_or(0) as u16,
                    instance_name: o
                        .get("instance_name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                })
            })
            .collect();
    }

    // Parse seed banks from portrait
    if let Some(banks) = portrait.get("seed_banks").and_then(|v| v.as_array()) {
        enrichment.seed_banks = banks
            .iter()
            .filter_map(|b| {
                Some(EnrichedSeedBank {
                    id: b
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    name: b.get("name").and_then(|v| v.as_str())?.to_string(),
                    capacity_bytes: (b.get("capacity_gb").and_then(|v| v.as_f64()).unwrap_or(0.0)
                        * 1_073_741_824.0) as u64,
                    used_bytes: (b.get("used_gb").and_then(|v| v.as_f64()).unwrap_or(0.0)
                        * 1_073_741_824.0) as u64,
                    visibility: b
                        .get("visibility")
                        .and_then(|v| v.as_str())
                        .unwrap_or("open")
                        .to_string(),
                    online: b.get("online").and_then(|v| v.as_bool()).unwrap_or(true),
                    encrypted: b
                        .get("encrypted")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                })
            })
            .collect();
    }

    // Parse companions from portrait
    if let Some(companions) = portrait.get("companions").and_then(|v| v.as_array()) {
        enrichment.companions = companions
            .iter()
            .filter_map(|c| {
                Some(EnrichedCompanion {
                    id: c.get("id").and_then(|v| v.as_str())?.to_string(),
                    name: c
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    status: c
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    description: c
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                })
            })
            .collect();
    }

    // Parse foundation (resources) from portrait
    if let Some(foundation) = portrait.get("foundation") {
        let cpu = foundation.get("cpu");
        let memory = foundation.get("memory");
        let disk = foundation.get("disk");

        let cpu_cores = cpu
            .and_then(|c| c.get("cores"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let cpu_percent = cpu
            .and_then(|c| c.get("percent"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32;
        let mem_total = memory
            .and_then(|m| m.get("total_gb"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let mem_used = memory
            .and_then(|m| m.get("used_gb"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let mem_pct = memory
            .and_then(|m| m.get("percent"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32;
        let disk_total = disk
            .and_then(|d| d.get("total_gb"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let disk_used = disk
            .and_then(|d| d.get("used_gb"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let disk_pct = disk
            .and_then(|d| d.get("percent"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32;

        enrichment.resources = Some(EnrichedResources {
            cpu_cores,
            cpu_percent,
            memory_total_bytes: (mem_total * 1_073_741_824.0) as u64,
            memory_used_bytes: (mem_used * 1_073_741_824.0) as u64,
            memory_percent: mem_pct,
            disk_total_gb: disk_total,
            disk_used_gb: disk_used,
            disk_percent: disk_pct,
            uptime_seconds: 0,
        });
    }

    Ok(enrichment)
}
