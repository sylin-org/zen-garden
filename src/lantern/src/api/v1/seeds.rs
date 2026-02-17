//! Seed banks endpoint — storage topology across all stones

use axum::extract::State;
use axum::Json;
use serde::Serialize;
use serde_json::Value;

use crate::AppState;

/// Seed bank with owning stone information
#[derive(Serialize)]
struct SeedBankView {
    stone_id: String,
    stone_name: String,
    id: String,
    name: String,
    capacity_bytes: u64,
    used_bytes: u64,
    visibility: String,
    online: bool,
    #[serde(default)]
    encrypted: bool,
}

/// GET /api/v1/garden/seeds — seed banks aggregated across all stones
pub async fn get_seeds(State(state): State<AppState>) -> Json<Value> {
    let topology = state.topology.read().await;

    let mut banks: Vec<SeedBankView> = Vec::new();

    for (key, entry) in &topology.stones {
        if let Some(enrichment) = topology.enrichment.get(key) {
            for bank in &enrichment.seed_banks {
                banks.push(SeedBankView {
                    stone_id: entry.stone_id.clone(),
                    stone_name: entry.stone_name.clone(),
                    id: bank.id.clone(),
                    name: bank.name.clone(),
                    capacity_bytes: bank.capacity_bytes,
                    used_bytes: bank.used_bytes,
                    visibility: bank.visibility.clone(),
                    online: bank.online,
                    encrypted: bank.encrypted,
                });
            }
        }
    }

    Json(serde_json::to_value(&banks).unwrap())
}
