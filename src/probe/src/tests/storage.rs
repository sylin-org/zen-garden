//! Storage tests - seed bank detection, beacon protocol

use crate::registry::TestDef;
use crate::{Bag, LiveGarden, StepResult};
use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;

// ============================================================================
// storage.overview - Get storage overview from each stone
// ============================================================================

pub fn overview_test() -> TestDef {
    TestDef {
        id: "storage.overview",
        name: "Storage Overview",
        description: "Check storage status on each stone",
        category: "storage",
        tags: &["storage", "seed-bank"],
        run: |garden, bag| Box::pin(test_overview(garden, bag)),
    }
}

async fn test_overview(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let mut total_banks = 0;
    let mut total_capacity_gb: f64 = 0.0;

    for stone in &garden.stones {
        let start = Instant::now();
        let result = stone.get_json("/api/v1/stone/storage").await;
        let duration = start.elapsed();

        match result {
            Ok(resp) => {
                let banks = resp
                    .get("data")
                    .and_then(|d| d.get("seed_banks"))
                    .and_then(|b| b.as_array())
                    .map(|arr| arr.len())
                    .unwrap_or(0);

                let capacity = resp
                    .get("data")
                    .and_then(|d| d.get("total_capacity_bytes"))
                    .and_then(|c| c.as_u64())
                    .map(|b| b as f64 / 1_073_741_824.0) // Convert to GB
                    .unwrap_or(0.0);

                total_banks += banks;
                total_capacity_gb += capacity;

                bag.record_step(
                    format!("storage_{}", stone.name),
                    format!("{}: {} seed banks, {:.1} GB", stone.name, banks, capacity),
                    duration.as_millis() as u64,
                    StepResult::ok_with(serde_json::json!({
                        "seed_banks": banks,
                        "capacity_gb": capacity,
                    })),
                );
            }
            Err(e) => {
                bag.record_step(
                    format!("storage_{}", stone.name),
                    format!("{} storage check failed", stone.name),
                    duration.as_millis() as u64,
                    StepResult::failed(e.to_string()),
                );
            }
        }
    }

    bag.put("total_banks", total_banks);
    bag.put("total_capacity_gb", total_capacity_gb);

    bag.record_step(
        "storage_summary",
        format!(
            "{} seed banks, {:.1} GB total capacity",
            total_banks, total_capacity_gb
        ),
        0,
        StepResult::ok_with(serde_json::json!({
            "total_banks": total_banks,
            "total_capacity_gb": total_capacity_gb,
        })),
    );

    Ok(bag)
}

// ============================================================================
// storage.candidates - Check for eligible storage devices
// ============================================================================

pub fn candidates_test() -> TestDef {
    TestDef {
        id: "storage.candidates",
        name: "Storage Candidates",
        description: "Check for eligible storage devices on each stone",
        category: "storage",
        tags: &["storage", "candidates"],
        run: |garden, bag| Box::pin(test_candidates(garden, bag)),
    }
}

