//! Provisioning worker — bounded concurrency job executor (ORCH-0024).
//!
//! Pulls jobs from ProvisioningDomain, executes with a semaphore-bounded
//! concurrency limit. Each job: ensure_cached → push_to_instance.
//!
//! Single shared HTTP client for all downloads (code-standards §19).

use std::sync::Arc;

use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::app_state::AppState;
use crate::domain::provisioning::{DownloadProgress, ProvisioningJob, ProvisioningTarget};

/// Run the provisioning worker loop.
///
/// Blocks until shutdown. Pulls jobs from `state.provisioning`, executes
/// them with bounded concurrency via a semaphore.
pub async fn run(state: AppState, shutdown: CancellationToken) {
    let concurrency = state.provisioning.concurrency();
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let http = state.http.clone();
    let notify = state.provisioning.notifier();

    tracing::info!(concurrency, "provisioning worker started");

    loop {
        // Wait for a job submission or shutdown
        tokio::select! {
            _ = notify.notified() => {}
            _ = shutdown.cancelled() => {
                tracing::info!("provisioning worker: shutdown requested, draining queue");
                state.provisioning.drain().await;
                // Wait for in-flight jobs (with timeout)
                let _ = tokio::time::timeout(
                    std::time::Duration::from_secs(30),
                    acquire_all(&semaphore, concurrency),
                ).await;
                tracing::info!("provisioning worker stopped");
                return;
            }
        }

        // Drain all available jobs up to concurrency limit
        loop {
            let job = match state.provisioning.take_next().await {
                Some(j) => j,
                None => break,
            };

            let permit = match semaphore.clone().acquire_owned().await {
                Ok(p) => p,
                Err(_) => break, // semaphore closed
            };

            let state = state.clone();
            let http = http.clone();
            let shutdown = shutdown.clone();

            tokio::spawn(async move {
                let _permit = permit; // held for the duration
                execute_job(&state, &http, &job, &shutdown).await;
            });
        }
    }
}

/// Execute a single provisioning job.
async fn execute_job(
    state: &AppState,
    http: &reqwest::Client,
    job: &ProvisioningJob,
    shutdown: &CancellationToken,
) {
    let start = std::time::Instant::now();
    let target = &job.target;

    tracing::info!(
        job_id = %job.id,
        skill = %target.skill,
        endpoint = %target.endpoint,
        stone = %job.stone_name,
        "provisioning job started"
    );

    // Build progress emitter
    let event_tx = build_progress_emitter(state, target);

    // Execute with cancellation support
    let result = tokio::select! {
        r = do_provision(state, http, job, event_tx) => r,
        _ = shutdown.cancelled() => Err(anyhow::anyhow!("shutdown requested")),
    };

    match result {
        Ok(()) => {
            let duration = start.elapsed();
            state.provisioning.complete(target, duration).await;

            // Update skill readiness
            state.skills.set_readiness(
                &target.skill,
                &target.endpoint,
                crate::domain::skill::SkillInstanceView {
                    stone_name: job.stone_name.clone(),
                    endpoint: target.endpoint.clone(),
                    ready: true,
                    reason: "provisioned".into(),
                    vram_mb: 0,
                },
            ).await;

            tracing::info!(
                job_id = %job.id,
                skill = %target.skill,
                duration_secs = duration.as_secs(),
                "provisioning completed"
            );
        }
        Err(e) => {
            state.provisioning.fail(target, format!("{e:#}")).await;

            state.skills.set_readiness(
                &target.skill,
                &target.endpoint,
                crate::domain::skill::SkillInstanceView {
                    stone_name: job.stone_name.clone(),
                    endpoint: target.endpoint.clone(),
                    ready: false,
                    reason: format!("provisioning failed: {e}"),
                    vram_mb: 0,
                },
            ).await;

            tracing::warn!(
                job_id = %job.id,
                skill = %target.skill,
                error = %e,
                "provisioning failed — will retry with backoff"
            );
        }
    }
}

/// Do the actual provisioning: ensure_cached + push_to_instance.
async fn do_provision(
    state: &AppState,
    http: &reqwest::Client,
    job: &ProvisioningJob,
    event_tx: Option<crate::skills::provisioner::EventEmitter>,
) -> anyhow::Result<()> {
    let skill_def = state.skills.get_skill(&job.target.skill).await
        .ok_or_else(|| anyhow::anyhow!("skill '{}' not found in registry", job.target.skill))?;

    let cache_paths = crate::skills::cache::CachePaths::new(
        std::path::Path::new(&state.data_dir),
        &job.provider,
    );

    // 1. Ensure all models cached locally
    let cached_models = crate::skills::provisioner::ensure_cached(
        http, &skill_def, &cache_paths, event_tx,
        Some(&state.secrets),
    ).await?;

    // 2. Push to instance
    let moss_endpoint = derive_moss_endpoint(&job.target.endpoint);
    let offering_fqn = job.provider.clone();

    crate::skills::provisioner::push_to_instance(
        http, &cached_models, &moss_endpoint, &offering_fqn, "comfyui-models",
    ).await?;

    // 3. Push skill definition to instance (ORCH-0025 Tier 3)
    // Skill files are stored alongside models so the instance is self-describing.
    let skill_dir = std::path::Path::new(&state.data_dir)
        .join("skills")
        .join(&job.provider)
        .join(job.target.skill.rsplit('.').next().unwrap_or(&job.target.skill));

    if skill_dir.exists() {
        if let Err(e) = crate::skills::persistence::push_skill_to_instance(
            http, &moss_endpoint, &offering_fqn, &skill_dir, &job.target.skill,
        ).await {
            // Non-fatal — provisioning succeeded, skill push is best-effort
            tracing::debug!(
                skill = %job.target.skill,
                error = %e,
                "failed to push skill definition to instance (non-fatal)"
            );
        }
    }

    Ok(())
}

/// Build a progress emitter that updates the domain and SSE stream.
fn build_progress_emitter(
    state: &AppState,
    target: &ProvisioningTarget,
) -> Option<crate::skills::provisioner::EventEmitter> {
    let dashboard_tx = state.dashboard_tx.clone();
    let prov_state = state.provisioning.clone();
    let prov_target = target.clone();

    Some(Arc::new(move |event_type: &str, data: &str| {
        // Update domain progress (fire-and-forget)
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
            let progress = DownloadProgress {
                model: v["model"].as_str().unwrap_or("").to_string(),
                downloaded_bytes: v["downloaded_bytes"].as_u64().unwrap_or(0),
                total_bytes: v["total_bytes"].as_u64(),
            };
            let state = prov_state.clone();
            let target = prov_target.clone();
            tokio::spawn(async move {
                state.update_progress(&target, progress).await;
            });
        }

        // Forward to dashboard SSE
        let _ = dashboard_tx.send(crate::app_state::DashboardEvent {
            event_type: event_type.to_string(),
            data: data.to_string(),
        });
    }))
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

/// Wait until all semaphore permits are reacquired (all jobs finished).
async fn acquire_all(sem: &Semaphore, n: usize) {
    let _ = sem.acquire_many(n as u32).await;
}
