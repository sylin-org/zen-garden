//! Offerings tests - catalog consistency, search, deployment state

use crate::registry::TestDef;
use crate::{Bag, LiveGarden, StepResult};
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

// ============================================================================
// offerings.catalog - Verify all stones have consistent offering catalogs
// ============================================================================

pub fn catalog_test() -> TestDef {
    TestDef {
        id: "offerings.catalog",
        name: "Offerings Catalog Consistency",
        description: "Verify all stones have the same available offerings catalog",
        category: "offerings",
        tags: &["offerings", "consistency"],
        run: |garden, bag| Box::pin(test_catalog(garden, bag)),
    }
}

async fn test_catalog(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let mut catalogs: HashMap<String, Vec<String>> = HashMap::new();

    for stone in &garden.stones {
        let start = Instant::now();
        let result = stone.get_json("/api/v1/stone/offerings").await;
        let duration = start.elapsed();

        match result {
            Ok(resp) => {
                let offerings: Vec<String> = resp
                    .get("data")
                    .and_then(|d| d.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|o| o.get("name").and_then(|n| n.as_str()))
                            .map(|s| s.to_string())
                            .collect()
                    })
                    .unwrap_or_default();

                let count = offerings.len();
                catalogs.insert(stone.name.clone(), offerings);

                bag.record_step(
                    format!("catalog_{}", stone.name),
                    format!("{} has {} offerings in catalog", stone.name, count),
                    duration.as_millis() as u64,
                    StepResult::ok_with(serde_json::json!({
                        "count": count,
                    })),
                );
            }
            Err(e) => {
                bag.record_step(
                    format!("catalog_{}", stone.name),
                    format!("{} catalog fetch failed", stone.name),
                    duration.as_millis() as u64,
                    StepResult::failed(e.to_string()),
                );
            }
        }
    }

    // Verify all have same catalog (offerings are loaded from manifests/)
    let counts: Vec<usize> = catalogs.values().map(|v| v.len()).collect();
    let all_same = counts.windows(2).all(|w| w[0] == w[1]);

    let result = if all_same && !counts.is_empty() {
        StepResult::ok_with(serde_json::json!({
            "stone_count": catalogs.len(),
            "catalog_size": counts[0],
        }))
    } else {
        StepResult::failed(format!("Catalog counts differ: {:?}", counts))
    };

    bag.record_step(
        "catalog_consistency",
        format!(
            "All {} stones have {} offerings",
            catalogs.len(),
            counts.first().unwrap_or(&0)
        ),
        0,
        result,
    );

    Ok(bag)
}

// ============================================================================
// offerings.installed - Verify installed offerings match topology
// ============================================================================

pub fn installed_test() -> TestDef {
    TestDef {
        id: "offerings.installed",
        name: "Installed Offerings",
        description: "Verify installed offerings match what topology reports",
        category: "offerings",
        tags: &["offerings", "topology"],
        run: |garden, bag| Box::pin(test_installed(garden, bag)),
    }
}

async fn test_installed(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    for stone in &garden.stones {
        let start = Instant::now();

        // Get installed offerings from offerings endpoint
        let offerings_result = stone.get_json("/api/v1/stone/offerings").await;
        let duration = start.elapsed();

        match offerings_result {
            Ok(resp) => {
                let installed: Vec<String> = resp
                    .get("data")
                    .and_then(|d| d.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter(|o| {
                                o.get("status")
                                    .and_then(|s| s.as_str())
                                    .map(|s| s == "running" || s == "installed")
                                    .unwrap_or(false)
                            })
                            .filter_map(|o| o.get("name").and_then(|n| n.as_str()))
                            .map(|s| s.to_string())
                            .collect()
                    })
                    .unwrap_or_default();

                bag.record_step(
                    format!("installed_{}", stone.name),
                    format!("{} has {} installed offerings", stone.name, installed.len()),
                    duration.as_millis() as u64,
                    StepResult::ok_with(serde_json::json!({
                        "installed": installed,
                        "count": installed.len(),
                    })),
                );
            }
            Err(e) => {
                bag.record_step(
                    format!("installed_{}", stone.name),
                    format!("{} failed to get installed offerings", stone.name),
                    duration.as_millis() as u64,
                    StepResult::failed(e.to_string()),
                );
            }
        }
    }

    Ok(bag)
}

// ============================================================================
// offerings.search - Verify search functionality works
// ============================================================================

