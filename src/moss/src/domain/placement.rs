//! Placement recommendation orchestration
//!
//! Coordinates topology discovery, resource collection, compatibility checking,
//! and scoring to recommend optimal stone placement for offerings.

use anyhow::{Context, Result};
use chrono::Utc;
use std::time::Duration;

use crate::domain::{
    compatibility, resources_collection, scoring, services, topology, CompiledOffering,
};
use crate::AppState;
use garden_common::TopologyEntry;

/// Placement request from client
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PlacementRequest {
    pub offering: String,
    #[serde(default)]
    pub preferences: Vec<String>,
    #[serde(default = "default_top_n")]
    pub top_n: usize,
}

fn default_top_n() -> usize {
    3
}

/// Placement response with ranked recommendations
#[derive(Debug, Clone, serde::Serialize)]
pub struct PlacementResponse {
    pub recommendations: Vec<PlacementRecommendation>,
    pub evaluated_stones: usize,
    pub timestamp: String,
    /// Summary of excluded stones (if any), e.g., "2 stones excluded: offering not available"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclusion_summary: Option<String>,
}

/// Single placement recommendation
#[derive(Debug, Clone, serde::Serialize)]
pub struct PlacementRecommendation {
    pub stone_id: String,
    pub hostname: String,
    pub score: i32,
    pub is_local: bool,
    pub compatibility: String, // "compatible" | "fallback" | "incompatible"
    pub metrics: PlacementMetrics,
    pub services_count: usize,
    pub breakdown: ScoreBreakdown,
}

/// Metrics included in recommendation
#[derive(Debug, Clone, serde::Serialize)]
pub struct PlacementMetrics {
    pub memory_free_mb: u64,
    pub memory_total_mb: u64,
    pub cpu_load_percent: u8,
    pub storage_free_gb: u64,
    pub storage_total_gb: u64,
    pub storage_type: String,
}

/// Detailed scoring breakdown for transparency
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScoreBreakdown {
    pub compatibility: i32,
    pub memory: i32,
    pub cpu: i32,
    pub storage: i32,
    pub hardware: i32,
    pub distribution: i32,
    pub tended_bonus: i32,
}

