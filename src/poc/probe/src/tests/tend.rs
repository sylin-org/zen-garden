//! Tend tests - tending selection, fallback, and persistence
//!
//! Tests for the "tend" concept: which stone Rake talks to by default.
//!
//! **Concept**: Tending = the stone Rake cares for. Persists in `~/.zen-garden/.tending`
//! until user clears it, selects a different stone, or stone goes offline (auto-fallback).
//!
//! **Targets**:
//! - `this/local` - localhost:7185
//! - `auto` - broadcast discovery, tend first responder
//! - `another` - switch to different stone
//! - `http://ip:port` - explicit endpoint
//! - `stone-name` - resolve by name

use crate::registry::TestDef;
use crate::{Bag, LiveGarden, StepResult};
use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;

// ============================================================================
// tend.reachable - Tended stone is reachable
// ============================================================================

pub fn reachable_test() -> TestDef {
    TestDef {
        id: "tend.reachable",
        name: "Tended Stone Reachable",
        description: "Verify the tended stone is reachable and responds to health check",
        category: "tend",
        tags: &["tend", "quick", "health"],
        run: |garden, bag| Box::pin(test_reachable(garden, bag)),
    }
}

async fn test_reachable(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let start = Instant::now();

    let result = match &garden.tended {
        Some(tended) => {
            // Health check
            match tended.get_json("/health").await {
                Ok(resp) => {
                    let status = resp
                        .get("status")
                        .and_then(|s| s.as_str())
                        .unwrap_or("unknown");
                    bag.put("tended_status", status);

                    if status == "healthy" {
                        StepResult::ok_with(serde_json::json!({
                            "stone": tended.name,
                            "endpoint": tended.endpoint,
                            "status": status,
                        }))
                    } else {
                        StepResult::failed(format!("Tended stone health status: {}", status))
                    }
                }
                Err(e) => StepResult::failed(format!("Tended stone unreachable: {}", e)),
            }
        }
        None => StepResult::skipped("No tended stone to test"),
    };

    let duration = start.elapsed();
    bag.record_step(
        "tend_reachable",
        "Tended stone reachability",
        duration.as_millis() as u64,
        result,
    );

    Ok(bag)
}

// ============================================================================
// tend.capabilities - Tended stone reports capabilities
// ============================================================================

pub fn capabilities_test() -> TestDef {
    TestDef {
        id: "tend.capabilities",
        name: "Tended Stone Capabilities",
        description: "Verify the tended stone reports its capabilities correctly",
        category: "tend",
        tags: &["tend", "capabilities"],
        run: |garden, bag| Box::pin(test_capabilities(garden, bag)),
    }
}

async fn test_capabilities(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let start = Instant::now();

    let result = match &garden.tended {
        Some(tended) => {
            match tended.get_json("/api/v1/stone/capabilities").await {
                Ok(resp) => {
                    // Extract capabilities from response
                    let data = resp.get("data");
                    let stone_name = data
                        .and_then(|d| d.get("stone_name"))
                        .and_then(|n| n.as_str())
                        .unwrap_or("unknown");

                    let cpu_count = data
                        .and_then(|d| d.get("cpu_count"))
                        .and_then(|c| c.as_u64())
                        .unwrap_or(0);

                    let total_memory = data
                        .and_then(|d| d.get("total_memory"))
                        .and_then(|m| m.as_u64())
                        .unwrap_or(0);

                    bag.put("tended_stone_name", stone_name);
                    bag.put("tended_cpu_count", cpu_count);
                    bag.put("tended_memory", total_memory);

                    // Verify name matches
                    if stone_name == tended.name {
                        StepResult::ok_with(serde_json::json!({
                            "stone_name": stone_name,
                            "cpu_count": cpu_count,
                            "total_memory": total_memory,
                            "name_match": true,
                        }))
                    } else {
                        StepResult::failed(format!(
                            "Name mismatch: expected '{}', got '{}'",
                            tended.name, stone_name
                        ))
                    }
                }
                Err(e) => StepResult::failed(format!("Failed to get capabilities: {}", e)),
            }
        }
        None => StepResult::skipped("No tended stone to test"),
    };

    let duration = start.elapsed();
    bag.record_step(
        "tend_capabilities",
        "Tended stone capabilities",
        duration.as_millis() as u64,
        result,
    );

    Ok(bag)
}