async fn test_candidates(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let mut total_candidates = 0;

    for stone in &garden.stones {
        let start = Instant::now();
        let result = stone.get_json("/api/v1/stone/storage/candidates").await;
        let duration = start.elapsed();

        match result {
            Ok(resp) => {
                let candidates = resp
                    .get("data")
                    .and_then(|d| d.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|c| {
                                let path = c.get("path").and_then(|p| p.as_str())?;
                                let size_gb = c
                                    .get("size_bytes")
                                    .and_then(|s| s.as_u64())
                                    .map(|b| b as f64 / 1_073_741_824.0)
                                    .unwrap_or(0.0);
                                Some((path.to_string(), size_gb))
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();

                let count = candidates.len();
                total_candidates += count;

                bag.record_step(
                    format!("candidates_{}", stone.name),
                    format!("{}: {} candidate devices", stone.name, count),
                    duration.as_millis() as u64,
                    StepResult::ok_with(serde_json::json!({
                        "count": count,
                        "devices": candidates,
                    })),
                );
            }
            Err(e) => {
                bag.record_step(
                    format!("candidates_{}", stone.name),
                    format!("{} candidate check failed", stone.name),
                    duration.as_millis() as u64,
                    StepResult::failed(e.to_string()),
                );
            }
        }
    }

    bag.put("total_candidates", total_candidates);

    Ok(bag)
}

// ============================================================================
// storage.beacon_visibility - Verify seed banks are visible across stones
// ============================================================================

pub fn beacon_visibility_test() -> TestDef {
    TestDef {
        id: "storage.beacon_visibility",
        name: "Beacon Visibility",
        description: "Verify seed banks are visible across all stones via beacon protocol",
        category: "storage",
        tags: &["storage", "beacon", "consistency"],
        run: |garden, bag| Box::pin(test_beacon_visibility(garden, bag)),
    }
}

async fn test_beacon_visibility(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    // Collect all seed banks from all stones
    let mut all_banks: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut visibility: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();

    for stone in &garden.stones {
        let result = stone.get_json("/api/v1/stone/storage/bank").await;

        if let Ok(resp) = result {
            let banks: Vec<String> = resp
                .get("data")
                .and_then(|d| d.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|b| b.get("id").and_then(|i| i.as_str()))
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_default();

            for bank in &banks {
                all_banks.insert(bank.clone());
            }
            visibility.insert(stone.name.clone(), banks);
        }
    }

    if all_banks.is_empty() {
        bag.record_step(
            "beacon_visibility",
            "No seed banks in garden",
            0,
            StepResult::skipped("No seed banks to check visibility"),
        );
        return Ok(bag);
    }

    // Check if all stones see the same banks (via beacon cache)
    let mut inconsistencies = Vec::new();

    for stone in &garden.stones {
        let start = Instant::now();
        // This endpoint should return cached banks from all stones
        let result = stone.get_json("/api/v1/stone/storage").await;
        let duration = start.elapsed();

        if let Ok(resp) = result {
            let cached_banks: Vec<String> = resp
                .get("data")
                .and_then(|d| d.get("garden_banks"))
                .and_then(|g| g.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|b| b.get("id").and_then(|i| i.as_str()))
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_default();

            let cached_set: std::collections::HashSet<_> = cached_banks.iter().cloned().collect();
            let missing: Vec<_> = all_banks.difference(&cached_set).collect();

            if missing.is_empty() {
                bag.record_step(
                    format!("beacon_{}", stone.name),
                    format!("{} sees all {} banks", stone.name, all_banks.len()),
                    duration.as_millis() as u64,
                    StepResult::ok(),
                );
            } else {
                inconsistencies.push((stone.name.clone(), missing.len()));
                bag.record_step(
                    format!("beacon_{}", stone.name),
                    format!("{} missing {} banks", stone.name, missing.len()),
                    duration.as_millis() as u64,
                    StepResult::failed(format!("Missing banks: {:?}", missing)),
                );
            }
        }
    }

    if inconsistencies.is_empty() {
        bag.record_step(
            "beacon_summary",
            format!("All stones see all {} banks", all_banks.len()),
            0,
            StepResult::ok(),
        );
    } else {
        bag.record_step(
            "beacon_summary",
            "Beacon visibility inconsistent",
            0,
            StepResult::failed(format!("Inconsistencies: {:?}", inconsistencies)),
        );
    }

    Ok(bag)
}

// ============================================================================
// storage.object_roundtrip - PUT, GET, DELETE object in a seed bank
// ============================================================================

pub fn object_roundtrip_test() -> TestDef {
    TestDef {
        id: "storage.object_roundtrip",
        name: "Object Roundtrip",
        description: "Upload, retrieve, and delete a test object in a seed bank",
        category: "storage",
        tags: &["storage", "object", "s3"],
        run: |garden, bag| Box::pin(test_object_roundtrip(garden, bag)),
    }
}

async fn test_object_roundtrip(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    // Find a stone with at least one seed bank
    let mut target_stone: Option<&crate::garden::Stone> = None;
    let mut target_bank_id: Option<String> = None;

    for stone in &garden.stones {
        let result = stone.get_json("/api/v1/stone/storage/bank").await;
        if let Ok(resp) = result {
            if let Some(banks) = resp.get("data").and_then(|d| d.as_array()) {
                if let Some(bank) = banks.first() {
                    if let Some(id) = bank.get("id").and_then(|i| i.as_str()) {
                        target_stone = Some(stone);
                        target_bank_id = Some(id.to_string());
                        break;
                    }
                }
            }
        }
    }

    let (stone, bank_id) = match (target_stone, target_bank_id) {
        (Some(s), Some(id)) => (s, id),
        _ => {
            bag.record_step(
                "object_roundtrip",
                "No seed banks available for testing",
                0,
                StepResult::skipped("No seed banks in garden"),
            );
            return Ok(bag);
        }
    };

    bag.record_step(
        "select_bank",
        format!("Using bank {} on {}", bank_id, stone.name),
        0,
        StepResult::ok(),
    );

    // Create a unique test key with timestamp
    let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S");
    let test_key = format!("probe-test/roundtrip-{}.txt", timestamp);
    let test_content = format!("Zen Garden probe test at {}", timestamp);
    let test_bytes = test_content.as_bytes().to_vec();

    // PUT the object
    let put_path = format!("/api/v1/stone/storage/bank/{}/{}", bank_id, test_key);
    let put_start = Instant::now();
    let put_result = stone
        .put_bytes(&put_path, "text/plain", test_bytes.clone())
        .await;
    let put_duration = put_start.elapsed();

    match &put_result {
        Ok(resp) => {
            let size = resp
                .get("data")
                .and_then(|d| d.get("size"))
                .and_then(|s| s.as_u64())
                .unwrap_or(0);
            bag.record_step(
                "put_object",
                format!("PUT {} bytes to {}", size, test_key),
                put_duration.as_millis() as u64,
                StepResult::ok_with(serde_json::json!({
                    "key": test_key,
                    "size": size,
                })),
            );
        }
        Err(e) => {
            bag.record_step(
                "put_object",
                format!("PUT failed: {}", e),
                put_duration.as_millis() as u64,
                StepResult::failed(e.to_string()),
            );
            return Ok(bag);
        }
    }

    // GET the object back
    let get_path = format!("/api/v1/stone/storage/bank/{}/{}", bank_id, test_key);
    let get_start = Instant::now();
    let get_result = stone.get_bytes(&get_path).await;
    let get_duration = get_start.elapsed();

    match &get_result {
        Ok(bytes) => {
            let content_matches = bytes == &test_bytes;
            if content_matches {
                bag.record_step(
                    "get_object",
                    format!("GET {} bytes, content verified", bytes.len()),
                    get_duration.as_millis() as u64,
                    StepResult::ok_with(serde_json::json!({
                        "size": bytes.len(),
                        "verified": true,
                    })),
                );
            } else {
                bag.record_step(
                    "get_object",
                    "Content mismatch!",
                    get_duration.as_millis() as u64,
                    StepResult::failed(format!(
                        "Expected {} bytes, got {} bytes",
                        test_bytes.len(),
                        bytes.len()
                    )),
                );
                // Still try to clean up
            }
        }
        Err(e) => {
            bag.record_step(
                "get_object",
                format!("GET failed: {}", e),
                get_duration.as_millis() as u64,
                StepResult::failed(e.to_string()),
            );
            // Still try to clean up
        }
    }

    // DELETE the object (cleanup)
    let delete_path = format!("/api/v1/stone/storage/bank/{}/{}", bank_id, test_key);
    let delete_start = Instant::now();
    let delete_result = stone.delete_status_code(&delete_path).await;
    let delete_duration = delete_start.elapsed();

    match delete_result {
        Ok(status) if status == 204 => {
            bag.record_step(
                "delete_object",
                format!("DELETE {} - cleaned up", test_key),
                delete_duration.as_millis() as u64,
                StepResult::ok(),
            );
        }
        Ok(status) => {
            bag.record_step(
                "delete_object",
                format!("DELETE returned unexpected status {}", status),
                delete_duration.as_millis() as u64,
                StepResult::failed(format!("Expected 204, got {}", status)),
            );
        }
        Err(e) => {
            bag.record_step(
                "delete_object",
                format!("DELETE failed: {}", e),
                delete_duration.as_millis() as u64,
                StepResult::failed(e.to_string()),
            );
        }
    }

    // Summary - object roundtrip is pass if we completed PUT/GET/DELETE
    bag.record_step(
        "roundtrip_summary",
        format!("Object roundtrip completed on {}", stone.name),
        0,
        StepResult::ok_with(serde_json::json!({
            "stone": stone.name,
            "bank": bank_id,
            "key": test_key,
        })),
    );

    Ok(bag)
}