/// Recommend placement for an offering
///
/// Main orchestration function that:
/// 1. Evaluates tended stone (zero latency)
/// 2. Discovers peer stones from topology cache
/// 3. Fetches resources AND offerings from all stones in parallel
/// 4. Scores each stone using multi-factor algorithm with full compatibility
/// 5. Returns top N recommendations sorted by score
pub async fn recommend_placement(
    request: PlacementRequest,
    state: &AppState,
) -> Result<PlacementResponse> {
    let start_time = std::time::Instant::now();

    // Get local compiled offering with compatibility
    let offerings_index = state.offerings_index.read().await;
    let local_offering = offerings_index
        .as_ref()
        .and_then(|idx| idx.offerings.iter().find(|o| o.name == request.offering))
        .ok_or_else(|| anyhow::anyhow!("Offering '{}' not found on local stone", request.offering))?
        .clone();
    drop(offerings_index);

    // 1. Evaluate tended stone first (zero latency)
    let local_candidate = score_local_stone(&request.offering, &local_offering, state).await?;

    // 2. Get peer stones from topology cache (online only — offline stones are unreachable)
    let peer_stones = topology::get_online_stones(&state.current.topology.cache).await;
    tracing::debug!(
        peer_count = peer_stones.len(),
        "Discovered {} peer stones from topology cache",
        peer_stones.len()
    );

    // 3. Fetch resources AND offerings from peers in parallel (with timeout)
    let timeout = Duration::from_secs(3);
    let endpoints: Vec<String> = peer_stones.iter().map(|s| s.address.http_base()).collect();

    let resources_results = resources_collection::fetch_resources_batch(endpoints.clone(), timeout).await;
    let offerings_results = fetch_offerings_batch(endpoints, timeout).await;

    // 4. Score each peer stone with full compatibility checking
    let mut all_candidates = vec![local_candidate];

    // Track exclusion reasons for summary
    let mut excluded_no_offering = 0usize;
    let mut excluded_resources_failed = 0usize;
    let mut excluded_offerings_failed = 0usize;

    for ((stone, resources_result), offerings_result) in peer_stones
        .iter()
        .zip(resources_results.iter())
        .zip(offerings_results.iter())
    {
        match (resources_result, offerings_result) {
            (Ok(stone_resources), Ok(offerings)) => {
                // Find the offering on remote stone
                match offerings.iter().find(|o| o.name == request.offering) {
                    Some(remote_offering) => {
                        match score_remote_stone(
                            stone,
                            &request.offering,
                            remote_offering,
                            stone_resources,
                            state,
                        )
                        .await
                        {
                            Ok(candidate) => all_candidates.push(candidate),
                            Err(e) => {
                                tracing::warn!(
                                    stone_id = %stone.stone_id,
                                    error = ?e,
                                    "Failed to score stone"
                                );
                            }
                        }
                    }
                    None => {
                        excluded_no_offering += 1;
                        tracing::debug!(
                            stone_id = %stone.stone_id,
                            offering = %request.offering,
                            "Offering not available on remote stone"
                        );
                    }
                }
            }
            (Err(e), _) => {
                excluded_resources_failed += 1;
                tracing::warn!(
                    stone_id = %stone.stone_id,
                    error = ?e,
                    "Failed to fetch resources from stone"
                );
            }
            (_, Err(e)) => {
                excluded_offerings_failed += 1;
                tracing::warn!(
                    stone_id = %stone.stone_id,
                    error = ?e,
                    "Failed to fetch offerings from stone"
                );
            }
        }
    }

    // Build exclusion summary
    let exclusion_summary = build_exclusion_summary(
        excluded_no_offering,
        excluded_resources_failed,
        excluded_offerings_failed,
    );

    // 5. Filter incompatible stones (score < -100)
    all_candidates.retain(|c| c.score > -100);

    if all_candidates.is_empty() {
        anyhow::bail!(
            "No compatible stones found for offering '{}'",
            request.offering
        );
    }

    // 6. Sort by score DESC
    all_candidates.sort_by(|a, b| b.score.cmp(&a.score));

    // 7. Return top N
    let top_n = request.top_n.min(all_candidates.len());
    let recommendations = all_candidates.into_iter().take(top_n).collect();

    let elapsed = start_time.elapsed();
    tracing::info!(
        offering = %request.offering,
        evaluated = peer_stones.len() + 1,
        duration_ms = elapsed.as_millis(),
        "Placement recommendation completed"
    );

    Ok(PlacementResponse {
        recommendations,
        evaluated_stones: peer_stones.len() + 1,
        timestamp: Utc::now().to_rfc3339(),
        exclusion_summary,
    })
}

/// Build human-readable exclusion summary
fn build_exclusion_summary(
    no_offering: usize,
    resources_failed: usize,
    offerings_failed: usize,
) -> Option<String> {
    let total = no_offering + resources_failed + offerings_failed;
    if total == 0 {
        return None;
    }

    let mut reasons = Vec::new();
    if no_offering > 0 {
        reasons.push(format!("{} offering not available", no_offering));
    }
    if resources_failed > 0 {
        reasons.push(format!("{} unreachable", resources_failed));
    }
    if offerings_failed > 0 {
        reasons.push(format!("{} offerings fetch failed", offerings_failed));
    }

    let stone_word = if total == 1 { "stone" } else { "stones" };
    Some(format!(
        "{} {} excluded: {}",
        total,
        stone_word,
        reasons.join(", ")
    ))
}

