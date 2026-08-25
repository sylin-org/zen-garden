//! Smoke tests - basic health checks

use crate::registry::TestDef;
use crate::{Bag, LiveGarden, StepResult};
use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;

// ============================================================================
// smoke.health - All stones respond to /health
// ============================================================================

pub fn health_test() -> TestDef {
    TestDef {
        id: "smoke.health",
        name: "Health Check",
        description: "Verify all stones respond to /health endpoint",
        category: "smoke",
        tags: &["smoke", "health", "quick"],
        run: |garden, bag| Box::pin(test_health(garden, bag)),
    }
}

async fn test_health(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    for stone in &garden.stones {
        let start = Instant::now();
        let healthy = stone.is_healthy().await;
        let duration = start.elapsed();

        let result = if healthy {
            bag.put(format!("{}_healthy", stone.name), true);
            StepResult::ok_with(serde_json::json!({ "healthy": true }))
        } else {
            StepResult::failed(format!("{} health check failed", stone.name))
        };

        bag.record_step(
            format!("health_{}", stone.name),
            format!("Health check: {}", stone.name),
            duration.as_millis() as u64,
            result,
        );
    }

    Ok(bag)
}

// ============================================================================
// smoke.capabilities - All stones return capabilities
// ============================================================================

pub fn capabilities_test() -> TestDef {
    TestDef {
        id: "smoke.capabilities",
        name: "Capabilities Check",
        description: "Verify all stones return their capabilities",
        category: "smoke",
        tags: &["smoke", "capabilities", "quick"],
        run: |garden, bag| Box::pin(test_capabilities(garden, bag)),
    }
}

async fn test_capabilities(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    for stone in &garden.stones {
        let start = Instant::now();
        let result = stone.get_json("/api/v1/stone/capabilities").await;
        let duration = start.elapsed();

        match result {
            Ok(caps) => {
                let stone_name = caps
                    .get("stone_name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown");

                bag.put(format!("{}_capabilities", stone.name), &caps);
                bag.record_step(
                    format!("caps_{}", stone.name),
                    format!("Get capabilities: {} ({})", stone.name, stone_name),
                    duration.as_millis() as u64,
                    StepResult::ok_with(caps),
                );
            }
            Err(e) => {
                bag.record_step(
                    format!("caps_{}", stone.name),
                    format!("Get capabilities: {}", stone.name),
                    duration.as_millis() as u64,
                    StepResult::failed(e.to_string()),
                );
            }
        }
    }

    Ok(bag)
}

// ============================================================================
// smoke.offerings_list - All stones can list offerings
// ============================================================================

pub fn offerings_list_test() -> TestDef {
    TestDef {
        id: "smoke.offerings_list",
        name: "Offerings List",
        description: "Verify all stones can list their offerings",
        category: "smoke",
        tags: &["smoke", "offerings", "quick"],
        run: |garden, bag| Box::pin(test_offerings_list(garden, bag)),
    }
}

async fn test_offerings_list(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    for stone in &garden.stones {
        let start = Instant::now();
        let result = stone.get_json("/api/v1/stone/offerings").await;
        let duration = start.elapsed();

        match result {
            Ok(resp) => {
                let count = resp
                    .get("data")
                    .and_then(|d| d.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);

                bag.put(format!("{}_offerings_count", stone.name), count);
                bag.record_step(
                    format!("offerings_{}", stone.name),
                    format!("List offerings: {} ({} offerings)", stone.name, count),
                    duration.as_millis() as u64,
                    StepResult::ok_with(serde_json::json!({ "count": count })),
                );
            }
            Err(e) => {
                bag.record_step(
                    format!("offerings_{}", stone.name),
                    format!("List offerings: {}", stone.name),
                    duration.as_millis() as u64,
                    StepResult::failed(e.to_string()),
                );
            }
        }
    }

    Ok(bag)
}
