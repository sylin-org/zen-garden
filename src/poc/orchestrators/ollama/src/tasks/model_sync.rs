//! Model sync task: periodically checks if models need replicating
//! across tier peers, and pulls them as background jobs.

use crate::app_state::AppState;
use crate::domain::policy;
use crate::domain::types::{JobKind, JobStatus};
use crate::infra::ollama_client::OllamaClient;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Sync check interval.
const SYNC_INTERVAL: Duration = Duration::from_secs(60);

/// Run the model sync loop.
pub async fn run(state: AppState, client: OllamaClient, shutdown: CancellationToken) {
    // Give discovery and profiling time to populate the registry.
    tokio::time::sleep(Duration::from_secs(30)).await;

    loop {
        tokio::select! {
            _ = tokio::time::sleep(SYNC_INTERVAL) => {}
            _ = shutdown.cancelled() => return,
        }

        sync_models(&state, &client).await;
    }
}

/// Check for models that need syncing and pull them.
async fn sync_models(state: &AppState, client: &OllamaClient) {
    let sync_targets = {
        let instances = state.instances.read().await;
        let config = state.config.read().await;
        let models = state.models.read().await;
        policy::models_needing_sync(&instances, &config, &models)
    };

    if sync_targets.is_empty() {
        return;
    }

    for (model, targets) in sync_targets {
        tracing::info!(
            model = %model,
            targets = ?targets,
            "model sync: pulling to missing peers"
        );

        let job_id = state
            .create_job(JobKind::ModelSync {
                model: model.clone(),
                targets: targets.clone(),
            })
            .await;

        state.update_job(&job_id, JobStatus::Running, None).await;

        let mut all_ok = true;
        for target in &targets {
            state
                .update_job(
                    &job_id,
                    JobStatus::Running,
                    Some(format!("pulling to {target}")),
                )
                .await;

            match pull_and_wait(client, target, &model).await {
                Ok(()) => {
                    tracing::info!(model = %model, target = %target, "sync pull succeeded");
                    // Re-profile so the model appears in the instance registry
                    if let Ok((avail, loaded, infos, _)) = client.full_profile(target).await {
                        state.update_instance_models(target, avail, loaded).await;
                        for info in infos {
                            state.upsert_model(info).await;
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(model = %model, target = %target, error = %e, "sync pull failed");
                    all_ok = false;
                }
            }
        }

        if all_ok {
            state.complete_job(&job_id).await;
        } else {
            state
                .fail_job(&job_id, "one or more sync pulls failed")
                .await;
        }

        state.emit_event("models.updated", "{}").await;
    }
}

/// Pull a model and consume the entire stream, returning success/failure.
async fn pull_and_wait(client: &OllamaClient, endpoint: &str, model: &str) -> anyhow::Result<()> {
    use futures_util::StreamExt;
    let mut stream = client.pull_model(endpoint, model).await?;
    let mut last_status = String::new();
    while let Some(chunk) = stream.next().await {
        if let Ok(bytes) = chunk {
            if let Ok(progress) =
                serde_json::from_slice::<crate::domain::types::OllamaPullProgress>(&bytes)
            {
                last_status = progress.status;
            }
        }
    }
    if last_status == "success" {
        Ok(())
    } else {
        anyhow::bail!("pull ended with status: {last_status}")
    }
}
