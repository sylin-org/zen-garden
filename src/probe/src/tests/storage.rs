//! Storage tests - seed bank detection, beacon protocol

use crate::registry::TestDef;
use crate::{Bag, LiveGarden, StepResult};
use anyhow::Result;
use garden_common::constants::headers::HEADER_SEED_BANK;
use reqwest::StatusCode;
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};

const PROBE_BUCKET: &str = "probe-test";

// ============================================================================
// Helpers
// ============================================================================

#[derive(Clone)]
struct SelectedSeedBank {
    stone: crate::garden::Stone,
    id: String,
    name: String,
}

fn extract_bank_info(bank: &Value) -> Option<(String, String)> {
    let id = bank.get("id").and_then(|i| i.as_str())?.to_string();
    let name = bank.get("name").and_then(|n| n.as_str())?.to_string();
    Some((id, name))
}

async fn select_seed_bank(garden: &LiveGarden) -> Option<SelectedSeedBank> {
    for stone in &garden.stones {
        if let Ok(resp) = stone.get_json("/api/v1/stone/storage/bank").await {
            if let Some(banks) = resp.get("data").and_then(|d| d.as_array()) {
                for bank in banks {
                    if let Some((id, name)) = extract_bank_info(bank) {
                        return Some(SelectedSeedBank {
                            stone: stone.clone(),
                            id,
                            name,
                        });
                    }
                }
            }
        }
    }

    None
}

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
                    .and_then(|d| d.get("bank_count"))
                    .and_then(|b| b.as_u64())
                    .map(|v| v as usize)
                    .or_else(|| {
                        resp.get("data")
                            .and_then(|d| d.get("types"))
                            .and_then(|t| t.as_array())
                            .and_then(|arr| arr.first())
                            .and_then(|t| t.get("count"))
                            .and_then(|c| c.as_u64())
                            .map(|v| v as usize)
                    })
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
// storage.health - Check storage readiness on each stone
// ============================================================================

pub fn health_test() -> TestDef {
    TestDef {
        id: "storage.health",
        name: "Storage Health",
        description: "Check storage readiness (mounted + canonical + writable) on each stone",
        category: "storage",
        tags: &["storage", "health", "readiness"],
        run: |garden, bag| Box::pin(test_health(garden, bag)),
    }
}

