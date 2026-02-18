//! Discovery task: subscribe to the Tools API SSE stream and profile
//! new Ollama instances as they appear.

use crate::app_state::AppState;
use crate::domain::types::{InstanceHealth, OllamaInstance};
use crate::infra::ollama_client::OllamaClient;
use crate::infra::tools_stream::{self, ToolEvent};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// Run the discovery loop. Reconnects on stream failure.
pub async fn run(state: AppState, client: OllamaClient, shutdown: CancellationToken) {
    loop {
        if shutdown.is_cancelled() {
            return;
        }

        tracing::info!(endpoint = %state.stone_endpoint, "starting discovery stream");

        let state_clone = state.clone();
        let client_clone = client.clone();

        let result = tools_stream::subscribe_tools_stream(
            &state.stone_endpoint,
            |event| {
                // We can't use async in FnMut, so spawn tasks for profiling
                match event {
                    ToolEvent::OllamaDiscovered {
                        stone_id,
                        stone_name,
                        endpoint,
                    } => {
                        tracing::info!(
                            stone = %stone_name,
                            endpoint = %endpoint,
                            "discovered Ollama instance"
                        );
                        let state = state_clone.clone();
                        let client = client_clone.clone();
                        tokio::spawn(async move {
                            profile_instance(state, client, stone_id, stone_name, endpoint).await;
                        });
                    }
                    ToolEvent::OllamaRemoved {
                        stone_id: _,
                        stone_name,
                    } => {
                        tracing::info!(stone = %stone_name, "Ollama instance removed");
                        let state = state_clone.clone();
                        tokio::spawn(async move {
                            // Find and remove the instance for this stone
                            let endpoint = {
                                let instances = state.instances.read().await;
                                instances
                                    .values()
                                    .find(|i| i.stone_name == stone_name)
                                    .map(|i| i.endpoint.clone())
                            };
                            if let Some(ep) = endpoint {
                                state.remove_instance(&ep).await;
                            }
                        });
                    }
                    ToolEvent::Heartbeat => {
                        tracing::trace!("tools stream heartbeat");
                    }
                }
            },
        )
        .await;

        match result {
            Ok(()) => tracing::warn!("tools stream ended normally, reconnecting"),
            Err(e) => tracing::warn!(error = %e, "tools stream error, reconnecting"),
        }

        // Wait before reconnecting
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(5)) => {}
            _ = shutdown.cancelled() => return,
        }
    }
}

/// Profile a newly discovered Ollama instance.
///
/// Queries /api/tags, /api/ps, /api/show per model, then registers in AppState.
async fn profile_instance(
    state: AppState,
    client: OllamaClient,
    stone_id: String,
    stone_name: String,
    endpoint: String,
) {
    tracing::info!(stone = %stone_name, endpoint = %endpoint, "profiling instance");

    let profile = client.full_profile(&endpoint).await;

    match profile {
        Ok((models_available, models_loaded, model_infos, version)) => {
            let vram_budget = state.vram_budget_for(&stone_name, 0).await;

            // Try to get VRAM total from loaded models
            let vram_total = if !models_loaded.is_empty() {
                // Use the total VRAM consumption as a floor + some headroom
                let used: u64 = models_loaded.iter().map(|m| m.size_vram).sum();
                // We know the GPU has at least this much VRAM. Add 20% headroom guess.
                used + used / 5
            } else {
                // No models loaded — can't determine VRAM from Ollama alone.
                // Default to 8 GiB as conservative estimate; user can override via config.
                8 * 1_073_741_824
            };

            let vram_budget = if vram_budget > 0 {
                vram_budget
            } else {
                vram_total
            };

            // Get GPU name from stone portrait if possible
            let gpu_name = get_gpu_name_from_stone(&stone_name).await;

            let instance = OllamaInstance {
                stone_id,
                stone_name: stone_name.clone(),
                endpoint: endpoint.clone(),
                ollama_version: version,
                gpu_name,
                vram_total_bytes: vram_total,
                vram_budget_bytes: vram_budget,
                health: InstanceHealth::Healthy,
                models_loaded,
                models_available,
                queue_depth: 0,
                last_seen: Instant::now(),
                last_profiled: Instant::now(),
            };

            // Register models
            for info in model_infos {
                state.upsert_model(info).await;
            }

            // Register instance (triggers tier recomputation)
            state.upsert_instance(instance).await;

            tracing::info!(stone = %stone_name, "instance profiled and added to routing pool");
        }
        Err(e) => {
            tracing::warn!(
                stone = %stone_name,
                endpoint = %endpoint,
                error = %e,
                "failed to profile instance"
            );
            // Register as unhealthy so we can retry later
            let instance = OllamaInstance {
                stone_id,
                stone_name,
                endpoint,
                ollama_version: None,
                gpu_name: None,
                vram_total_bytes: 0,
                vram_budget_bytes: 0,
                health: InstanceHealth::Unhealthy {
                    since: Instant::now(),
                    reason: e.to_string(),
                },
                models_loaded: vec![],
                models_available: vec![],
                queue_depth: 0,
                last_seen: Instant::now(),
                last_profiled: Instant::now(),
            };
            state.upsert_instance(instance).await;
        }
    }
}

/// Try to get GPU name from the stone's portrait API.
async fn get_gpu_name_from_stone(stone_name: &str) -> Option<String> {
    let endpoint = format!("http://{stone_name}.local:7185");
    let url = format!("{endpoint}/api/v1/stone/portrait");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .ok()?;

    let resp = client.get(&url).send().await.ok()?;
    let json: serde_json::Value = resp.json().await.ok()?;

    // Look for GPU info in the portrait response
    json.get("foundation")
        .and_then(|f| f.get("gpu"))
        .and_then(|g| g.get("name"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            // Alternative: look in capabilities
            json.get("identity")
                .and_then(|i| i.get("ai"))
                .and_then(|a| a.as_str())
                .map(|s| s.to_string())
        })
}