// ============================================================================
// tend.alternatives - Other stones exist for fallback
// ============================================================================

pub fn alternatives_test() -> TestDef {
    TestDef {
        id: "tend.alternatives",
        name: "Alternative Stones Available",
        description: "Verify there are alternative stones available for fallback (tend another)",
        category: "tend",
        tags: &["tend", "fallback", "discovery"],
        run: |garden, bag| Box::pin(test_alternatives(garden, bag)),
    }
}

async fn test_alternatives(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let start = Instant::now();

    let total_stones = garden.stones.len();
    let alternatives: Vec<&str> = garden
        .stones
        .iter()
        .filter(|s| {
            garden
                .tended
                .as_ref()
                .map(|t| t.name != s.name)
                .unwrap_or(true)
        })
        .map(|s| s.name.as_str())
        .collect();

    let alternative_count = alternatives.len();
    bag.put("alternative_stones", &alternatives);
    bag.put("alternative_count", alternative_count);

    let result = if total_stones > 1 {
        if alternative_count > 0 {
            StepResult::ok_with(serde_json::json!({
                "total_stones": total_stones,
                "alternatives": alternatives,
                "count": alternative_count,
            }))
        } else {
            StepResult::failed("No alternatives found despite multiple stones")
        }
    } else {
        StepResult::skipped("Single stone garden - no alternatives possible")
    };

    let duration = start.elapsed();
    bag.record_step(
        "tend_alternatives",
        format!("Found {} alternative stones", alternative_count),
        duration.as_millis() as u64,
        result,
    );

    Ok(bag)
}

// ============================================================================
// tend.switch_simulation - Simulate switching to another stone
// ============================================================================

pub fn switch_simulation_test() -> TestDef {
    TestDef {
        id: "tend.switch_simulation",
        name: "Switch Stone Simulation",
        description: "Simulate 'tend another' by querying an alternative stone",
        category: "tend",
        tags: &["tend", "fallback", "simulation"],
        run: |garden, bag| Box::pin(test_switch_simulation(garden, bag)),
    }
}

async fn test_switch_simulation(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let start = Instant::now();

    // Find an alternative stone
    let alternative = garden.stones.iter().find(|s| {
        garden
            .tended
            .as_ref()
            .map(|t| t.name != s.name)
            .unwrap_or(true)
    });

    let result = match alternative {
        Some(alt) => {
            // Verify the alternative is reachable
            match alt.get_json("/health").await {
                Ok(resp) => {
                    let status = resp
                        .get("status")
                        .and_then(|s| s.as_str())
                        .unwrap_or("unknown");

                    if status == "healthy" {
                        bag.put("simulated_switch_to", &alt.name);
                        StepResult::ok_with(serde_json::json!({
                            "switched_to": alt.name,
                            "endpoint": alt.endpoint,
                            "status": status,
                            "note": "Simulation only - tending state not modified",
                        }))
                    } else {
                        StepResult::failed(format!("Alternative stone unhealthy: {}", status))
                    }
                }
                Err(e) => StepResult::failed(format!("Alternative stone unreachable: {}", e)),
            }
        }
        None => StepResult::skipped("No alternative stones available"),
    };

    let duration = start.elapsed();
    bag.record_step(
        "tend_switch_simulation",
        "Simulated switch to alternative stone",
        duration.as_millis() as u64,
        result,
    );

    Ok(bag)
}

// ============================================================================
// tend.all_healthy - All stones in garden are healthy (fallback pool)
// ============================================================================

pub fn all_healthy_test() -> TestDef {
    TestDef {
        id: "tend.all_healthy",
        name: "All Stones Healthy",
        description: "Verify all discovered stones are healthy (full fallback pool)",
        category: "tend",
        tags: &["tend", "health", "comprehensive"],
        run: |garden, bag| Box::pin(test_all_healthy(garden, bag)),
    }
}