/// Score the tended stone (local)
async fn score_local_stone(
    _offering_id: &str,
    offering: &CompiledOffering,
    state: &AppState,
) -> Result<PlacementRecommendation> {
    // Get local resources (zero latency)
    let local_resources = resources_collection::get_local_resources()
        .context("Failed to collect local resources")?;

    // Get local service count
    let service_count = services::get_local_service_count(state).await.unwrap_or(0);

    // Use pre-compiled compatibility decision
    let compat_str = &offering.compatibility.decision;

    // Convert compatibility decision to enum for scoring
    let compat_decision = match compat_str.as_str() {
        "pass" => compatibility::CompatibilityDecision::Pass,
        "fallback" => compatibility::CompatibilityDecision::Fallback {
            image: offering
                .compatibility
                .fallback_image
                .clone()
                .unwrap_or_default(),
            name: offering.compatibility.fallback_name.clone(),
            reason: offering.compatibility.reason.clone().unwrap_or_default(),
        },
        _ => compatibility::CompatibilityDecision::Fail {
            reason: offering
                .compatibility
                .reason
                .clone()
                .unwrap_or_else(|| "Incompatible".to_string()),
            suggestion: offering.compatibility.suggestion.clone(),
        },
    };

    // Calculate scores using reusable functions
    let compat_score = scoring::calculate_compatibility_penalty(&compat_decision);
    let memory_score = scoring::score_memory_headroom(
        local_resources.memory_free_mb,
        local_resources.memory_total_mb,
    );
    let cpu_score = scoring::score_cpu_availability(local_resources.cpu_load_percent);
    let storage_capacity_score = scoring::score_storage_capacity(local_resources.storage_free_gb);
    let storage_type_score = scoring::score_storage_type(&local_resources.storage_type);
    let distribution_score = scoring::calculate_distribution_penalty(service_count);
    let tended_bonus = 3; // Small bonus for local stone

    let total_score = compat_score
        + memory_score
        + cpu_score
        + storage_capacity_score
        + storage_type_score
        + distribution_score
        + tended_bonus;

    Ok(PlacementRecommendation {
        stone_id: state.current.stone.id.clone(),
        hostname: state.current.stone.name.clone(),
        score: total_score,
        is_local: true,
        compatibility: compat_str.to_string(),
        // Note: `metrics` (wire-format field name) is a deferred rename —
        // see docs/scaffolding.md § Deferred renames.
        metrics: PlacementMetrics {
            memory_free_mb: local_resources.memory_free_mb,
            memory_total_mb: local_resources.memory_total_mb,
            cpu_load_percent: local_resources.cpu_load_percent,
            storage_free_gb: local_resources.storage_free_gb,
            storage_total_gb: local_resources.storage_total_gb,
            storage_type: format!("{:?}", local_resources.storage_type),
        },
        services_count: service_count,
        breakdown: ScoreBreakdown {
            compatibility: compat_score,
            memory: memory_score,
            cpu: cpu_score,
            storage: storage_capacity_score + storage_type_score,
            hardware: storage_type_score,
            distribution: distribution_score,
            tended_bonus,
        },
    })
}

/// Score a remote stone
async fn score_remote_stone(
    stone: &TopologyEntry,
    _offering_id: &str,
    offering: &CompiledOffering,
    stone_resources: &resources_collection::NormalizedResources,
    _state: &AppState,
) -> Result<PlacementRecommendation> {
    // Get remote service count (with timeout)
    let service_count =
        services::fetch_remote_service_count(&stone.address.http_base(), Duration::from_secs(2))
            .await
            .unwrap_or(0);

    // Use remote stone's compiled compatibility decision
    let compat_str = &offering.compatibility.decision;

    // Convert compatibility decision to enum for scoring
    let compat_decision = match compat_str.as_str() {
        "pass" => compatibility::CompatibilityDecision::Pass,
        "fallback" => compatibility::CompatibilityDecision::Fallback {
            image: offering
                .compatibility
                .fallback_image
                .clone()
                .unwrap_or_default(),
            name: offering.compatibility.fallback_name.clone(),
            reason: offering.compatibility.reason.clone().unwrap_or_default(),
        },
        _ => compatibility::CompatibilityDecision::Fail {
            reason: offering
                .compatibility
                .reason
                .clone()
                .unwrap_or_else(|| "Incompatible".to_string()),
            suggestion: offering.compatibility.suggestion.clone(),
        },
    };

    // Calculate scores
    let compat_score = scoring::calculate_compatibility_penalty(&compat_decision);
    let memory_score = scoring::score_memory_headroom(
        stone_resources.memory_free_mb,
        stone_resources.memory_total_mb,
    );
    let cpu_score = scoring::score_cpu_availability(stone_resources.cpu_load_percent);
    let storage_capacity_score = scoring::score_storage_capacity(stone_resources.storage_free_gb);
    let storage_type_score = scoring::score_storage_type(&stone_resources.storage_type);
    let distribution_score = scoring::calculate_distribution_penalty(service_count);
    let tended_bonus = 0; // No bonus for remote stones

    let total_score = compat_score
        + memory_score
        + cpu_score
        + storage_capacity_score
        + storage_type_score
        + distribution_score
        + tended_bonus;

    Ok(PlacementRecommendation {
        stone_id: stone.stone_id.clone(),
        hostname: stone.stone_name.clone(),
        score: total_score,
        is_local: false,
        compatibility: compat_str.to_string(),
        // Note: `metrics` (wire-format field name) is a deferred rename —
        // see docs/scaffolding.md § Deferred renames.
        metrics: PlacementMetrics {
            memory_free_mb: stone_resources.memory_free_mb,
            memory_total_mb: stone_resources.memory_total_mb,
            cpu_load_percent: stone_resources.cpu_load_percent,
            storage_free_gb: stone_resources.storage_free_gb,
            storage_total_gb: stone_resources.storage_total_gb,
            storage_type: format!("{:?}", stone_resources.storage_type),
        },
        services_count: service_count,
        breakdown: ScoreBreakdown {
            compatibility: compat_score,
            memory: memory_score,
            cpu: cpu_score,
            storage: storage_capacity_score + storage_type_score,
            hardware: storage_type_score,
            distribution: distribution_score,
            tended_bonus,
        },
    })
}

