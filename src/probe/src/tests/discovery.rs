//! Discovery tests - topology and stone visibility

use crate::registry::TestDef;
use crate::{Bag, LiveGarden, StepResult};
use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;

// ============================================================================
// discovery.topology - All stones see each other
// ============================================================================

pub fn topology_test() -> TestDef {
    TestDef {
        id: "discovery.topology",
        name: "Topology Visibility",
        description: "Verify all stones can see each other in the garden topology",
        category: "discovery",
        tags: &["discovery", "topology", "network"],
        run: |garden, bag| Box::pin(test_topology(garden, bag)),
    }
}

async fn test_topology(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let expected_count = garden.stones.len();
    bag.put("expected_stone_count", expected_count);

    for stone in &garden.stones {
        let start = Instant::now();
        let result = stone.get_json("/api/v1/garden").await;
        let duration = start.elapsed();

        match result {
            Ok(resp) => {
                let visible_stones: Vec<String> = resp
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

                let visible_count = visible_stones.len();
                bag.put(format!("{}_sees", stone.name), &visible_stones);

                // Check if this stone sees all others
                let sees_all = visible_count >= expected_count;

                let result = if sees_all {
                    StepResult::ok_with(serde_json::json!({
                        "visible": visible_stones,
                        "count": visible_count,
                        "expected": expected_count,
                    }))
                } else {
                    StepResult::failed(format!(
                        "Expected {} stones, but {} only sees {}",
                        expected_count, stone.name, visible_count
                    ))
                };

                bag.record_step(
                    format!("topology_{}", stone.name),
                    format!(
                        "Topology from {}: sees {}/{} stones",
                        stone.name, visible_count, expected_count
                    ),
                    duration.as_millis() as u64,
                    result,
                );
            }
            Err(e) => {
                bag.record_step(
                    format!("topology_{}", stone.name),
                    format!("Topology from {}", stone.name),
                    duration.as_millis() as u64,
                    StepResult::failed(e.to_string()),
                );
            }
        }
    }

    Ok(bag)
}

// ============================================================================
// discovery.stone_count - Verify expected stone count
// ============================================================================

pub fn stone_count_test() -> TestDef {
    TestDef {
        id: "discovery.stone_count",
        name: "Stone Count",
        description: "Verify the expected number of stones are discovered",
        category: "discovery",
        tags: &["discovery", "quick"],
        run: |garden, bag| Box::pin(test_stone_count(garden, bag)),
    }
}

async fn test_stone_count(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let min_stones: usize = bag.get("min_stones").unwrap_or(1);
    let actual_count = garden.stones.len();

    let start = Instant::now();
    let duration = start.elapsed();

    let result = if actual_count >= min_stones {
        bag.put("stone_count", actual_count);
        bag.put("stone_names", garden.stone_names());
        StepResult::ok_with(serde_json::json!({
            "count": actual_count,
            "names": garden.stone_names(),
        }))
    } else {
        StepResult::failed(format!(
            "Expected at least {} stones, found {}",
            min_stones, actual_count
        ))
    };

    bag.record_step(
        "stone_count",
        format!("Found {} stones: {:?}", actual_count, garden.stone_names()),
        duration.as_millis() as u64,
        result,
    );

    Ok(bag)
}

// ============================================================================
// discovery.tended - Verify a tended stone exists
// ============================================================================

pub fn tended_test() -> TestDef {
    TestDef {
        id: "discovery.tended",
        name: "Tended Stone",
        description: "Verify a tended stone exists and is accessible",
        category: "discovery",
        tags: &["discovery", "quick"],
        run: |garden, bag| Box::pin(test_tended(garden, bag)),
    }
}

async fn test_tended(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let start = Instant::now();

    let result = match &garden.tended {
        Some(tended) => {
            bag.put("tended_stone", &tended.name);
            StepResult::ok_with(serde_json::json!({
                "tended": tended.name,
                "endpoint": tended.endpoint,
            }))
        }
        None => StepResult::failed("No tended stone found"),
    };

    let duration = start.elapsed();
    let description = match &garden.tended {
        Some(t) => format!("Tended stone: {}", t.name),
        None => "No tended stone".to_string(),
    };

    bag.record_step("tended", description, duration.as_millis() as u64, result);

    Ok(bag)
}
