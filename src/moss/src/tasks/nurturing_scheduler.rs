//! Nurturing Scheduler - Background task for automated backups
//!
//! Handles the nurturing workflow when triggered by system timers:
//! 1. Harvest: Create local A/B backup snapshot
//! 2. Replicate: Copy to seed bank (with routing and failover)
//! 3. Prune: Clean up old snapshots based on retention policy
//!
//! # Trigger Mechanism
//! System timers (systemd on Linux, Task Scheduler on Windows) call
//! the `/api/v1/nurturing/{name}/trigger` endpoint. This module
//! executes the actual workflow.
//!
//! # Seed Bank Routing
//! Uses the local SeedBankRegistry to find available seed banks and selects
//! targets based on routing strategy. Implements failover if primary fails.

use crate::domain::nurturing::{build_memories_manifest, NurturingResult, ReplicationResult};
use crate::infra::storage::SeedBankRegistry;
use crate::AppState;
use anyhow::{Context, Result};
use garden_common::storage::{SeedBankInfo, SeedBankRole};
use garden_common::types::Offering;

/// Result of a full nurturing workflow execution
#[derive(Debug, Clone, serde::Serialize)]
pub struct NurturingWorkflowResult {
    /// Offering ID
    pub offering_id: String,
    /// Offering name
    pub offering_name: String,
    /// Local snapshot result
    pub local_snapshot: Option<NurturingResult>,
    /// Remote replication results (one per attempted seed bank)
    pub replications: Vec<ReplicationAttempt>,
    /// Overall success status
    pub success: bool,
    /// Human-readable summary
    pub summary: String,
}

/// Result of a replication attempt to a seed bank
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReplicationAttempt {
    /// Seed bank name
    pub seed_bank_name: String,
    /// Seed bank ID
    pub seed_bank_id: String,
    /// Whether this attempt succeeded
    pub success: bool,
    /// Replication result (if successful)
    pub result: Option<ReplicationResult>,
    /// Error message (if failed)
    pub error: Option<String>,
}

/// Seed bank routing strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum RoutingStrategy {
    /// Use the first available seed bank
    #[default]
    First,
    /// Use the seed bank with most available capacity
    MostCapacity,
    /// Use all available seed banks (redundant backup)
    All,
}

/// Configuration for the nurturing workflow
#[derive(Debug, Clone)]
pub struct NurturingWorkflowConfig {
    /// Whether to commit container image during snapshot
    pub commit_image: bool,
    /// Routing strategy for seed bank selection
    pub routing_strategy: RoutingStrategy,
    /// Maximum number of replication attempts (for failover)
    pub max_replication_attempts: usize,
    /// Whether to continue if local snapshot fails
    pub continue_on_local_failure: bool,
}

impl Default for NurturingWorkflowConfig {
    fn default() -> Self {
        Self {
            commit_image: true,
            routing_strategy: RoutingStrategy::First,
            max_replication_attempts: 3,
            continue_on_local_failure: false,
        }
    }
}

impl NurturingWorkflowConfig {
    /// Create config for testing (no image commit)
    pub fn for_testing() -> Self {
        Self {
            commit_image: false,
            routing_strategy: RoutingStrategy::First,
            max_replication_attempts: 1,
            continue_on_local_failure: true,
        }
    }
}

/// Nurturing scheduler that executes the full backup workflow
pub struct NurturingScheduler {
    /// Reference to app state
    state: AppState,
    /// Workflow configuration
    config: NurturingWorkflowConfig,
}