/// Fetch offerings from multiple remote stones in parallel
async fn fetch_offerings_batch(
    endpoints: Vec<String>,
    timeout: Duration,
) -> Vec<Result<Vec<CompiledOffering>>> {
    let tasks: Vec<_> = endpoints
        .into_iter()
        .map(|endpoint| {
            tokio::spawn(async move { fetch_remote_offerings(&endpoint, timeout).await })
        })
        .collect();

    let mut results = Vec::new();
    for task in tasks {
        match task.await {
            Ok(result) => results.push(result),
            Err(e) => results.push(Err(anyhow::anyhow!("Task join error: {}", e))),
        }
    }

    results
}

/// Fetch offerings from a single remote stone
async fn fetch_remote_offerings(
    endpoint: &str,
    timeout: Duration,
) -> Result<Vec<CompiledOffering>> {
    let offerings_url = format!("{}/api/v1/offerings", endpoint.trim_end_matches('/'));
    let response = crate::http::HTTP
        .get(&offerings_url)
        .timeout(timeout)
        .send()
        .await
        .context("Failed to fetch offerings from remote stone")?;

    if !response.status().is_success() {
        anyhow::bail!("Remote stone returned error: {}", response.status());
    }

    // The endpoint returns ApiResponse<Vec<OfferingView>>, we need to extract the data
    #[derive(serde::Deserialize)]
    struct ApiResponse<T> {
        data: T,
    }

    #[derive(serde::Deserialize)]
    struct OfferingView {
        name: String,
        category: String,
        description: String,
        tags: Vec<String>,
        image: String,
        compatibility: Option<CompatibilityView>,
    }

    #[derive(serde::Deserialize)]
    struct CompatibilityView {
        decision: String,
        reason: Option<String>,
    }

    let api_response: ApiResponse<Vec<OfferingView>> = response
        .json()
        .await
        .context("Failed to parse offerings response")?;

    // Convert OfferingView to CompiledOffering
    let compiled_offerings: Vec<CompiledOffering> = api_response
        .data
        .into_iter()
        .map(|view| CompiledOffering {
            name: view.name,
            category: view.category,
            description: view.description,
            tags: view.tags,
            image: view.image,
            command: None,                           // Not included in OfferingView
            config_files: vec![],                    // Not included in OfferingView
            ports: std::collections::HashMap::new(), // Not included in OfferingView
            environment: vec![],                     // Not included in OfferingView
            volumes: vec![],                         // Not included in OfferingView
            compatibility: crate::domain::compatibility::CompiledCompatibility {
                decision: view
                    .compatibility
                    .as_ref()
                    .map(|c| c.decision.clone())
                    .unwrap_or_else(|| "pass".to_string()),
                reason: view.compatibility.and_then(|c| c.reason),
                original_image: None,
                fallback_image: None,
                fallback_name: None,
                suggestion: None,
            },
            tasks: std::collections::HashMap::new(), // Not included in OfferingView
            network: garden_common::manifests::NetworkRequirements::default(), // Not included in OfferingView
            coordination: garden_common::CoordinationMode::default(), // Remote views don't expose coordination
            device_requests: vec![], // Not included in OfferingView
        })
        .collect();

    Ok(compiled_offerings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_top_n() {
        assert_eq!(default_top_n(), 3);
    }

    #[test]
    fn test_placement_request_deserialize() {
        let json = r#"{"offering": "redis"}"#;
        let req: PlacementRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.offering, "redis");
        assert_eq!(req.top_n, 3);
        assert!(req.preferences.is_empty());
    }

    #[test]
    fn test_placement_request_with_top_n() {
        let json = r#"{"offering": "postgres", "top_n": 5}"#;
        let req: PlacementRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.offering, "postgres");
        assert_eq!(req.top_n, 5);
    }
}
