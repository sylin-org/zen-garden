//! Cache warming — pull models from ComfyUI instances to local cache (ORCH-0025).
//!
//! After discovery identifies ComfyUI instances, this task scans their model
//! inventories and backfills the local dependency cache. This ensures:
//! - Imported skills referencing existing models don't re-download from internet
//! - A second ComfyUI instance can be provisioned from cache, not from source
//! - Docker wipes don't lose the model cache permanently
//!
//! Runs once after the first successful discovery cycle, then periodically
//! (every 30 min) to catch newly provisioned models.

use std::path::Path;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::app_state::AppState;
use crate::domain::types::OfferingKind;
use crate::skills::cache::{CachePaths, DependencyManifest};

/// How often to re-scan instances for new models after the initial warm.
const RESCAN_INTERVAL: Duration = Duration::from_secs(30 * 60); // 30 min

/// Run the cache warming loop.
pub async fn run(state: AppState, shutdown: CancellationToken) {
    // Wait for discovery to find at least one ComfyUI instance
    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(10)) => {}
            _ = shutdown.cancelled() => return,
        }

        let snap = state.registry.snapshot().clone();
        let has_comfyui = snap.instances.values().any(|i| i.kind == OfferingKind::ComfyUi && i.is_routable());
        if has_comfyui {
            break;
        }
    }

    // Initial warm + periodic rescan
    loop {
        if let Err(e) = warm_cache(&state).await {
            tracing::warn!(error = %e, "cache warm failed");
        }

        tokio::select! {
            _ = tokio::time::sleep(RESCAN_INTERVAL) => {}
            _ = shutdown.cancelled() => return,
        }
    }
}

/// Scan all ComfyUI instances, compare with local cache, pull missing models.
async fn warm_cache(state: &AppState) -> anyhow::Result<()> {
    let cache_paths = CachePaths::new(
        Path::new(&state.data_dir),
        "comfyui",
    );
    tokio::fs::create_dir_all(&cache_paths.provider_dir).await?;

    let mut manifest = DependencyManifest::load(&cache_paths.manifest_path).await;

    // Collect model inventories from all healthy ComfyUI instances
    let snap = state.registry.snapshot().clone();
    let comfyui_instances: Vec<_> = snap.instances.values()
        .filter(|i| i.kind == OfferingKind::ComfyUi && i.is_routable())
        .collect();

    if comfyui_instances.is_empty() {
        return Ok(());
    }

    let model_types = ["checkpoints", "loras", "vae", "upscale_models", "clip", "text_encoders", "diffusion_models"];
    let mut pulled = 0u32;

    for instance in &comfyui_instances {
        let endpoint = &instance.endpoint;
        let moss_endpoint = derive_moss_endpoint(endpoint);
        let offering_fqn = "comfyui";

        for model_type in &model_types {
            // List models on this instance via ComfyUI API
            let models = match list_instance_models(&state.http, endpoint, model_type).await {
                Ok(m) => m,
                Err(_) => continue,
            };

            for filename in &models {
                // Skip if already in local cache
                let resolved = manifest.resolve(filename);
                if manifest.files.contains_key(&resolved) {
                    continue;
                }

                // Pull from instance to local cache
                let local_path = cache_paths.provider_dir.join(filename);

                tracing::info!(
                    model = %filename,
                    model_type,
                    source = %endpoint,
                    "cache warm: pulling model from instance"
                );

                match crate::skills::persistence::pull_model_from_instance(
                    &state.http,
                    &moss_endpoint,
                    offering_fqn,
                    model_type,
                    filename,
                    &local_path,
                ).await {
                    Ok(()) => {
                        // Compute checksum and register in manifest
                        let checksum = match crate::skills::cache::checksum_file(&local_path).await {
                            Ok(cs) => cs,
                            Err(e) => {
                                tracing::warn!(model = %filename, error = %e, "cache warm: checksum failed");
                                continue;
                            }
                        };

                        manifest.files.insert(filename.clone(), checksum);
                        pulled += 1;

                        tracing::info!(
                            model = %filename,
                            model_type,
                            "cache warm: model cached locally"
                        );
                    }
                    Err(e) => {
                        tracing::debug!(
                            model = %filename,
                            error = %e,
                            "cache warm: could not pull model from instance"
                        );
                        // Clean up partial file
                        let _ = tokio::fs::remove_file(&local_path).await;
                    }
                }
            }
        }
    }

    if pulled > 0 {
        manifest.save(&cache_paths.manifest_path).await?;
        tracing::info!(pulled, "cache warm: complete");
    }

    Ok(())
}

/// List models on a ComfyUI instance via GET /models/{type}.
async fn list_instance_models(
    http: &reqwest::Client,
    endpoint: &str,
    model_type: &str,
) -> anyhow::Result<Vec<String>> {
    let resp = http
        .get(format!("{endpoint}/models/{model_type}"))
        .timeout(Duration::from_secs(10))
        .send()
        .await?;

    if !resp.status().is_success() {
        return Ok(Vec::new());
    }

    let models: Vec<String> = resp.json().await.unwrap_or_default();
    Ok(models)
}

/// Derive the Moss HTTP endpoint from a service endpoint.
fn derive_moss_endpoint(service_endpoint: &str) -> String {
    if let Some(colon_pos) = service_endpoint.rfind(':') {
        format!(
            "{}:{}",
            &service_endpoint[..colon_pos],
            garden_common::constants::MOSS_HTTP
        )
    } else {
        format!("{service_endpoint}:{}", garden_common::constants::MOSS_HTTP)
    }
}
