//! Extension API — `/v1/` endpoints for models, stones, capabilities.

use axum::extract::State;
use axum::Json;

use crate::app_state::AppState;
use crate::domain::types::Capability;

/// `GET /v1/models` — merged model list across all offerings.
pub async fn list_models(State(state): State<AppState>) -> Json<serde_json::Value> {
    let models = state.models.read().await;
    let model_list: Vec<&str> = models.keys().map(|k| k.as_str()).collect();
    Json(serde_json::json!({ "models": model_list }))
}

/// `GET /v1/stones` — discovered stones with offering details.
pub async fn list_stones(State(state): State<AppState>) -> Json<serde_json::Value> {
    let instances = state.instances.read().await;
    let stones: Vec<serde_json::Value> = instances
        .values()
        .map(|inst| {
            serde_json::json!({
                "stone": inst.stone,
                "endpoint": inst.endpoint,
                "offering": inst.kind,
                "health": inst.health,
                "capabilities": inst.capabilities,
                "models_available": inst.models_available.len(),
                "models_loaded": inst.models_loaded.len(),
                "queue_depth": inst.queue_depth,
                "priority": inst.priority,
            })
        })
        .collect();
    Json(serde_json::json!({ "stones": stones }))
}

/// `GET /v1/capabilities` — available capabilities and which offerings serve them.
pub async fn list_capabilities(State(state): State<AppState>) -> Json<serde_json::Value> {
    let instances = state.instances.read().await;

    let mut cap_map: std::collections::HashMap<Capability, Vec<String>> =
        std::collections::HashMap::new();
    for inst in instances.values() {
        if !inst.health.is_healthy() {
            continue;
        }
        for cap in &inst.capabilities {
            cap_map
                .entry(*cap)
                .or_default()
                .push(inst.endpoint.clone());
        }
    }

    let capabilities: Vec<serde_json::Value> = cap_map
        .iter()
        .map(|(cap, endpoints)| {
            serde_json::json!({
                "capability": cap.to_string(),
                "instance_count": endpoints.len(),
            })
        })
        .collect();

    Json(serde_json::json!({ "capabilities": capabilities }))
}
