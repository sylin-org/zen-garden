//! Monitoring API endpoints — oplog, cache, lag, placement.
//!
//! All monitoring data is read from `ReplicaSetState` (populated by the
//! health monitor task every 15 seconds).  No additional MongoDB connections
//! are made here — the health monitor is the single writer.

use crate::app_state::AppState;
use crate::domain::placement::{self, StonePlacementProfile};
use crate::domain::types::*;
use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

/// `GET /api/monitoring/oplog` — oplog health for all replica sets.
///
/// Reads cached oplog snapshots from state (written by health monitor).
pub async fn get_oplog(State(state): State<AppState>) -> Json<Value> {
    let replica_sets = state.replica_sets.read().await;
    let mut results: Vec<Value> = Vec::new();

    for (fqn, rs) in replica_sets.iter() {
        if !rs.initialized {
            continue;
        }

        let oplog_value = match &rs.oplog {
            Some(health) => serde_json::to_value(health).unwrap_or(json!(null)),
            None => json!(null),
        };

        results.push(json!({
            "fqn": fqn,
            "rs_name": rs.rs_name,
            "oplog": oplog_value,
        }));
    }

    Json(json!({ "oplog_health": results }))
}

/// `GET /api/monitoring/cache` — WiredTiger cache status for all replica sets.
///
/// Reads cached WiredTiger snapshots from state (written by health monitor).
pub async fn get_cache(State(state): State<AppState>) -> Json<Value> {
    let replica_sets = state.replica_sets.read().await;
    let mut results: Vec<Value> = Vec::new();

    for (fqn, rs) in replica_sets.iter() {
        if !rs.initialized {
            continue;
        }

        let cache_value = match &rs.cache {
            Some(snapshot) => json!({
                "status": snapshot.status,
                "health": snapshot.health,
                "recommendations": snapshot.recommendations,
            }),
            None => json!(null),
        };

        results.push(json!({
            "fqn": fqn,
            "rs_name": rs.rs_name,
            "cache": cache_value,
        }));
    }

    Json(json!({ "cache_status": results }))
}

/// `GET /api/monitoring/lag` — replication lag for all secondaries.
pub async fn get_lag(State(state): State<AppState>) -> Json<Value> {
    let replica_sets = state.replica_sets.read().await;
    let mut results: Vec<Value> = Vec::new();

    for (fqn, rs) in replica_sets.iter() {
        if !rs.initialized {
            continue;
        }

        let secondaries: Vec<Value> = rs
            .members
            .iter()
            .filter(|m| m.role == ReplicaRole::Secondary)
            .map(|m| {
                json!({
                    "endpoint": m.endpoint,
                    "stone_name": m.stone_name,
                    "lag_seconds": m.lag_seconds,
                    "healthy": m.healthy,
                    "last_heartbeat": m.last_heartbeat.map(|dt| dt.to_rfc3339()),
                })
            })
            .collect();

        let max_lag = rs
            .members
            .iter()
            .filter_map(|m| m.lag_seconds)
            .fold(0.0_f64, f64::max);

        results.push(json!({
            "fqn": fqn,
            "rs_name": rs.rs_name,
            "max_lag_seconds": max_lag,
            "secondaries": secondaries,
        }));
    }

    Json(json!({ "replication_lag": results }))
}

/// `GET /api/monitoring/placement` — placement recommendations.
///
/// Queries the full topology to get hardware capabilities for all stones,
/// then scores them for MongoDB placement suitability.
pub async fn get_placement(State(state): State<AppState>) -> Json<Value> {
    use garden_common::types::topology::TopologyEntry;

    // Fetch full topology from the tended stone to get all stones + capabilities
    let tended_endpoint = state.tended_endpoint().await;
    let topology_entries: Vec<TopologyEntry> = match tended_endpoint {
        Some(ref ep) => {
            let url = format!("{}/api/v1/garden/topology", ep.trim_end_matches('/'));
            match reqwest::get(&url).await {
                Ok(resp) => {
                    #[derive(serde::Deserialize)]
                    struct TopoResp {
                        data: Vec<TopologyEntry>,
                    }
                    match resp.json::<TopoResp>().await {
                        Ok(topo) => topo.data,
                        Err(e) => {
                            tracing::warn!(error = %e, "failed to parse topology for placement");
                            vec![]
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "failed to fetch topology for placement");
                    vec![]
                }
            }
        }
        None => vec![],
    };

    // Build profiles from topology entries (all stones, not just ones with MongoDB)
    let instances = state.instances.read().await;
    let mut fqns_by_stone: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for inst in instances.values() {
        fqns_by_stone
            .entry(inst.stone_id.clone())
            .or_default()
            .push(inst.fqn.to_string());
    }

    let mut profiles: Vec<StonePlacementProfile> = Vec::new();

    for entry in &topology_entries {
        let caps = entry.capabilities.as_ref();
        let ram_mb = caps
            .map(|c| c.hardware.memory.total_mb)
            .unwrap_or(0);
        // Tri-state: Some(true)=SSD/NVMe, Some(false)=HDD, None=unknown
        let has_ssd = caps
            .and_then(|c| c.hardware.disk.as_ref())
            .and_then(|d| d.disk_type.as_ref())
            .and_then(|t| {
                if t.eq_ignore_ascii_case("ssd") || t.eq_ignore_ascii_case("nvme") {
                    Some(true)
                } else if t.eq_ignore_ascii_case("hdd") {
                    Some(false)
                } else {
                    None // "Unknown" or other — treat as undetected
                }
            });
        let vram_mb = caps
            .map(|c| c.hardware.gpus.iter().filter_map(|g| g.vram_mb).sum::<u64>())
            .unwrap_or(0);
        let other_offerings = entry
            .services
            .iter()
            .filter(|s| s.status == "running")
            .count() as u32;

        let cpu_cores = caps
            .map(|c| c.hardware.cpu.cores as u32)
            .unwrap_or(0);

        profiles.push(StonePlacementProfile {
            stone_name: entry.stone_name.clone(),
            stone_id: entry.stone_id.clone(),
            ram_mb,
            cpu_cores,
            other_offerings,
            has_ssd,
            installed_fqns: fqns_by_stone.get(&entry.stone_id).cloned().unwrap_or_default(),
            vram_mb,
            moss_endpoint: Some(
                entry
                    .address
                    .https_base()
                    .unwrap_or_else(|| entry.address.http_base()),
            ),
        });
    }

    // If topology was empty (no tended stone yet), fall back to instances-only view
    if profiles.is_empty() {
        for instance in instances.values() {
            if profiles.iter().any(|p| p.stone_id == instance.stone_id) {
                continue;
            }
            profiles.push(StonePlacementProfile {
                stone_name: instance.stone_name.clone(),
                stone_id: instance.stone_id.clone(),
                ram_mb: 0,
                cpu_cores: 0,
                other_offerings: 0,
                has_ssd: None,
                installed_fqns: fqns_by_stone.get(&instance.stone_id).cloned().unwrap_or_default(),
                vram_mb: 0,
                moss_endpoint: Some(instance.moss_endpoint.clone()),
            });
        }
    }

    let recommendations = placement::evaluate_placement(&profiles);

    Json(json!({ "placement": recommendations }))
}
