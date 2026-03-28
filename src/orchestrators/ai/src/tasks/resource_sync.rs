//! Resource sync task — syncs models/resources across tier peers.
//!
//! When `auto_pull_mode` is Sync or OnDemand, models available on any
//! instance in a VRAM tier should be available on all instances in that
//! tier. This task detects missing models and triggers sync via the
//! offering adapter's `sync_resource()` method.
//!
//! Generalized from ollama-orchestrator tasks/model_sync.rs.

use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::app_state::AppState;
use crate::domain::policy;

const SYNC_INTERVAL: Duration = Duration::from_secs(60);

/// Background task: periodic model sync across tier peers.
pub async fn run(state: AppState, shutdown: CancellationToken) {
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = tokio::time::sleep(SYNC_INTERVAL) => {}
        }

        let config = state.config.read().await.clone();
        let instances = state.instances.read().await.clone();
        let models = state.models.read().await.clone();

        let sync_targets = policy::models_needing_sync(&instances, &config, &models);

        if sync_targets.is_empty() {
            continue;
        }

        tracing::info!(
            targets = sync_targets.len(),
            "resource_sync: models needing sync"
        );

        for (model_name, target_endpoints) in &sync_targets {
            // Find a source instance that has this model.
            let source = instances
                .values()
                .find(|i| {
                    i.health.is_healthy()
                        && i.models_available.iter().any(|m| m == model_name)
                        && !target_endpoints.contains(&i.endpoint)
                });

            let source = match source {
                Some(s) => s.clone(),
                None => continue,
            };

            for target_ep in target_endpoints {
                let target = match instances.get(target_ep) {
                    Some(t) => t.clone(),
                    None => continue,
                };

                let offering = match state.catalog.get(target.kind) {
                    Some(o) => o.clone(),
                    None => continue,
                };

                tracing::info!(
                    model = %model_name,
                    from = %source.endpoint,
                    to = %target_ep,
                    "resource_sync: syncing model"
                );

                let job_id = state
                    .create_job(
                        crate::domain::types::JobKind::ModelSync,
                        &format!("sync {model_name} → {}", target.stone.name),
                    )
                    .await;

                match offering
                    .sync_resource(model_name, &source, &target)
                    .await
                {
                    Ok(progress) => {
                        tracing::info!(
                            model = %model_name,
                            to = %target_ep,
                            result = ?progress,
                            "resource_sync: complete"
                        );
                        state.complete_job(&job_id).await;
                    }
                    Err(e) => {
                        tracing::warn!(
                            model = %model_name,
                            to = %target_ep,
                            error = %e,
                            "resource_sync: failed"
                        );
                        state.fail_job(&job_id, &e.to_string()).await;
                    }
                }
            }
        }
    }

    tracing::info!("resource_sync task shutting down");
}