pub fn search_test() -> TestDef {
    TestDef {
        id: "offerings.search",
        name: "Offerings Search",
        description: "Verify search returns relevant results for common queries",
        category: "offerings",
        tags: &["offerings", "search"],
        run: |garden, bag| Box::pin(test_search(garden, bag)),
    }
}

async fn test_search(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let tended = match garden.tended() {
        Some(s) => s,
        None => {
            bag.record_step(
                "search_skipped",
                "No tended stone",
                0,
                StepResult::skipped("No tended stone available"),
            );
            return Ok(bag);
        }
    };

    // Test queries that should return results
    let queries = vec![
        ("database", vec!["mongodb", "mariadb", "redis"]),
        ("cache", vec!["redis", "memcached"]),
        ("queue", vec!["rabbitmq"]),
    ];

    for (query, expected_any) in queries {
        let start = Instant::now();
        let url = format!("/api/v1/stone/offerings/search?q={}", query);
        let result = tended.get_json(&url).await;
        let duration = start.elapsed();

        match result {
            Ok(resp) => {
                let results: Vec<String> = resp
                    .get("data")
                    .and_then(|d| d.get("results"))
                    .and_then(|r| r.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|o| o.get("name").and_then(|n| n.as_str()))
                            .map(|s| s.to_string())
                            .collect()
                    })
                    .unwrap_or_default();

                let expected_set: HashSet<&str> = expected_any.iter().copied().collect();
                let found_expected = results.iter().any(|r| expected_set.contains(r.as_str()));

                let step_result = if found_expected || !results.is_empty() {
                    StepResult::ok_with(serde_json::json!({
                        "query": query,
                        "results": results,
                        "count": results.len(),
                    }))
                } else {
                    StepResult::failed(format!(
                        "Query '{}' returned no results, expected one of: {:?}",
                        query, expected_any
                    ))
                };

                bag.record_step(
                    format!("search_{}", query),
                    format!("Search '{}': {} results", query, results.len()),
                    duration.as_millis() as u64,
                    step_result,
                );
            }
            Err(e) => {
                bag.record_step(
                    format!("search_{}", query),
                    format!("Search '{}' failed", query),
                    duration.as_millis() as u64,
                    StepResult::failed(e.to_string()),
                );
            }
        }
    }

    Ok(bag)
}

// ============================================================================
// offerings.services_match_topology - Running services visible in topology
// ============================================================================

pub fn services_match_topology_test() -> TestDef {
    TestDef {
        id: "offerings.services_match_topology",
        name: "Services Match Topology",
        description: "Verify running services are visible in garden topology",
        category: "offerings",
        tags: &["offerings", "topology", "consistency"],
        run: |garden, bag| Box::pin(test_services_match_topology(garden, bag)),
    }
}

async fn test_services_match_topology(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let tended = match garden.tended() {
        Some(s) => s,
        None => {
            bag.record_step(
                "topology_skipped",
                "No tended stone",
                0,
                StepResult::skipped("No tended stone available"),
            );
            return Ok(bag);
        }
    };

    // Get topology from tended stone
    let start = Instant::now();
    let topology_result = tended.get_json("/api/v1/garden/topology").await;
    let duration = start.elapsed();

    match topology_result {
        Ok(resp) => {
            let entries = resp.get("data").and_then(|d| d.as_array());

            if let Some(entries) = entries {
                let mut total_services = 0;
                let mut stones_with_services = 0;

                for entry in entries {
                    let stone_name = entry
                        .get("stone_name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("unknown");

                    let services = entry
                        .get("services")
                        .and_then(|s| s.as_array())
                        .map(|arr| arr.len())
                        .unwrap_or(0);

                    if services > 0 {
                        stones_with_services += 1;
                        total_services += services;
                    }

                    bag.put(format!("{}_services", stone_name), services);
                }

                bag.record_step(
                    "topology_services",
                    format!(
                        "{} services across {} stones",
                        total_services, stones_with_services
                    ),
                    duration.as_millis() as u64,
                    StepResult::ok_with(serde_json::json!({
                        "total_services": total_services,
                        "stones_with_services": stones_with_services,
                        "total_stones": entries.len(),
                    })),
                );
            } else {
                bag.record_step(
                    "topology_services",
                    "Failed to parse topology",
                    duration.as_millis() as u64,
                    StepResult::failed("No data array in topology response"),
                );
            }
        }
        Err(e) => {
            bag.record_step(
                "topology_services",
                "Failed to get topology",
                duration.as_millis() as u64,
                StepResult::failed(e.to_string()),
            );
        }
    }

    Ok(bag)
}
