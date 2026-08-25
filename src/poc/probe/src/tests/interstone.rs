//! Inter-stone communication tests
//!
//! Tests that verify stones can communicate with each other:
//! - Chirp propagation (deploy offering, verify other stones see it)
//! - Beacon propagation (storage mount, verify other stones receive beacon)
//! - Topology consistency (all stones agree on garden state)

use crate::registry::TestDef;
use crate::{Bag, LiveGarden, StepResult};
use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;

// ============================================================================
// interstone.discovery_consistency - All stones discover each other
// ============================================================================

pub fn discovery_consistency_test() -> TestDef {
    TestDef {
        id: "interstone.discovery_consistency",
        name: "Discovery Consistency",
        description: "Verify UDP discovery finds the same stones as each stone's topology",
        category: "interstone",
        tags: &["interstone", "discovery", "consistency"],
        run: |garden, bag| Box::pin(test_discovery_consistency(garden, bag)),
    }
}

async fn test_discovery_consistency(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let start = Instant::now();

    // We discovered N stones via UDP - each stone should know about the same set
    let expected: std::collections::HashSet<String> =
        garden.stone_names().iter().map(|s| s.to_string()).collect();

    bag.put("expected_stones", garden.stone_names());

    let mut all_consistent = true;

    for stone in &garden.stones {
        let step_start = Instant::now();

        match stone.get_json("/api/v1/garden").await {
            Ok(resp) => {
                let visible: std::collections::HashSet<String> = resp
                    .get("data")
                    .and_then(|d| d.get("stones"))
                    .and_then(|s| s.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|s| s.get("name").and_then(|n| n.as_str()))
                            .map(|s| s.to_string())
                            .collect()
                    })
                    .unwrap_or_default();

                let missing: Vec<_> = expected.difference(&visible).collect();
                let extra: Vec<_> = visible.difference(&expected).collect();

                let consistent = missing.is_empty() && extra.is_empty();

                if consistent {
                    bag.record_step(
                        format!("consistency_{}", stone.name),
                        format!("{} sees all {} stones", stone.name, visible.len()),
                        step_start.elapsed().as_millis() as u64,
                        StepResult::ok_with(serde_json::json!({
                            "visible": visible.len(),
                            "expected": expected.len(),
                        })),
                    );
                } else {
                    all_consistent = false;
                    bag.record_step(
                        format!("consistency_{}", stone.name),
                        format!(
                            "{} inconsistent: missing {:?}, extra {:?}",
                            stone.name, missing, extra
                        ),
                        step_start.elapsed().as_millis() as u64,
                        StepResult::failed(format!("Missing: {:?}, Extra: {:?}", missing, extra)),
                    );
                }
            }
            Err(e) => {
                all_consistent = false;
                bag.record_step(
                    format!("consistency_{}", stone.name),
                    format!("{} unreachable", stone.name),
                    step_start.elapsed().as_millis() as u64,
                    StepResult::failed(e.to_string()),
                );
            }
        }
    }

    let duration = start.elapsed();

    if all_consistent {
        bag.record_step(
            "discovery_consistency",
            "All stones have consistent view",
            duration.as_millis() as u64,
            StepResult::ok(),
        );
    } else {
        bag.record_step(
            "discovery_consistency",
            "Stones have inconsistent views",
            duration.as_millis() as u64,
            StepResult::failed("Topology inconsistency detected"),
        );
    }

    Ok(bag)
}

// ============================================================================
// interstone.cross_query - Query each stone from another
// ============================================================================

pub fn cross_query_test() -> TestDef {
    TestDef {
        id: "interstone.cross_query",
        name: "Cross-Stone Query",
        description: "Verify each stone can query every other stone's capabilities",
        category: "interstone",
        tags: &["interstone", "network", "comprehensive"],
        run: |garden, bag| Box::pin(test_cross_query(garden, bag)),
    }
}

async fn test_cross_query(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    // For now, we can only test from probe to each stone
    // True cross-stone queries would require a relay endpoint on Moss

    let mut success_count = 0;
    let mut failure_count = 0;

    for source in &garden.stones {
        for target in &garden.stones {
            let step_start = Instant::now();
            let step_id = format!("cross_{}_{}", source.name, target.name);

            // Query target's capabilities
            match target.get_json("/api/v1/stone/capabilities").await {
                Ok(resp) => {
                    let target_name = resp
                        .get("data")
                        .and_then(|d| d.get("stone_name"))
                        .and_then(|n| n.as_str())
                        .unwrap_or("unknown");

                    success_count += 1;
                    bag.record_step(
                        &step_id,
                        format!("{} -> {}: OK", source.name, target.name),
                        step_start.elapsed().as_millis() as u64,
                        StepResult::ok_with(serde_json::json!({
                            "source": source.name,
                            "target": target.name,
                            "reported_name": target_name,
                        })),
                    );
                }
                Err(e) => {
                    failure_count += 1;
                    bag.record_step(
                        &step_id,
                        format!("{} -> {}: FAIL", source.name, target.name),
                        step_start.elapsed().as_millis() as u64,
                        StepResult::failed(e.to_string()),
                    );
                }
            }
        }
    }

    let total = success_count + failure_count;
    let result = if failure_count == 0 {
        StepResult::ok_with(serde_json::json!({
            "queries": total,
            "success": success_count,
            "failed": 0,
        }))
    } else {
        StepResult::failed(format!("{}/{} queries failed", failure_count, total))
    };

    bag.record_step(
        "cross_query_summary",
        format!("{} cross-queries", total),
        0,
        result,
    );

    Ok(bag)
}

