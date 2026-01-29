//! Nourishment tests - update detection and aggregation

use crate::registry::TestDef;
use crate::{Bag, LiveGarden, StepResult};
use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;

// ============================================================================
// nourishment.detection - Check for pending updates on each stone
// ============================================================================

pub fn detection_test() -> TestDef {
    TestDef {
        id: "nourishment.detection",
        name: "Update Detection",
        description: "Check for pending updates on each stone",
        category: "nourishment",
        tags: &["nourishment", "updates"],
        run: |garden, bag| Box::pin(test_detection(garden, bag)),
    }
}

async fn test_detection(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let mut total_updates = 0;
    let mut stones_with_updates = 0;

    for stone in &garden.stones {
        let start = Instant::now();
        let result = stone.get_json("/api/v1/stone/nourishment").await;
        let duration = start.elapsed();

        match result {
            Ok(resp) => {
                // Count updates by scope
                let offerings_count = resp
                    .get("data")
                    .and_then(|d| d.get("offerings"))
                    .and_then(|o| o.as_array())
                    .map(|arr| arr.len())
                    .unwrap_or(0);

                let firmware_count = resp
                    .get("data")
                    .and_then(|d| d.get("firmware"))
                    .and_then(|f| f.as_array())
                    .map(|arr| arr.len())
                    .unwrap_or(0);

                let stone_updates = offerings_count + firmware_count;
                total_updates += stone_updates;
                if stone_updates > 0 {
                    stones_with_updates += 1;
                }

                bag.record_step(
                    format!("nourishment_{}", stone.name),
                    format!(
                        "{}: {} offerings, {} firmware updates",
                        stone.name, offerings_count, firmware_count
                    ),
                    duration.as_millis() as u64,
                    StepResult::ok_with(serde_json::json!({
                        "offerings": offerings_count,
                        "firmware": firmware_count,
                        "total": stone_updates,
                    })),
                );
            }
            Err(e) => {
                bag.record_step(
                    format!("nourishment_{}", stone.name),
                    format!("{} nourishment check failed", stone.name),
                    duration.as_millis() as u64,
                    StepResult::failed(e.to_string()),
                );
            }
        }
    }

    bag.put("total_updates", total_updates);
    bag.put("stones_with_updates", stones_with_updates);

    bag.record_step(
        "nourishment_summary",
        format!(
            "{} pending updates across {} stones",
            total_updates, stones_with_updates
        ),
        0,
        StepResult::ok_with(serde_json::json!({
            "total_updates": total_updates,
            "stones_with_updates": stones_with_updates,
        })),
    );

    Ok(bag)
}

// ============================================================================
// nourishment.garden_aggregation - Verify garden-level aggregation works
// ============================================================================

pub fn garden_aggregation_test() -> TestDef {
    TestDef {
        id: "nourishment.garden_aggregation",
        name: "Garden Update Aggregation",
        description: "Verify garden-wide update aggregation works",
        category: "nourishment",
        tags: &["nourishment", "garden", "aggregation"],
        run: |garden, bag| Box::pin(test_garden_aggregation(garden, bag)),
    }
}

async fn test_garden_aggregation(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let tended = match garden.tended() {
        Some(s) => s,
        None => {
            bag.record_step(
                "aggregation_skipped",
                "No tended stone",
                0,
                StepResult::skipped("No tended stone available"),
            );
            return Ok(bag);
        }
    };

    let start = Instant::now();
    let result = tended.get_json("/api/v1/garden/nourishment").await;
    let duration = start.elapsed();

    match result {
        Ok(resp) => {
            // Garden nourishment should aggregate from all stones
            let stones = resp
                .get("data")
                .and_then(|d| d.get("stones"))
                .and_then(|s| s.as_array());

            if let Some(stones) = stones {
                let stone_count = stones.len();
                let expected = garden.stones.len();

                let result = if stone_count >= expected {
                    StepResult::ok_with(serde_json::json!({
                        "stone_count": stone_count,
                        "expected": expected,
                    }))
                } else {
                    StepResult::failed(format!(
                        "Garden nourishment only includes {}/{} stones",
                        stone_count, expected
                    ))
                };

                bag.record_step(
                    "garden_nourishment",
                    format!("Garden nourishment covers {} stones", stone_count),
                    duration.as_millis() as u64,
                    result,
                );
            } else {
                // May be a different response format - check for direct updates
                let has_data = resp.get("data").is_some();
                bag.record_step(
                    "garden_nourishment",
                    "Garden nourishment endpoint responding",
                    duration.as_millis() as u64,
                    if has_data {
                        StepResult::ok()
                    } else {
                        StepResult::failed("No data in response")
                    },
                );
            }
        }
        Err(e) => {
            bag.record_step(
                "garden_nourishment",
                "Garden nourishment failed",
                duration.as_millis() as u64,
                StepResult::failed(e.to_string()),
            );
        }
    }

    Ok(bag)
}

// ============================================================================
// nourishment.scope_filtering - Verify scope filter works (offerings/firmware)
// ============================================================================

pub fn scope_filtering_test() -> TestDef {
    TestDef {
        id: "nourishment.scope_filtering",
        name: "Scope Filtering",
        description: "Verify nourishment scope filter works correctly",
        category: "nourishment",
        tags: &["nourishment", "filtering"],
        run: |garden, bag| Box::pin(test_scope_filtering(garden, bag)),
    }
}

async fn test_scope_filtering(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let tended = match garden.tended() {
        Some(s) => s,
        None => {
            bag.record_step(
                "scope_skipped",
                "No tended stone",
                0,
                StepResult::skipped("No tended stone available"),
            );
            return Ok(bag);
        }
    };

    // Test that scope parameter is accepted (even if no updates)
    for scope in ["all", "offerings", "firmware"] {
        let start = Instant::now();
        let url = format!("/api/v1/stone/nourishment?scope={}", scope);
        let result = tended.get_json(&url).await;
        let duration = start.elapsed();

        let step_result = match result {
            Ok(_) => StepResult::ok_with(serde_json::json!({
                "scope": scope,
                "accepted": true,
            })),
            Err(e) => StepResult::failed(e.to_string()),
        };

        bag.record_step(
            format!("scope_{}", scope),
            format!("Scope '{}' filter", scope),
            duration.as_millis() as u64,
            step_result,
        );
    }

    Ok(bag)
}