async fn test_all_healthy(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let total = garden.stones.len();
    let mut healthy: Vec<String> = Vec::new();
    let mut unhealthy: Vec<String> = Vec::new();

    for stone in &garden.stones {
        let step_start = Instant::now();

        match stone.get_json("/health").await {
            Ok(resp) => {
                let status = resp
                    .get("status")
                    .and_then(|s| s.as_str())
                    .unwrap_or("unknown");
                if status == "healthy" {
                    healthy.push(stone.name.clone());
                    bag.record_step(
                        format!("health_{}", stone.name),
                        format!("{} is healthy", stone.name),
                        step_start.elapsed().as_millis() as u64,
                        StepResult::ok_with(serde_json::json!({"status": status})),
                    );
                } else {
                    unhealthy.push(stone.name.clone());
                    bag.record_step(
                        format!("health_{}", stone.name),
                        format!("{} unhealthy: {}", stone.name, status),
                        step_start.elapsed().as_millis() as u64,
                        StepResult::failed(format!("Status: {}", status)),
                    );
                }
            }
            Err(e) => {
                unhealthy.push(stone.name.clone());
                bag.record_step(
                    format!("health_{}", stone.name),
                    format!("{} unreachable", stone.name),
                    step_start.elapsed().as_millis() as u64,
                    StepResult::failed(e.to_string()),
                );
            }
        }
    }

    bag.put("healthy_stones", &healthy);
    bag.put("unhealthy_stones", &unhealthy);

    let result = if unhealthy.is_empty() {
        StepResult::ok_with(serde_json::json!({
            "total": total,
            "healthy": healthy,
            "all_healthy": true,
        }))
    } else {
        StepResult::failed(format!(
            "{}/{} stones unhealthy: {:?}",
            unhealthy.len(),
            total,
            unhealthy
        ))
    };

    bag.record_step(
        "all_healthy",
        format!("{}/{} stones healthy", healthy.len(), total),
        0, // Already tracked per-stone
        result,
    );

    Ok(bag)
}

// ============================================================================
// tend.round_robin - Query each stone in rotation (load distribution test)
// ============================================================================

pub fn round_robin_test() -> TestDef {
    TestDef {
        id: "tend.round_robin",
        name: "Round Robin Query",
        description: "Query each stone in rotation to simulate load distribution",
        category: "tend",
        tags: &["tend", "load", "simulation"],
        run: |garden, bag| Box::pin(test_round_robin(garden, bag)),
    }
}

async fn test_round_robin(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let iterations: usize = bag.get("round_robin_iterations").unwrap_or(3);
    let mut response_times: Vec<(String, u64)> = Vec::new();
    let mut failures: Vec<String> = Vec::new();

    for i in 0..iterations {
        for stone in &garden.stones {
            let start = Instant::now();
            let step_id = format!("rr_{}_{}", i + 1, stone.name);

            match stone.get_json("/health").await {
                Ok(_) => {
                    let duration = start.elapsed().as_millis() as u64;
                    response_times.push((stone.name.clone(), duration));
                    bag.record_step(
                        &step_id,
                        format!(
                            "Iteration {}: {} responded in {}ms",
                            i + 1,
                            stone.name,
                            duration
                        ),
                        duration,
                        StepResult::ok(),
                    );
                }
                Err(e) => {
                    failures.push(format!("{}:{}", stone.name, i + 1));
                    bag.record_step(
                        &step_id,
                        format!("Iteration {}: {} failed", i + 1, stone.name),
                        start.elapsed().as_millis() as u64,
                        StepResult::failed(e.to_string()),
                    );
                }
            }
        }
    }

    // Calculate average response times per stone
    let mut stone_averages: std::collections::HashMap<String, Vec<u64>> =
        std::collections::HashMap::new();
    for (stone, time) in &response_times {
        stone_averages.entry(stone.clone()).or_default().push(*time);
    }
    let averages: std::collections::HashMap<String, f64> = stone_averages
        .iter()
        .map(|(k, v)| (k.clone(), v.iter().sum::<u64>() as f64 / v.len() as f64))
        .collect();

    bag.put("response_averages", &averages);

    let result = if failures.is_empty() {
        StepResult::ok_with(serde_json::json!({
            "iterations": iterations,
            "total_queries": response_times.len(),
            "averages_ms": averages,
            "failures": 0,
        }))
    } else {
        StepResult::failed(format!("{} failures: {:?}", failures.len(), failures))
    };

    bag.record_step(
        "round_robin_summary",
        format!(
            "{} queries across {} stones",
            response_times.len(),
            garden.stones.len()
        ),
        0,
        result,
    );

    Ok(bag)
}