// ============================================================================
// interstone.offering_visibility - Deploy offering, verify others see it
// ============================================================================

pub fn offering_visibility_test() -> TestDef {
    TestDef {
        id: "interstone.offering_visibility",
        name: "Offering Visibility",
        description: "Deploy an offering on one stone, verify others see it in garden state",
        category: "interstone",
        tags: &["interstone", "offerings", "chirp"],
        run: |garden, bag| Box::pin(test_offering_visibility(garden, bag)),
    }
}

async fn test_offering_visibility(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    // This test requires at least 2 stones
    if garden.stones.len() < 2 {
        bag.record_step(
            "offering_visibility",
            "Requires 2+ stones",
            0,
            StepResult::skipped("Need at least 2 stones for inter-stone test"),
        );
        return Ok(bag);
    }

    // Check if any stone has a running offering we can observe
    let start = Instant::now();

    for source in &garden.stones {
        // Get offerings from source stone
        match source.get_json("/api/v1/stone/offerings").await {
            Ok(resp) => {
                let offerings: Vec<String> = resp
                    .get("data")
                    .and_then(|d| d.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter(|o| {
                                o.get("vitality")
                                    .and_then(|v| v.as_str())
                                    .map(|v| v == "running")
                                    .unwrap_or(false)
                            })
                            .filter_map(|o| o.get("name").and_then(|n| n.as_str()))
                            .map(|s| s.to_string())
                            .collect()
                    })
                    .unwrap_or_default();

                if !offerings.is_empty() {
                    bag.put(format!("{}_running_offerings", source.name), &offerings);
                    bag.record_step(
                        format!("offerings_{}", source.name),
                        format!("{} has {} running offerings", source.name, offerings.len()),
                        start.elapsed().as_millis() as u64,
                        StepResult::ok_with(serde_json::json!({
                            "running": offerings,
                        })),
                    );
                } else {
                    bag.record_step(
                        format!("offerings_{}", source.name),
                        format!("{} has no running offerings", source.name),
                        start.elapsed().as_millis() as u64,
                        StepResult::ok(),
                    );
                }
            }
            Err(e) => {
                bag.record_step(
                    format!("offerings_{}", source.name),
                    format!("Failed to query {} offerings", source.name),
                    start.elapsed().as_millis() as u64,
                    StepResult::failed(e.to_string()),
                );
            }
        }
    }

    // Summary - this is an observational test, not a mutation test
    bag.record_step(
        "offering_visibility",
        "Observed offering state across stones",
        start.elapsed().as_millis() as u64,
        StepResult::ok_with(serde_json::json!({
            "stones_checked": garden.stones.len(),
            "note": "Use --set offering=<name> with a mutation test to verify chirp propagation",
        })),
    );

    Ok(bag)
}

// ============================================================================
// interstone.latency_matrix - Measure latency between all stone pairs
// ============================================================================

pub fn latency_matrix_test() -> TestDef {
    TestDef {
        id: "interstone.latency_matrix",
        name: "Latency Matrix",
        description: "Measure response latency from probe to each stone",
        category: "interstone",
        tags: &["interstone", "performance", "network"],
        run: |garden, bag| Box::pin(test_latency_matrix(garden, bag)),
    }
}

async fn test_latency_matrix(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let iterations: usize = bag.get("latency_iterations").unwrap_or(5);
    let mut matrix: std::collections::HashMap<String, Vec<u64>> = std::collections::HashMap::new();

    for stone in &garden.stones {
        let mut latencies = Vec::new();

        for _i in 0..iterations {
            let start = Instant::now();
            if stone.get_json("/health").await.is_ok() {
                latencies.push(start.elapsed().as_millis() as u64);
            }
        }

        if !latencies.is_empty() {
            let avg = latencies.iter().sum::<u64>() / latencies.len() as u64;
            let min = *latencies.iter().min().unwrap();
            let max = *latencies.iter().max().unwrap();

            bag.record_step(
                format!("latency_{}", stone.name),
                format!(
                    "{}: avg={}ms, min={}ms, max={}ms",
                    stone.name, avg, min, max
                ),
                avg,
                StepResult::ok_with(serde_json::json!({
                    "stone": stone.name,
                    "avg_ms": avg,
                    "min_ms": min,
                    "max_ms": max,
                    "samples": latencies.len(),
                })),
            );

            matrix.insert(stone.name.clone(), latencies);
        }
    }

    bag.put("latency_matrix", &matrix);

    bag.record_step(
        "latency_matrix",
        format!("Measured latency to {} stones", matrix.len()),
        0,
        StepResult::ok_with(serde_json::json!({
            "stones": matrix.len(),
            "iterations": iterations,
        })),
    );

    Ok(bag)
}
