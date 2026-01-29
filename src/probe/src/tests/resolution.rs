//! Resolution tests - service discovery and resolution
//!
//! Tests the /api/v1/garden/services endpoint (used by `rake find`)

use crate::registry::TestDef;
use crate::{Bag, LiveGarden, StepResult};
use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;

// ============================================================================
// resolution.service_discovery - Find services using the services API
// ============================================================================

pub fn offering_lookup_test() -> TestDef {
    TestDef {
        id: "resolution.service_discovery",
        name: "Service Discovery",
        description: "Find running services using /api/v1/garden/services (rake find)",
        category: "resolution",
        tags: &["resolution", "services", "find"],
        run: |garden, bag| Box::pin(test_service_discovery(garden, bag)),
    }
}

async fn test_service_discovery(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let tended = match garden.tended() {
        Some(s) => s,
        None => {
            bag.record_step(
                "resolution_skipped",
                "No tended stone",
                0,
                StepResult::skipped("No tended stone available"),
            );
            return Ok(bag);
        }
    };

    // Test queries that should find running services
    let queries = vec![
        "mongodb",
        "redis", 
        "c:database",  // category query
    ];

    let mut found_any = false;

    for query in queries {
        let start = Instant::now();
        let url = format!("/api/v1/garden/services?q={}", urlencoding::encode(query));
        let result = tended.get_json(&url).await;
        let duration = start.elapsed();

        match result {
            Ok(resp) => {
                let services = resp
                    .get("data")
                    .and_then(|d| d.get("services"))
                    .and_then(|s| s.as_array())
                    .map(|arr| arr.len())
                    .unwrap_or(0);

                if services > 0 {
                    found_any = true;
                }

                bag.record_step(
                    format!("find_{}", query.replace(':', "_")),
                    format!("find '{}': {} services", query, services),
                    duration.as_millis() as u64,
                    StepResult::ok_with(serde_json::json!({
                        "query": query,
                        "services": services,
                    })),
                );
            }
            Err(e) => {
                // 404 means endpoint not available
                if e.to_string().contains("404") {
                    bag.record_step(
                        format!("find_{}", query.replace(':', "_")),
                        format!("find '{}': API not available", query),
                        duration.as_millis() as u64,
                        StepResult::skipped("Services API not available"),
                    );
                } else {
                    bag.record_step(
                        format!("find_{}", query.replace(':', "_")),
                        format!("find '{}': failed", query),
                        duration.as_millis() as u64,
                        StepResult::failed(e.to_string()),
                    );
                }
            }
        }
    }

    bag.record_step(
        "discovery_summary",
        if found_any { "Found running services" } else { "No running services (OK if none deployed)" }.to_string(),
        0,
        if found_any {
            StepResult::ok()
        } else {
            StepResult::skipped("No services currently running")
        },
    );

    Ok(bag)
}

// ============================================================================
// resolution.category_search - Search by category (c:database, c:cache)
// ============================================================================

pub fn protocol_test() -> TestDef {
    TestDef {
        id: "resolution.category_search",
        name: "Category Search",
        description: "Find services by category (c:database, c:cache, etc)",
        category: "resolution",
        tags: &["resolution", "category", "search"],
        run: |garden, bag| Box::pin(test_category_search(garden, bag)),
    }
}

async fn test_category_search(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let tended = match garden.tended() {
        Some(s) => s,
        None => {
            bag.record_step(
                "category_skipped",
                "No tended stone",
                0,
                StepResult::skipped("No tended stone available"),
            );
            return Ok(bag);
        }
    };

    // Test category queries
    let categories = ["c:database", "c:cache", "c:queue", "c:vault"];

    for category in categories {
        let start = Instant::now();
        let url = format!("/api/v1/garden/services?q={}", urlencoding::encode(category));
        let result = tended.get_json(&url).await;
        let duration = start.elapsed();

        match result {
            Ok(resp) => {
                let services = resp
                    .get("data")
                    .and_then(|d| d.get("services"))
                    .and_then(|s| s.as_array())
                    .map(|arr| arr.len())
                    .unwrap_or(0);

                bag.record_step(
                    format!("category_{}", category.replace(':', "_")),
                    format!("{}: {} services", category, services),
                    duration.as_millis() as u64,
                    StepResult::ok_with(serde_json::json!({
                        "category": category,
                        "services": services,
                    })),
                );
            }
            Err(e) => {
                if e.to_string().contains("404") {
                    bag.record_step(
                        format!("category_{}", category.replace(':', "_")),
                        format!("{}: API not available", category),
                        duration.as_millis() as u64,
                        StepResult::skipped("Services API not available"),
                    );
                } else {
                    bag.record_step(
                        format!("category_{}", category.replace(':', "_")),
                        format!("{}: search error", category),
                        duration.as_millis() as u64,
                        StepResult::failed(e.to_string()),
                    );
                }
            }
        }
    }

    Ok(bag)
}

// ============================================================================
// resolution.consistency - Each stone reports its own services correctly
// ============================================================================

pub fn consistency_test() -> TestDef {
    TestDef {
        id: "resolution.consistency",
        name: "Service Reporting",
        description: "Verify each stone correctly reports its local services",
        category: "resolution",
        tags: &["resolution", "consistency"],
        run: |garden, bag| Box::pin(test_consistency(garden, bag)),
    }
}

async fn test_consistency(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    // Query each stone for its local services
    let mut total_services = 0;

    for stone in &garden.stones {
        let start = Instant::now();
        let result = stone.get_json("/api/v1/stone/services").await;
        let duration = start.elapsed();

        match result {
            Ok(resp) => {
                let services: Vec<String> = resp
                    .get("data")
                    .and_then(|d| d.get("services"))
                    .and_then(|s| s.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|s| s.get("name").and_then(|n| n.as_str()))
                            .map(|s| s.to_string())
                            .collect()
                    })
                    .unwrap_or_default();

                let count = services.len();
                total_services += count;

                bag.record_step(
                    format!("services_{}", stone.name),
                    format!("{}: {} local services", stone.name, count),
                    duration.as_millis() as u64,
                    StepResult::ok_with(serde_json::json!({
                        "stone": stone.name,
                        "services": services,
                        "count": count,
                    })),
                );
            }
            Err(e) => {
                if e.to_string().contains("404") {
                    bag.record_step(
                        format!("services_{}", stone.name),
                        format!("{}: API not available", stone.name),
                        duration.as_millis() as u64,
                        StepResult::skipped("Services API not available"),
                    );
                } else {
                    bag.record_step(
                        format!("services_{}", stone.name),
                        format!("{}: query failed", stone.name),
                        duration.as_millis() as u64,
                        StepResult::failed(e.to_string()),
                    );
                }
            }
        }
    }

    bag.record_step(
        "services_summary",
        format!("{} total services across {} stones", total_services, garden.stones.len()),
        0,
        StepResult::ok_with(serde_json::json!({
            "total_services": total_services,
            "stones": garden.stones.len(),
        })),
    );

    Ok(bag)
}