async fn test_health(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    for stone in &garden.stones {
        let start = Instant::now();
        let result = stone.get_json("/api/v1/stone/storage/health").await;
        let duration = start.elapsed();

        match result {
            Ok(resp) => {
                let data = resp.get("data");
                let ready = data
                    .and_then(|d| d.get("ready"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let bank_count = data
                    .and_then(|d| d.get("bank_count"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let ready_count = data
                    .and_then(|d| d.get("ready_count"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let issues = data
                    .and_then(|d| d.get("issues"))
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|i| i.as_str())
                            .map(|s| s.to_string())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();

                let status = if ready { "ready" } else { "not ready" };
                let issue_text = if issues.is_empty() {
                    "no issues".to_string()
                } else {
                    issues.join(", ")
                };

                bag.record_step(
                    format!("health_{}", stone.name),
                    format!(
                        "{}: {} (banks: {}, ready: {}, issues: {})",
                        stone.name, status, bank_count, ready_count, issue_text
                    ),
                    duration.as_millis() as u64,
                    StepResult::ok_with(serde_json::json!({
                        "ready": ready,
                        "bank_count": bank_count,
                        "ready_count": ready_count,
                        "issues": issues,
                    })),
                );
            }
            Err(e) => {
                bag.record_step(
                    format!("health_{}", stone.name),
                    format!("{} storage health check failed", stone.name),
                    duration.as_millis() as u64,
                    StepResult::failed(e.to_string()),
                );
            }
        }
    }

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
    let selected = match select_seed_bank(&garden).await {
        Some(target) => target,
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

    let stone = selected.stone;
    let bank_id = selected.id;
    let bank_name = selected.name;

    bag.record_step(
        "select_bank",
        format!("Using bank {} ({}) on {}", bank_id, bank_name, stone.name),
        0,
        StepResult::ok(),
    );

    // Create a unique test key with timestamp
    let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S");
    let object_key = format!("roundtrip-{}.txt", timestamp);
    let test_key = format!("{}/{}", PROBE_BUCKET, object_key);
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

    // LIST the bucket to confirm object shows up
    let list_path = format!("/api/v1/stone/storage/bank/{}/{}/", bank_id, PROBE_BUCKET);
    let list_start = Instant::now();
    let list_result = stone.get_json(&list_path).await;
    let list_duration = list_start.elapsed();

    match &list_result {
        Ok(resp) => {
            let entries = resp
                .get("data")
                .and_then(|d| d.get("entries"))
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let found = entries.iter().any(|entry| {
                entry
                    .get("name")
                    .and_then(|n| n.as_str())
                    .map(|name| name == object_key)
                    .unwrap_or(false)
            });

            if found {
                bag.record_step(
                    "list_object",
                    format!("LIST {} found {}", PROBE_BUCKET, object_key),
                    list_duration.as_millis() as u64,
                    StepResult::ok(),
                );
            } else {
                bag.record_step(
                    "list_object",
                    format!("LIST {} missing {}", PROBE_BUCKET, object_key),
                    list_duration.as_millis() as u64,
                    StepResult::failed("Object not found in listing".to_string()),
                );
            }
        }
        Err(e) => {
            bag.record_step(
                "list_object",
                format!("LIST failed: {}", e),
                list_duration.as_millis() as u64,
                StepResult::failed(e.to_string()),
            );
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
        Ok(204) => {
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

// ============================================================================
// storage.gateway_roundtrip - PUT, GET, DELETE via garden storage gateway
// ============================================================================

pub fn gateway_roundtrip_test() -> TestDef {
    TestDef {
        id: "storage.gateway_roundtrip",
        name: "Gateway Roundtrip",
        description: "Upload, retrieve, and delete an object via /api/v1/storage",
        category: "storage",
        tags: &["storage", "gateway", "object"],
        run: |garden, bag| Box::pin(test_gateway_roundtrip(garden, bag)),
    }
}

async fn test_gateway_roundtrip(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let selected = match select_seed_bank(&garden).await {
        Some(target) => target,
        None => {
            bag.record_step(
                "gateway_roundtrip",
                "No seed banks available for testing",
                0,
                StepResult::skipped("No seed banks in garden"),
            );
            return Ok(bag);
        }
    };

    let storage_stone = selected.stone;
    let seed_bank = selected.name;
    let gateway_stone = garden
        .stones
        .iter()
        .find(|stone| stone.name != storage_stone.name)
        .cloned()
        .unwrap_or_else(|| storage_stone.clone());
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S");
    let bucket = PROBE_BUCKET;
    let key = format!("gateway-roundtrip-{}.txt", timestamp);
    let test_content = format!("Zen Garden gateway test at {}", timestamp);
    let test_bytes = test_content.as_bytes().to_vec();
    let url = format!(
        "{}/api/v1/storage/{}/{}",
        gateway_stone.endpoint, bucket, key
    );

    bag.record_step(
        "gateway_target",
        format!(
            "Gateway stone {} (seed bank on {})",
            gateway_stone.name, storage_stone.name
        ),
        0,
        StepResult::ok_with(serde_json::json!({
            "gateway_stone": gateway_stone.name,
            "storage_stone": storage_stone.name,
            "seed_bank": seed_bank,
        })),
    );

    // PUT
    let put_start = Instant::now();
    let put_resp = client
        .put(&url)
        .header("Content-Type", "text/plain")
        .header(HEADER_SEED_BANK, &seed_bank)
        .body(test_bytes.clone())
        .send()
        .await?;
    let put_duration = put_start.elapsed();

    if !put_resp.status().is_success() {
        bag.record_step(
            "gateway_put",
            format!("PUT failed with {}", put_resp.status()),
            put_duration.as_millis() as u64,
            StepResult::failed(format!("PUT status {}", put_resp.status())),
        );
        return Ok(bag);
    }

    let put_json: Value = put_resp.json().await.unwrap_or(Value::Null);
    bag.record_step(
        "gateway_put",
        format!("PUT {} bytes to {}/{}", test_bytes.len(), bucket, key),
        put_duration.as_millis() as u64,
        StepResult::ok_with(serde_json::json!({
            "bucket": bucket,
            "key": key,
            "seed_bank": seed_bank,
            "response": put_json,
        })),
    );

    // LIST
    let list_url = format!("{}/api/v1/storage/{}/", gateway_stone.endpoint, bucket);
    let list_start = Instant::now();
    let list_resp = client
        .get(&list_url)
        .header(HEADER_SEED_BANK, &seed_bank)
        .send()
        .await?;
    let list_duration = list_start.elapsed();

    if list_resp.status() == StatusCode::OK {
        let list_json: Value = list_resp.json().await.unwrap_or(Value::Null);
        let objects = list_json
            .get("data")
            .and_then(|d| d.get("objects"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let found = objects.iter().any(|obj| {
            obj.get("key")
                .and_then(|k| k.as_str())
                .map(|k| k == key)
                .unwrap_or(false)
        });

        if found {
            bag.record_step(
                "gateway_list",
                format!("LIST {} found {}", bucket, key),
                list_duration.as_millis() as u64,
                StepResult::ok(),
            );
        } else {
            bag.record_step(
                "gateway_list",
                format!("LIST {} missing {}", bucket, key),
                list_duration.as_millis() as u64,
                StepResult::failed("Object not found in listing".to_string()),
            );
        }
    } else {
        bag.record_step(
            "gateway_list",
            format!("LIST failed with {}", list_resp.status()),
            list_duration.as_millis() as u64,
            StepResult::failed(format!("LIST status {}", list_resp.status())),
        );
    }

    // GET
    let get_start = Instant::now();
    let get_resp = client
        .get(&url)
        .header(HEADER_SEED_BANK, &seed_bank)
        .send()
        .await?;
    let get_duration = get_start.elapsed();

    if get_resp.status() != StatusCode::OK {
        bag.record_step(
            "gateway_get",
            format!("GET failed with {}", get_resp.status()),
            get_duration.as_millis() as u64,
            StepResult::failed(format!("GET status {}", get_resp.status())),
        );
        return Ok(bag);
    }

    let bytes = get_resp.bytes().await?;
    let content_matches = bytes.as_ref() == test_bytes;
    if content_matches {
        bag.record_step(
            "gateway_get",
            format!("GET {} bytes, content verified", bytes.len()),
            get_duration.as_millis() as u64,
            StepResult::ok_with(serde_json::json!({
                "size": bytes.len(),
                "verified": true,
            })),
        );
    } else {
        bag.record_step(
            "gateway_get",
            "Content mismatch",
            get_duration.as_millis() as u64,
            StepResult::failed(format!(
                "Expected {} bytes, got {} bytes",
                test_bytes.len(),
                bytes.len()
            )),
        );
    }

    // DELETE
    let delete_start = Instant::now();
    let delete_resp = client
        .delete(&url)
        .header(HEADER_SEED_BANK, &seed_bank)
        .send()
        .await?;
    let delete_duration = delete_start.elapsed();

    if delete_resp.status() == StatusCode::NO_CONTENT || delete_resp.status() == StatusCode::OK {
        bag.record_step(
            "gateway_delete",
            format!("DELETE {} - cleaned up", key),
            delete_duration.as_millis() as u64,
            StepResult::ok(),
        );
    } else {
        bag.record_step(
            "gateway_delete",
            format!("DELETE returned {}", delete_resp.status()),
            delete_duration.as_millis() as u64,
            StepResult::failed(format!("DELETE status {}", delete_resp.status())),
        );
    }

    bag.record_step(
        "gateway_summary",
        format!(
            "Gateway roundtrip completed via {} (seed bank on {})",
            gateway_stone.name, storage_stone.name
        ),
        0,
        StepResult::ok_with(serde_json::json!({
            "gateway_stone": gateway_stone.name,
            "storage_stone": storage_stone.name,
            "seed_bank": seed_bank,
            "bucket": bucket,
            "key": key,
        })),
    );

    Ok(bag)
}

// ============================================================================
// storage.memories_index - List remote memories via /api/v1/memories
// ============================================================================

pub fn memories_index_test() -> TestDef {
    TestDef {
        id: "storage.memories_index",
        name: "Memories Index",
        description: "List remote snapshots via /api/v1/memories",
        category: "storage",
        tags: &["storage", "memories", "hydration"],
        run: |garden, bag| Box::pin(test_memories_index(garden, bag)),
    }
}

async fn test_memories_index(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let selected = match select_seed_bank(&garden).await {
        Some(target) => target,
        None => {
            bag.record_step(
                "memories_index",
                "No seed banks available for testing",
                0,
                StepResult::skipped("No seed banks in garden"),
            );
            return Ok(bag);
        }
    };

    let stone = selected.stone;
    let seed_bank = selected.name;
    let seed_bank_id = selected.id;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let url = format!("{}/api/v1/memories", stone.endpoint);
    let start = Instant::now();
    let resp = client
        .get(&url)
        .header(HEADER_SEED_BANK, &seed_bank)
        .send()
        .await?;
    let duration = start.elapsed();

    if resp.status() != StatusCode::OK {
        bag.record_step(
            "memories_index",
            format!("GET /api/v1/memories failed with {}", resp.status()),
            duration.as_millis() as u64,
            StepResult::failed(format!("Status {}", resp.status())),
        );
        return Ok(bag);
    }

    let json: Value = resp.json().await.unwrap_or(Value::Null);
    let data = json.get("data");
    let reported_id = data
        .and_then(|d| d.get("seed_bank_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let snapshots = data
        .and_then(|d| d.get("snapshots"))
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    let id_matches = reported_id == seed_bank_id;
    let result = if id_matches {
        StepResult::ok_with(serde_json::json!({
            "seed_bank": seed_bank,
            "seed_bank_id": seed_bank_id,
            "snapshots": snapshots,
        }))
    } else {
        StepResult::failed(format!(
            "seed_bank_id mismatch (expected {}, got {})",
            seed_bank_id, reported_id
        ))
    };

    bag.record_step(
        "memories_index",
        format!("Found {} snapshots on {}", snapshots, seed_bank),
        duration.as_millis() as u64,
        result,
    );

    Ok(bag)
}
