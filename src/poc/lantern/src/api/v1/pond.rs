//! Pond endpoint — trust circle members across the garden

use axum::extract::State;
use axum::Json;
use serde::Serialize;
use serde_json::Value;

use crate::AppState;

/// Pond member (stone in the trust circle)
#[derive(Serialize)]
struct PondMember {
    stone_id: String,
    stone_name: String,
    endpoint: String,
    health: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    mac: Option<String>,
    tags: Vec<String>,
    services_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    os: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cpu_cores: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    memory_mb: Option<u64>,
}

/// GET /api/v1/garden/pond — all stones as trust circle members
pub async fn get_pond(State(state): State<AppState>) -> Json<Value> {
    let topology = state.topology.read().await;

    let members: Vec<PondMember> = topology
        .stones
        .values()
        .map(|entry| {
            let (os, cpu_cores, memory_mb) = entry
                .capabilities
                .as_ref()
                .map(|cap| {
                    (
                        cap.runtime.as_ref().map(|r| r.os.clone()),
                        Some(cap.hardware.cpu.cores),
                        Some(cap.hardware.memory.total_mb),
                    )
                })
                .unwrap_or((None, None, None));

            PondMember {
                stone_id: entry.stone_id.clone(),
                stone_name: entry.stone_name.clone(),
                endpoint: entry.address.http_base(),
                health: entry.health.clone(),
                status: format!("{:?}", entry.status).to_lowercase(),
                mac: entry.mac.clone(),
                tags: entry.tags.clone(),
                services_count: entry.services.len(),
                os,
                cpu_cores,
                memory_mb,
            }
        })
        .collect();

    Json(serde_json::to_value(&members).unwrap())
}