impl NurturingScheduler {
    /// Create a new scheduler with the given state
    pub fn new(state: AppState) -> Self {
        Self::with_config(state, NurturingWorkflowConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(state: AppState, config: NurturingWorkflowConfig) -> Self {
        Self { state, config }
    }

    /// Execute the full nurturing workflow for an offering
    ///
    /// This is the main entry point called by the timer trigger endpoint.
    ///
    /// # Workflow
    /// 1. Look up offering by name to get offering_id
    /// 2. Create local A/B snapshot
    /// 3. Find available seed banks
    /// 4. Replicate to seed bank(s) based on routing strategy
    /// 5. Return aggregated result
    pub async fn execute(&self, offering_name: &str) -> Result<NurturingWorkflowResult> {
        tracing::info!(
            offering = offering_name,
            strategy = ?self.config.routing_strategy,
            "Starting nurturing workflow"
        );

        // Look up offering to get offering_id
        let offering_entry = {
            let offerings = self.state.offerings.read().await;
            offerings
                .iter()
                .find(|o| o.name == offering_name || o.offering_id == offering_name)
                .cloned()
                .ok_or_else(|| {
                    anyhow::anyhow!("Offering '{}' not found in registry", offering_name)
                })?
        };
        let offering_id = offering_entry.offering_id.clone();
        let actual_name = offering_entry.name.clone();

        // Phase 1: Local snapshot
        let local_result = self.create_local_snapshot(&offering_id, &actual_name).await;

        let local_snapshot = match local_result {
            Ok(result) => {
                tracing::info!(
                    offering = actual_name,
                    slot = %result.slot,
                    harvest_id = %result.harvest_id,
                    "Local snapshot created"
                );
                Some(result)
            }
            Err(e) => {
                tracing::error!(
                    offering = actual_name,
                    error = ?e,
                    "Failed to create local snapshot"
                );
                if !self.config.continue_on_local_failure {
                    return Ok(NurturingWorkflowResult {
                        offering_id,
                        offering_name: actual_name,
                        local_snapshot: None,
                        replications: Vec::new(),
                        success: false,
                        summary: format!("Local snapshot failed: {}", e),
                    });
                }
                None
            }
        };

        // Phase 2: Find seed banks and replicate
        let replications = self.replicate_to_seed_banks(&offering_entry).await;

        // Determine overall success
        let local_success = local_snapshot.is_some();
        let replication_success = replications.iter().any(|r| r.success);
        let success = local_success && (replications.is_empty() || replication_success);

        let summary = self.build_summary(&actual_name, local_success, &replications);

        tracing::info!(
            offering = actual_name,
            success,
            summary = %summary,
            "Nurturing workflow completed"
        );

        Ok(NurturingWorkflowResult {
            offering_id,
            offering_name: actual_name,
            local_snapshot,
            replications,
            success,
            summary,
        })
    }

    /// Create local A/B snapshot
    async fn create_local_snapshot(
        &self,
        offering_id: &str,
        offering_name: &str,
    ) -> Result<NurturingResult> {
        self.state
            .nurturing_store
            .create_snapshot(
                &self.state.docker,
                offering_id,
                offering_name,
                &self.state.stone_id,
                self.config.commit_image,
            )
            .await
            .context("Failed to create local nurturing snapshot")
    }

    /// Replicate to seed banks based on routing strategy
    async fn replicate_to_seed_banks(&self, offering: &Offering) -> Vec<ReplicationAttempt> {
        // Get available seed banks
        let seed_banks = match self.find_available_seed_banks().await {
            Ok(banks) if banks.is_empty() => {
                tracing::debug!(
                    offering = offering.name,
                    "No seed banks available for replication"
                );
                return Vec::new();
            }
            Ok(banks) => banks,
            Err(e) => {
                tracing::warn!(
                    offering = offering.name,
                    error = ?e,
                    "Failed to find seed banks"
                );
                return Vec::new();
            }
        };

        // Select seed banks based on routing strategy + role awareness
        let targets = self.select_targets(&seed_banks).await;

        tracing::debug!(
            offering = offering.name,
            target_count = targets.len(),
            available_count = seed_banks.len(),
            strategy = ?self.config.routing_strategy,
            "Selected seed bank targets"
        );

        let manifest = self
            .state
            .manifest_registry
            .get_offering(&offering.offering)
            .cloned();
        let hydration_manifest = build_memories_manifest(
            offering,
            manifest,
            &self.state.stone_id,
            &self.state.stone_name,
        );

        // Attempt replication to each target
        let mut attempts = Vec::new();
        let mut successful_replications = 0;

        for seed_bank in targets {
            if successful_replications >= self.config.max_replication_attempts
                && self.config.routing_strategy != RoutingStrategy::All
            {
                break;
            }

            let attempt = self
                .attempt_replication(offering, &seed_bank, &hydration_manifest)
                .await;

            if attempt.success {
                successful_replications += 1;
            }

            attempts.push(attempt);
        }

        attempts
    }

    /// Find available seed banks from local registry
    ///
    /// Scans mounted seed banks and returns those that are online.
    /// For remote seed banks (via S3 gateway), use the storage_cache
    /// routing separately.
    async fn find_available_seed_banks(&self) -> Result<Vec<SeedBankInfo>> {
        let local_registry = SeedBankRegistry::scan().await?;

        // Get all local seed banks that are online
        let seed_banks: Vec<SeedBankInfo> = local_registry
            .list()
            .into_iter()
            .filter(|sb| sb.online)
            .cloned()
            .collect();

        Ok(seed_banks)
    }

    /// Select target seed banks based on routing strategy
    ///
    /// Filters out Dormant replicas whose Primary is elsewhere (STORAGE-0006).
    /// Only local Primary banks are eligible write targets. When a logical name
    /// has no local Primary, the seed bank is skipped (remote write support is
    /// Phase 3b).
    ///
    /// STORAGE-0007: Uses lifecycle objects for role lookup.
    async fn select_targets(&self, seed_banks: &[SeedBankInfo]) -> Vec<SeedBankInfo> {
        let lifecycle_banks = self.state.seed_banks.read().await;

        let primary_banks: Vec<SeedBankInfo> = seed_banks
            .iter()
            .filter(|sb| {
                let role = lifecycle_banks
                    .get(&sb.id)
                    .map(|b| b.role)
                    .unwrap_or(SeedBankRole::Primary);
                if role == SeedBankRole::Dormant {
                    tracing::debug!(
                        seed_bank = %sb.name,
                        id = %sb.id,
                        "Skipping dormant seed bank — writes route to primary"
                    );
                    false
                } else {
                    true
                }
            })
            .cloned()
            .collect();

        if primary_banks.is_empty() && !seed_banks.is_empty() {
            // All local banks are Dormant — primary is on a remote stone
            tracing::info!(
                dormant_count = seed_banks.len(),
                "All local seed banks are dormant — remote write not yet supported"
            );
        }

        // Apply routing strategy to the filtered (Primary-only) set
        match self.config.routing_strategy {
            RoutingStrategy::First => primary_banks.first().cloned().into_iter().collect(),
            RoutingStrategy::MostCapacity => {
                let mut sorted = primary_banks;
                sorted.sort_by_key(|sb| {
                    std::cmp::Reverse(sb.capacity_bytes.saturating_sub(sb.used_bytes))
                });
                sorted.into_iter().take(1).collect()
            }
            RoutingStrategy::All => primary_banks,
        }
    }

    /// Attempt replication to a single seed bank
    async fn attempt_replication(
        &self,
        offering: &Offering,
        seed_bank: &SeedBankInfo,
        hydration_manifest: &garden_common::storage::MemoriesOfferingManifest,
    ) -> ReplicationAttempt {
        tracing::debug!(
            offering = offering.name,
            seed_bank = %seed_bank.name,
            "Attempting replication"
        );

        // STORAGE-0007: prefer store from lifecycle object; fall back to ad-hoc
        let store = {
            let banks = self.state.seed_banks.read().await;
            banks
                .get(&seed_bank.id)
                .map(|b| b.store.clone())
                .unwrap_or_else(|| {
                    crate::infra::storage::SeedBankStore::new_public(&seed_bank.mount_path)
                })
        };

        let result = self
            .state
            .nurturing_store
            .replicate_to_seed_bank(
                &offering.offering_id,
                &store,
                &seed_bank.id,
                &seed_bank.name,
                &self.state.stone_id,
                Some(hydration_manifest.clone()),
            )
            .await;

        match result {
            Ok(replication_result) => {
                tracing::info!(
                    offering = offering.name,
                    seed_bank = %seed_bank.name,
                    size = replication_result.size_bytes,
                    pruned = replication_result.pruned_harvest_ids.len(),
                    "Replication succeeded"
                );
                ReplicationAttempt {
                    seed_bank_name: seed_bank.name.clone(),
                    seed_bank_id: seed_bank.id.clone(),
                    success: true,
                    result: Some(replication_result),
                    error: None,
                }
            }
            Err(e) => {
                tracing::warn!(
                    offering = offering.name,
                    seed_bank = %seed_bank.name,
                    error = ?e,
                    "Replication failed"
                );
                ReplicationAttempt {
                    seed_bank_name: seed_bank.name.clone(),
                    seed_bank_id: seed_bank.id.clone(),
                    success: false,
                    result: None,
                    error: Some(e.to_string()),
                }
            }
        }
    }

    /// Build human-readable summary
    fn build_summary(
        &self,
        offering_name: &str,
        local_success: bool,
        replications: &[ReplicationAttempt],
    ) -> String {
        let mut parts = Vec::new();

        if local_success {
            parts.push(format!("{}: local snapshot created", offering_name));
        } else {
            parts.push(format!("{}: local snapshot failed", offering_name));
        }

        let successful_replications: Vec<_> = replications.iter().filter(|r| r.success).collect();
        let failed_replications: Vec<_> = replications.iter().filter(|r| !r.success).collect();

        if !successful_replications.is_empty() {
            let names: Vec<_> = successful_replications
                .iter()
                .map(|r| r.seed_bank_name.as_str())
                .collect();
            parts.push(format!("replicated to: {}", names.join(", ")));
        }

        if !failed_replications.is_empty() {
            let names: Vec<_> = failed_replications
                .iter()
                .map(|r| r.seed_bank_name.as_str())
                .collect();
            parts.push(format!("replication failed: {}", names.join(", ")));
        }

        if replications.is_empty() {
            parts.push("no seed banks available".to_string());
        }

        parts.join("; ")
    }
}

/// Trigger nurturing workflow for a specific offering
///
/// This is the function called by the API endpoint when a timer fires.
pub async fn trigger_nurturing(
    state: &AppState,
    offering_name: &str,
) -> Result<NurturingWorkflowResult> {
    let scheduler = NurturingScheduler::new(state.clone());
    scheduler.execute(offering_name).await
}

/// Trigger nurturing for all running offerings
///
/// Used for batch nurturing or testing.
pub async fn trigger_all_nurturing(state: &AppState) -> Vec<NurturingWorkflowResult> {
    let offerings: Vec<String> = {
        let offerings = state.offerings.read().await;
        offerings
            .iter()
            .filter(|o| o.status == garden_common::OfferingStatus::Running)
            .map(|o| o.name.clone())
            .collect()
    };

    let mut results = Vec::new();
    let scheduler = NurturingScheduler::new(state.clone());

    for offering in offerings {
        match scheduler.execute(&offering).await {
            Ok(result) => results.push(result),
            Err(e) => {
                tracing::error!(
                    offering = %offering,
                    error = ?e,
                    "Failed to execute nurturing workflow"
                );
            }
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_routing_strategy_default() {
        assert_eq!(RoutingStrategy::default(), RoutingStrategy::First);
    }

    #[test]
    fn test_workflow_config_default() {
        let config = NurturingWorkflowConfig::default();
        assert!(config.commit_image);
        assert_eq!(config.routing_strategy, RoutingStrategy::First);
        assert_eq!(config.max_replication_attempts, 3);
        assert!(!config.continue_on_local_failure);
    }

    #[test]
    fn test_workflow_config_for_testing() {
        let config = NurturingWorkflowConfig::for_testing();
        assert!(!config.commit_image);
        assert_eq!(config.max_replication_attempts, 1);
        assert!(config.continue_on_local_failure);
    }

    #[test]
    fn test_replication_attempt_serialization() {
        let attempt = ReplicationAttempt {
            seed_bank_name: "backup-drive".to_string(),
            seed_bank_id: "sb-123".to_string(),
            success: true,
            result: None,
            error: None,
        };

        let json = serde_json::to_string(&attempt).unwrap();
        assert!(json.contains("backup-drive"));
        assert!(json.contains("sb-123"));
    }

    #[test]
    fn test_workflow_result_serialization() {
        let result = NurturingWorkflowResult {
            offering_id: "offer-123".to_string(),
            offering_name: "mongodb".to_string(),
            local_snapshot: None,
            replications: Vec::new(),
            success: true,
            summary: "Test summary".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("mongodb"));
        assert!(json.contains("offer-123"));
    }

    #[test]
    fn test_routing_strategy_serialization() {
        let first = RoutingStrategy::First;
        let json = serde_json::to_string(&first).unwrap();
        assert_eq!(json, "\"first\"");

        let most_capacity = RoutingStrategy::MostCapacity;
        let json = serde_json::to_string(&most_capacity).unwrap();
        assert_eq!(json, "\"most_capacity\"");

        let all = RoutingStrategy::All;
        let json = serde_json::to_string(&all).unwrap();
        assert_eq!(json, "\"all\"");
    }
}
