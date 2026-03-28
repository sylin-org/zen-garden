//! Reconciliation task — periodic drift detection via `Offering::enumerate()`.
//!
//! Detects model changes (appeared/disappeared/loaded/unloaded) on all
//! registered instances and updates the registry accordingly.

use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::app_state::AppState;
use crate::domain::reconciliation;

/// How often to re-enumerate all instances.
const RECONCILE_INTERVAL: Duration = Duration::from_secs(60);

/// Background task: periodic reconciliation of model registries.
pub async fn run(state: AppState, shutdown: CancellationToken) {
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = tokio::time::sleep(RECONCILE_INTERVAL) => {}
        }

        let instances: Vec<(String, crate::domain::types::OfferingKind)> = {
            let reg = state.instances.read().await;
            reg.values()
                .filter(|i| i.health.is_healthy())
                .map(|i| (i.endpoint.clone(), i.kind))
                .collect()
        };

        for (endpoint, kind) in instances {
            let offering = match state.catalog.get(kind) {
                Some(o) => o.clone(),
                None => continue,
            };

            let fresh_models = match offering.enumerate(&endpoint).await {
                Ok(m) => m,
                Err(e) => {
                    tracing::debug!(
                        endpoint = %endpoint,
                        error = %e,
                        "reconciliation: enumerate failed"
                    );
                    continue;
                }
            };

            let (cached_available, cached_loaded) = {
                let reg = state.instances.read().await;
                match reg.get(&endpoint) {
                    Some(inst) => (inst.models_available.clone(), inst.models_loaded.clone()),
                    None => continue,
                }
            };

            let fresh_available: Vec<String> =
                fresh_models.iter().map(|m| m.name.clone()).collect();
            // Only models with is_loaded=true are resident in VRAM.
            // Ollama sets this from /api/ps. Other offerings set false
            // (static estimate, not runtime state).
            let fresh_loaded: Vec<crate::domain::types::LoadedModel> = fresh_models
                .iter()
                .filter(|m| m.is_loaded)
                .filter_map(|m| {
                    m.vram_bytes.map(|vram| crate::domain::types::LoadedModel {
                        name: m.name.clone(),
                        vram_bytes: vram,
                        expires_at: None,
                    })
                })
                .collect();

            let drifts = reconciliation::diff_instance(
                &endpoint,
                &cached_available,
                &cached_loaded,
                &fresh_available,
                &fresh_loaded,
            );

            if !drifts.is_empty() {
                tracing::info!(
                    endpoint = %endpoint,
                    drifts = drifts.len(),
                    "reconciliation: model changes detected"
                );
                state
                    .update_instance_models(&endpoint, fresh_available, fresh_loaded)
                    .await;
            }
        }
    }

    tracing::info!("reconciliation task shutting down");
}
