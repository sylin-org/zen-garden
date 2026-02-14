//! Offerings endpoint — aggregated catalog + deployment state across all stones

use axum::extract::State;
use axum::Json;
use serde::Serialize;
use serde_json::Value;

use crate::AppState;

/// Offering identity group: same offering type deployed across multiple stones
#[derive(Serialize)]
struct OfferingGroup {
    offering: String,
    category: String,
    instances: Vec<OfferingInstance>,
}

#[derive(Serialize)]
struct OfferingInstance {
    stone_id: String,
    stone_name: String,
    offering_id: String,
    name: String,
    status: String,
    health: String,
    port: u16,
}

/// GET /api/v1/garden/offerings — offerings aggregated across all stones
pub async fn get_offerings(State(state): State<AppState>) -> Json<Value> {
    let topology = state.topology.read().await;

    // Collect all offerings across all stones, grouped by offering type
    let mut groups: std::collections::HashMap<String, OfferingGroup> =
        std::collections::HashMap::new();

    for (key, entry) in &topology.stones {
        if let Some(enrichment) = topology.enrichment.get(key) {
            for offering in &enrichment.offerings {
                let group =
                    groups
                        .entry(offering.offering.clone())
                        .or_insert_with(|| OfferingGroup {
                            offering: offering.offering.clone(),
                            category: offering.category.clone(),
                            instances: Vec::new(),
                        });

                group.instances.push(OfferingInstance {
                    stone_id: entry.stone_id.clone(),
                    stone_name: entry.stone_name.clone(),
                    offering_id: offering.offering_id.clone(),
                    name: offering.name.clone(),
                    status: offering.status.clone(),
                    health: offering.health.clone(),
                    port: offering.port,
                });
            }
        }
    }

    let groups_vec: Vec<OfferingGroup> = groups.into_values().collect();
    Json(serde_json::to_value(&groups_vec).unwrap())
}
