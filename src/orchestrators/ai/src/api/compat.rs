//! Ollama-compatible endpoints for backward compatibility.
//!
//! Standard Ollama clients expect these endpoints on the proxy port.
//! The AI orchestrator provides them by merging data across all Ollama
//! instances in the registry.

use axum::extract::State;
use axum::Json;

use crate::app_state::AppState;
use crate::domain::types::OfferingKind;

/// `GET /` — returns "Ollama is running" for Ollama client compatibility.
///
/// This makes the AI orchestrator's proxy port pass the standard Ollama
/// client liveness check (`ollama list` probes `GET /` for this string).
pub async fn ollama_root() -> &'static str {
    "Ollama is running"
}

/// `GET /api/tags` — merged model list across all Ollama instances.
pub async fn ollama_tags(State(state): State<AppState>) -> Json<serde_json::Value> {
    let instances = state.instances.read().await;
    let models = state.models.read().await;

    // Collect unique model names from all Ollama instances.
    let mut seen = std::collections::HashSet::new();
    let mut model_list = Vec::new();

    for inst in instances.values() {
        if inst.kind != OfferingKind::Ollama || !inst.health.is_healthy() {
            continue;
        }
        for model_name in &inst.models_available {
            if seen.insert(model_name.clone()) {
                let info = models.get(model_name);
                model_list.push(serde_json::json!({
                    "name": model_name,
                    "model": model_name,
                    "size": info.map(|i| i.size_disk).unwrap_or(0),
                    "details": {
                        "family": info.and_then(|i| i.family.as_deref()),
                        "parameter_size": info.and_then(|i| i.parameter_size.as_deref()),
                        "quantization_level": info.and_then(|i| i.quantization_level.as_deref()),
                        "format": info.and_then(|i| i.format.as_deref()),
                    },
                }));
            }
        }
    }

    Json(serde_json::json!({ "models": model_list }))
}

/// `GET /api/ps` — merged list of currently loaded models across all Ollama instances.
pub async fn ollama_ps(State(state): State<AppState>) -> Json<serde_json::Value> {
    let instances = state.instances.read().await;

    let mut running = Vec::new();
    for inst in instances.values() {
        if inst.kind != OfferingKind::Ollama || !inst.health.is_healthy() {
            continue;
        }
        for loaded in &inst.models_loaded {
            running.push(serde_json::json!({
                "name": loaded.name,
                "model": loaded.name,
                "size": loaded.vram_bytes,
                "size_vram": loaded.vram_bytes,
                "expires_at": loaded.expires_at,
            }));
        }
    }

    Json(serde_json::json!({ "models": running }))
}

/// `GET /api/version` — orchestrator version.
pub async fn ollama_version() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
    }))
}
