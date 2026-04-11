//! State provider for election criteria evaluation

use crate::app_state::AppState;
use crate::domain::fitness;
use crate::version_string;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

/// State provider that extracts election-relevant fields from AppState
pub struct MossStateProvider {
    state: Arc<AppState>,
}

impl MossStateProvider {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    /// Get current state snapshot asynchronously
    pub async fn get_state_async(&self) -> HashMap<String, Value> {
        let mut fields = HashMap::new();

        // Basic identity
        fields.insert("stone_id".to_string(), json!(self.state.current.stone.id));
        fields.insert(
            "stone_name".to_string(),
            json!(self.state.current.stone.name),
        );

        // Version
        fields.insert("moss_version".to_string(), json!(version_string()));

        // Uptime (seconds since start)
        let uptime_secs = self.state.start_time.elapsed().as_secs();
        fields.insert("uptime".to_string(), json!(uptime_secs));

        // Health (from current.health)
        let health = self.state.current.health.read().await.clone();
        fields.insert("health".to_string(), json!(health));

        // Offering count
        let offerings_count = self.state.offerings.read().await.len();
        fields.insert("offerings".to_string(), json!(offerings_count));

        fields
    }
}

impl super::election_service::StateProvider for MossStateProvider {
    fn get_state(&self) -> HashMap<String, Value> {
        // For sync access, return minimal identity
        // Async criteria evaluation should use get_state_async() instead
        let mut fields = HashMap::new();
        fields.insert("stone_id".to_string(), json!(self.state.current.stone.id));
        fields.insert(
            "stone_name".to_string(),
            json!(self.state.current.stone.name),
        );
        fields.insert("moss_version".to_string(), json!(version_string()));
        fields
    }
}

/// Placeholder state provider for bootstrapping
/// Used temporarily before AppState is fully constructed
pub struct PlaceholderStateProvider;

impl super::election_service::StateProvider for PlaceholderStateProvider {
    fn get_state(&self) -> HashMap<String, Value> {
        HashMap::new() // No criteria matching during bootstrap
    }
}

// ============================================================================
// Fitness Provider (ORCH-0001)
// ============================================================================

/// Fitness provider that computes scores from live AppState.
///
/// Injected into `Elections` after bootstrap so the election protocol
/// can ask "how fit is this stone for offering X?" without knowing the answer
/// algorithm. SoC between election protocol (infra) and fitness scoring (domain).
///
/// Relies on the **existing per-stone compatibility evaluation** stored in
/// the compiled offerings index — no manifest constraint duplication.
pub struct MossFitnessProvider {
    state: Arc<AppState>,
}

impl MossFitnessProvider {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

impl super::election_service::FitnessProvider for MossFitnessProvider {
    fn compute_fitness(
        &self,
        offering_fqn: &str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Option<(i16, Option<String>)>> + Send + '_>,
    > {
        let fqn = offering_fqn.to_string();
        Box::pin(async move {
            // Find the running offering by FQN
            let offering = self.state.find_offering(&fqn).await?;

            // ORCH-0008: if a registered gateway handles this offering type,
            // this stone is ineligible — the gateway owns the lifecycle.
            {
                let reg = self.state.tool.registry.read().await;
                if reg.handles_offering(&offering.offering) {
                    return None;
                }
            }

            // Look up the per-stone compatibility evaluation from the
            // compiled offerings index. This was already evaluated against
            // THIS stone's capabilities when the index was built.
            let compatibility = {
                let idx_guard = self.state.offerings_index.read().await;
                idx_guard.as_ref().and_then(|idx| {
                    idx.offerings
                        .iter()
                        .find(|o| o.name == offering.offering)
                        .map(|o| o.compatibility.clone())
                })
            };

            let compat = compatibility.unwrap_or_else(|| {
                // Index not built yet or offering not in manifest — assume pass
                crate::domain::compatibility::CompiledCompatibility {
                    decision: garden_common::constants::COMPAT_PASS.to_string(),
                    reason: None,
                    original_image: None,
                    fallback_image: None,
                    fallback_name: None,
                    suggestion: None,
                }
            });

            // Collect normalised resources for placement scoring
            let resources = crate::domain::resources_collection::get_local_resources().ok();

            let offering_count = self.state.offerings.read().await.len();

            // Compute fitness score (domain logic)
            let score = fitness::compute_fitness_score(
                &offering,
                &compat,
                resources.as_ref(),
                offering_count,
            )?;

            // Extract pin_timestamp if pinned
            let pin_ts = offering
                .orchestration
                .as_ref()
                .and_then(|o| o.pin_timestamp.clone());

            Some((score, pin_ts))
        })
    }
}
