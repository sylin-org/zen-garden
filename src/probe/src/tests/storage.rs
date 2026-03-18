//! Storage tests - seed bank detection, beacon protocol

use crate::registry::TestDef;
use crate::{Bag, LiveGarden, StepResult};
use anyhow::Result;
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
        if let Ok(resp) = stone.get_json("/api/v1/stone/storage/banks").await {
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
        let result = stone.get_json("/api/v1/stone/storage/banks").await;

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
    let put_path = format!("/api/v1/garden/storage/{}/objects/{}", bank_name, test_key);
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
    let list_path = format!(
        "/api/v1/garden/storage/{}/objects/{}/",
        bank_name, PROBE_BUCKET
    );
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
    let get_path = format!("/api/v1/garden/storage/{}/objects/{}", bank_name, test_key);
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
    let delete_path = format!("/api/v1/garden/storage/{}/objects/{}", bank_name, test_key);
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
        description: "Upload, retrieve, and delete an object via garden storage objects",
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
        "{}/api/v1/garden/storage/{}/objects/{}/{}",
        gateway_stone.endpoint, seed_bank, bucket, key
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
            "storage": seed_bank,
        })),
    );

    // PUT
    let put_start = Instant::now();
    let put_resp = client
        .put(&url)
        .header("Content-Type", "text/plain")
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
            "storage": seed_bank,
            "response": put_json,
        })),
    );

    // LIST
    let list_url = format!(
        "{}/api/v1/garden/storage/{}/objects/{}/",
        gateway_stone.endpoint, seed_bank, bucket
    );
    let list_start = Instant::now();
    let list_resp = client.get(&list_url).send().await?;
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
    let get_resp = client.get(&url).send().await?;
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
    let delete_resp = client.delete(&url).send().await?;
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
            "storage": seed_bank,
            "bucket": bucket,
            "key": key,
        })),
    );

    Ok(bag)
}

// ============================================================================
// storage.memories_index - List remote memories via garden storage memories
// ============================================================================

pub fn memories_index_test() -> TestDef {
    TestDef {
        id: "storage.memories_index",
        name: "Memories Index",
        description: "List remote snapshots via garden storage memories endpoint",
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

    let url = format!(
        "{}/api/v1/garden/storage/{}/memories",
        stone.endpoint, seed_bank
    );
    let start = Instant::now();
    let resp = client.get(&url).send().await?;
    let duration = start.elapsed();

    if resp.status() != StatusCode::OK {
        bag.record_step(
            "memories_index",
            format!("GET memories endpoint failed with {}", resp.status()),
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
            "storage": seed_bank,
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

// ============================================================================
// STORAGE-0006: Orchestration integration tests
// ============================================================================

// ============================================================================
// storage.roles - Verify orchestration assigns exactly one Primary per name
// ============================================================================

pub fn roles_test() -> TestDef {
    TestDef {
        id: "storage.roles",
        name: "Role Assignment",
        description: "Verify orchestration assigns exactly one Primary per seed bank name",
        category: "storage",
        tags: &["storage", "orchestration", "storage-0006"],
        run: |garden, bag| Box::pin(test_roles(garden, bag)),
    }
}

async fn test_roles(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    // Collect all garden_banks from each stone's storage overview
    let mut banks_by_name: std::collections::HashMap<String, Vec<(String, String, bool)>> =
        std::collections::HashMap::new();

    for stone in &garden.stones {
        let start = Instant::now();
        let result = stone.get_json("/api/v1/stone/storage").await;
        let duration = start.elapsed();

        match result {
            Ok(resp) => {
                let garden_banks = resp
                    .get("data")
                    .and_then(|d| d.get("garden_banks"))
                    .and_then(|g| g.as_array())
                    .cloned()
                    .unwrap_or_default();

                for bank in &garden_banks {
                    let name = bank
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("")
                        .to_string();
                    let role = bank
                        .get("role")
                        .and_then(|r| r.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let pinned = bank
                        .get("pinned")
                        .and_then(|p| p.as_bool())
                        .unwrap_or(false);

                    if !name.is_empty() {
                        banks_by_name.entry(name).or_default().push((
                            stone.name.clone(),
                            role,
                            pinned,
                        ));
                    }
                }

                bag.record_step(
                    format!("overview_{}", stone.name),
                    format!(
                        "{}: {} garden_banks visible",
                        stone.name,
                        garden_banks.len()
                    ),
                    duration.as_millis() as u64,
                    StepResult::ok_with(serde_json::json!({
                        "garden_banks": garden_banks.len(),
                    })),
                );
            }
            Err(e) => {
                bag.record_step(
                    format!("overview_{}", stone.name),
                    format!("{} overview failed", stone.name),
                    duration.as_millis() as u64,
                    StepResult::failed(e.to_string()),
                );
            }
        }
    }

    if banks_by_name.is_empty() {
        bag.record_step(
            "roles_check",
            "No seed banks in garden — nothing to verify",
            0,
            StepResult::skipped("No seed banks"),
        );
        return Ok(bag);
    }

    // For each seed bank name, verify exactly one Primary
    let mut all_ok = true;
    for (name, replicas) in &banks_by_name {
        let primaries: Vec<_> = replicas
            .iter()
            .filter(|(_, role, _)| role == "primary")
            .collect();
        let primary_count = primaries.len();

        if primary_count == 1 {
            let (stone, _, pinned) = &primaries[0];
            bag.record_step(
                format!("role_{}", name),
                format!(
                    "'{}': Primary on {} (pinned={}), {} total replicas",
                    name,
                    stone,
                    pinned,
                    replicas.len()
                ),
                0,
                StepResult::ok_with(serde_json::json!({
                    "name": name,
                    "primary_stone": stone,
                    "pinned": pinned,
                    "replica_count": replicas.len(),
                })),
            );
        } else {
            all_ok = false;
            bag.record_step(
                format!("role_{}", name),
                format!(
                    "'{}': {} primaries (expected 1) — {:?}",
                    name, primary_count, primaries
                ),
                0,
                StepResult::failed(format!(
                    "Expected exactly 1 Primary for '{}', found {}",
                    name, primary_count
                )),
            );
        }
    }

    bag.record_step(
        "roles_summary",
        format!(
            "{} seed bank names checked, {}",
            banks_by_name.len(),
            if all_ok {
                "all have exactly 1 Primary"
            } else {
                "INVARIANT VIOLATED"
            }
        ),
        0,
        if all_ok {
            StepResult::ok()
        } else {
            StepResult::failed("Primary-uniqueness invariant violated")
        },
    );

    Ok(bag)
}

// ============================================================================
// storage.portrait - Verify portrait includes enriched seed bank fields
// ============================================================================

pub fn portrait_enrichment_test() -> TestDef {
    TestDef {
        id: "storage.portrait",
        name: "Portrait Enrichment",
        description: "Verify portrait includes role, pinned, encrypted fields on seed banks",
        category: "storage",
        tags: &["storage", "portrait", "storage-0006"],
        run: |garden, bag| Box::pin(test_portrait_enrichment(garden, bag)),
    }
}

async fn test_portrait_enrichment(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    for stone in &garden.stones {
        let start = Instant::now();
        let result = stone.get_json("/api/v1/portrait").await;
        let duration = start.elapsed();

        match result {
            Ok(resp) => {
                let seed_banks = resp
                    .get("data")
                    .and_then(|d| d.get("seed_banks"))
                    .and_then(|s| s.as_array())
                    .cloned()
                    .unwrap_or_default();

                if seed_banks.is_empty() {
                    bag.record_step(
                        format!("portrait_{}", stone.name),
                        format!("{}: no seed banks in portrait", stone.name),
                        duration.as_millis() as u64,
                        StepResult::skipped("No seed banks on this stone"),
                    );
                    continue;
                }

                let mut all_enriched = true;
                let mut issues = Vec::new();

                for bank in &seed_banks {
                    let name = bank.get("name").and_then(|n| n.as_str()).unwrap_or("?");

                    // Check STORAGE-0006 enrichment fields
                    let has_id = bank.get("id").and_then(|v| v.as_str()).is_some();
                    let has_short_id = bank.get("short_id").and_then(|v| v.as_str()).is_some();
                    let has_role = bank.get("role").and_then(|v| v.as_str()).is_some();
                    let has_pinned = bank.get("pinned").is_some();
                    let has_encrypted = bank.get("encrypted").is_some();

                    if !has_id {
                        all_enriched = false;
                        issues.push(format!("{}: missing 'id'", name));
                    }
                    if !has_short_id {
                        all_enriched = false;
                        issues.push(format!("{}: missing 'short_id'", name));
                    }
                    if !has_role {
                        all_enriched = false;
                        issues.push(format!("{}: missing 'role'", name));
                    }
                    if !has_pinned {
                        all_enriched = false;
                        issues.push(format!("{}: missing 'pinned'", name));
                    }
                    if !has_encrypted {
                        all_enriched = false;
                        issues.push(format!("{}: missing 'encrypted'", name));
                    }
                }

                if all_enriched {
                    bag.record_step(
                        format!("portrait_{}", stone.name),
                        format!(
                            "{}: {} seed banks with full STORAGE-0006 enrichment",
                            stone.name,
                            seed_banks.len()
                        ),
                        duration.as_millis() as u64,
                        StepResult::ok_with(serde_json::json!({
                            "seed_banks": seed_banks.len(),
                            "fields_checked": ["id", "short_id", "role", "pinned", "encrypted"],
                        })),
                    );
                } else {
                    bag.record_step(
                        format!("portrait_{}", stone.name),
                        format!("{}: portrait enrichment incomplete", stone.name),
                        duration.as_millis() as u64,
                        StepResult::failed(format!("Missing fields: {}", issues.join("; "))),
                    );
                }
            }
            Err(e) => {
                bag.record_step(
                    format!("portrait_{}", stone.name),
                    format!("{} portrait fetch failed", stone.name),
                    duration.as_millis() as u64,
                    StepResult::failed(e.to_string()),
                );
            }
        }
    }

    Ok(bag)
}

// ============================================================================
// storage.pin_roundtrip - Pin, verify, unpin roundtrip
// ============================================================================

pub fn pin_roundtrip_test() -> TestDef {
    TestDef {
        id: "storage.pin_roundtrip",
        name: "Pin/Unpin Roundtrip",
        description: "Pin a Primary seed bank, verify pinned flag, then unpin",
        category: "storage",
        tags: &["storage", "pin", "storage-0006"],
        run: |garden, bag| Box::pin(test_pin_roundtrip(garden, bag)),
    }
}

async fn test_pin_roundtrip(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    // Find a stone with a Primary seed bank
    let mut target: Option<(crate::garden::Stone, String)> = None;

    for stone in &garden.stones {
        if let Ok(resp) = stone.get_json("/api/v1/stone/storage").await {
            let garden_banks = resp
                .get("data")
                .and_then(|d| d.get("garden_banks"))
                .and_then(|g| g.as_array())
                .cloned()
                .unwrap_or_default();

            for bank in &garden_banks {
                let is_local = bank
                    .get("is_local")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let role = bank.get("role").and_then(|r| r.as_str()).unwrap_or("");
                let already_pinned = bank
                    .get("pinned")
                    .and_then(|p| p.as_bool())
                    .unwrap_or(false);
                let name = bank.get("name").and_then(|n| n.as_str()).unwrap_or("");

                if is_local && role == "primary" && !already_pinned && !name.is_empty() {
                    target = Some((stone.clone(), name.to_string()));
                    break;
                }
            }
            if target.is_some() {
                break;
            }
        }
    }

    let (stone, bank_name) = match target {
        Some(t) => t,
        None => {
            bag.record_step(
                "pin_roundtrip",
                "No unpinned local Primary seed bank found",
                0,
                StepResult::skipped("No eligible bank for pin test"),
            );
            return Ok(bag);
        }
    };

    bag.record_step(
        "pin_target",
        format!("Target: '{}' on {}", bank_name, stone.name),
        0,
        StepResult::ok(),
    );

    // Step 1: PIN
    let pin_url = format!("/api/v1/stone/storage/banks/{}/pin", bank_name);
    let empty = serde_json::json!({});
    let pin_start = Instant::now();
    let pin_result = stone.post_json(&pin_url, &empty).await;
    let pin_duration = pin_start.elapsed();

    match &pin_result {
        Ok(resp) => {
            let pinned = resp
                .get("data")
                .and_then(|d| d.get("pinned"))
                .and_then(|p| p.as_bool())
                .unwrap_or(false);

            if pinned {
                bag.record_step(
                    "pin",
                    format!("PIN '{}' succeeded", bank_name),
                    pin_duration.as_millis() as u64,
                    StepResult::ok(),
                );
            } else {
                bag.record_step(
                    "pin",
                    format!("PIN '{}' — response.pinned=false", bank_name),
                    pin_duration.as_millis() as u64,
                    StepResult::failed("pinned=false in response"),
                );
                return Ok(bag);
            }
        }
        Err(e) => {
            bag.record_step(
                "pin",
                format!("PIN '{}' failed", bank_name),
                pin_duration.as_millis() as u64,
                StepResult::failed(e.to_string()),
            );
            return Ok(bag);
        }
    }

    // Step 2: Verify pinned flag in storage overview
    let verify_start = Instant::now();
    let verify_result = stone.get_json("/api/v1/stone/storage").await;
    let verify_duration = verify_start.elapsed();

    let is_pinned = verify_result
        .as_ref()
        .ok()
        .and_then(|resp| {
            resp.get("data")
                .and_then(|d| d.get("garden_banks"))
                .and_then(|g| g.as_array())
        })
        .and_then(|banks| {
            banks.iter().find(|b| {
                b.get("name")
                    .and_then(|n| n.as_str())
                    .map(|n| n == bank_name)
                    .unwrap_or(false)
                    && b.get("is_local").and_then(|v| v.as_bool()).unwrap_or(false)
            })
        })
        .and_then(|b| b.get("pinned").and_then(|p| p.as_bool()))
        .unwrap_or(false);

    if is_pinned {
        bag.record_step(
            "verify_pinned",
            format!("Verified '{}' shows pinned=true in overview", bank_name),
            verify_duration.as_millis() as u64,
            StepResult::ok(),
        );
    } else {
        bag.record_step(
            "verify_pinned",
            format!("'{}' not showing pinned=true in overview", bank_name),
            verify_duration.as_millis() as u64,
            StepResult::failed("Pin not reflected in storage overview"),
        );
    }

    // Step 3: UNPIN (always — cleanup)
    let unpin_url = format!("/api/v1/stone/storage/banks/{}/unpin", bank_name);
    let unpin_start = Instant::now();
    let unpin_result = stone.post_json(&unpin_url, &empty).await;
    let unpin_duration = unpin_start.elapsed();

    match &unpin_result {
        Ok(resp) => {
            let pinned = resp
                .get("data")
                .and_then(|d| d.get("pinned"))
                .and_then(|p| p.as_bool())
                .unwrap_or(true);

            bag.record_step(
                "unpin",
                format!("UNPIN '{}' — pinned={}", bank_name, pinned),
                unpin_duration.as_millis() as u64,
                if !pinned {
                    StepResult::ok()
                } else {
                    StepResult::failed("pinned still true after unpin")
                },
            );
        }
        Err(e) => {
            bag.record_step(
                "unpin",
                format!("UNPIN '{}' failed", bank_name),
                unpin_duration.as_millis() as u64,
                StepResult::failed(e.to_string()),
            );
        }
    }

    Ok(bag)
}

// ============================================================================
// storage.replication - Verify changelog/changes endpoint returns valid cursors
// ============================================================================

pub fn replication_test() -> TestDef {
    TestDef {
        id: "storage.replication",
        name: "Replication Changes",
        description: "Write an object and verify the changelog returns a valid cursor",
        category: "storage",
        tags: &["storage", "replication", "storage-0006"],
        run: |garden, bag| Box::pin(test_replication(garden, bag)),
    }
}

async fn test_replication(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let selected = match select_seed_bank(&garden).await {
        Some(s) => s,
        None => {
            bag.record_step(
                "replication",
                "No seed banks available",
                0,
                StepResult::skipped("No seed banks in garden"),
            );
            return Ok(bag);
        }
    };

    let stone = selected.stone;
    let bank_name = &selected.name;

    // Step 1: GET /changes with no cursor (initial sync)
    let changes_path = format!("/api/v1/stone/storage/banks/{}/changes", bank_name);
    let start = Instant::now();
    let result = stone.get_json(&changes_path).await;
    let duration = start.elapsed();

    let initial_cursor = match &result {
        Ok(resp) => {
            let cursor = resp
                .get("data")
                .and_then(|d| d.get("cursor"))
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string();
            let changes = resp
                .get("data")
                .and_then(|d| d.get("changes"))
                .and_then(|c| c.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let full_sync = resp
                .get("data")
                .and_then(|d| d.get("full_sync_required"))
                .and_then(|f| f.as_bool())
                .unwrap_or(false);

            bag.record_step(
                "initial_changes",
                format!(
                    "Initial: {} changes, cursor={}, full_sync={}",
                    changes,
                    if cursor.is_empty() { "none" } else { &cursor },
                    full_sync
                ),
                duration.as_millis() as u64,
                StepResult::ok_with(serde_json::json!({
                    "changes": changes,
                    "cursor": cursor,
                    "full_sync_required": full_sync,
                })),
            );

            cursor
        }
        Err(e) => {
            bag.record_step(
                "initial_changes",
                format!("GET changes failed: {}", e),
                duration.as_millis() as u64,
                StepResult::failed(e.to_string()),
            );
            return Ok(bag);
        }
    };

    // Step 2: Write a probe object to generate a changelog entry
    let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S%3f");
    let probe_key = format!("{}/repl-probe-{}.txt", PROBE_BUCKET, timestamp);
    let put_path = format!("/api/v1/garden/storage/{}/objects/{}", bank_name, probe_key);
    let put_start = Instant::now();
    let put_result = stone
        .put_bytes(&put_path, "text/plain", b"probe-replication-test".to_vec())
        .await;
    let put_duration = put_start.elapsed();

    if let Err(e) = &put_result {
        bag.record_step(
            "write_probe",
            format!("PUT probe object failed: {}", e),
            put_duration.as_millis() as u64,
            StepResult::failed(e.to_string()),
        );
        return Ok(bag);
    }
    bag.record_step(
        "write_probe",
        format!("PUT probe object {}", probe_key),
        put_duration.as_millis() as u64,
        StepResult::ok(),
    );

    // Step 3: GET /changes?since=<initial_cursor> — should see the new entry
    let since_path = if initial_cursor.is_empty() {
        changes_path.clone()
    } else {
        format!("{}?since={}", changes_path, initial_cursor)
    };
    let since_start = Instant::now();
    let since_result = stone.get_json(&since_path).await;
    let since_duration = since_start.elapsed();

    match &since_result {
        Ok(resp) => {
            let cursor = resp
                .get("data")
                .and_then(|d| d.get("cursor"))
                .and_then(|c| c.as_str())
                .unwrap_or("");
            let changes = resp
                .get("data")
                .and_then(|d| d.get("changes"))
                .and_then(|c| c.as_array())
                .cloned()
                .unwrap_or_default();
            let full_sync = resp
                .get("data")
                .and_then(|d| d.get("full_sync_required"))
                .and_then(|f| f.as_bool())
                .unwrap_or(false);

            // Should see at least 1 new change (the PUT we just did)
            let has_our_change = changes.iter().any(|c| {
                c.get("path")
                    .and_then(|p| p.as_str())
                    .map(|p| p == probe_key)
                    .unwrap_or(false)
            });

            let cursor_advanced = !cursor.is_empty()
                && (initial_cursor.is_empty() || cursor > initial_cursor.as_str());

            if has_our_change && cursor_advanced && !full_sync {
                bag.record_step(
                    "since_changes",
                    format!(
                        "Since cursor: {} changes, new cursor={}, probe object found",
                        changes.len(),
                        cursor
                    ),
                    since_duration.as_millis() as u64,
                    StepResult::ok_with(serde_json::json!({
                        "changes": changes.len(),
                        "cursor": cursor,
                        "probe_key_found": true,
                    })),
                );
            } else {
                let mut reasons = Vec::new();
                if !has_our_change {
                    reasons.push("probe object not in changes");
                }
                if !cursor_advanced {
                    reasons.push("cursor did not advance");
                }
                if full_sync {
                    reasons.push("unexpected full_sync_required=true");
                }
                bag.record_step(
                    "since_changes",
                    format!("Changes endpoint issues: {}", reasons.join(", ")),
                    since_duration.as_millis() as u64,
                    StepResult::failed(reasons.join("; ")),
                );
            }
        }
        Err(e) => {
            bag.record_step(
                "since_changes",
                format!("GET changes?since= failed: {}", e),
                since_duration.as_millis() as u64,
                StepResult::failed(e.to_string()),
            );
        }
    }

    // Cleanup: delete the probe object
    let del_path = format!("/api/v1/garden/storage/{}/objects/{}", bank_name, probe_key);
    let _ = stone.delete_status_code(&del_path).await;

    Ok(bag)
}

// ============================================================================
// storage.role_consistency - Verify all stones agree on role assignments
// ============================================================================

pub fn role_consistency_test() -> TestDef {
    TestDef {
        id: "storage.role_consistency",
        name: "Role Consistency",
        description: "Verify all stones report the same Primary for each seed bank name",
        category: "storage",
        tags: &["storage", "orchestration", "consistency", "storage-0006"],
        run: |garden, bag| Box::pin(test_role_consistency(garden, bag)),
    }
}

async fn test_role_consistency(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    if garden.stones.len() < 2 {
        bag.record_step(
            "role_consistency",
            "Single-stone garden — consistency check not applicable",
            0,
            StepResult::skipped("Need 2+ stones"),
        );
        return Ok(bag);
    }

    // Collect garden_banks from each stone
    let mut views: Vec<(String, Vec<(String, String)>)> = Vec::new(); // (stone_name, [(bank_name, role)])

    for stone in &garden.stones {
        let start = Instant::now();
        let result = stone.get_json("/api/v1/stone/storage").await;
        let duration = start.elapsed();

        match result {
            Ok(resp) => {
                let banks: Vec<(String, String)> = resp
                    .get("data")
                    .and_then(|d| d.get("garden_banks"))
                    .and_then(|g| g.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|b| {
                                let name = b.get("name").and_then(|n| n.as_str())?.to_string();
                                let role = b.get("role").and_then(|r| r.as_str())?.to_string();
                                Some((name, role))
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                bag.record_step(
                    format!("view_{}", stone.name),
                    format!("{}: sees {} banks", stone.name, banks.len()),
                    duration.as_millis() as u64,
                    StepResult::ok(),
                );
                views.push((stone.name.clone(), banks));
            }
            Err(e) => {
                bag.record_step(
                    format!("view_{}", stone.name),
                    format!("{}: overview failed", stone.name),
                    duration.as_millis() as u64,
                    StepResult::failed(e.to_string()),
                );
            }
        }
    }

    if views.len() < 2 {
        bag.record_step(
            "role_consistency",
            "Could not get overview from 2+ stones",
            0,
            StepResult::failed("Insufficient data"),
        );
        return Ok(bag);
    }

    // Build primary map per stone: name → role reported by that stone
    // Then compare across stones
    let mut inconsistencies = Vec::new();

    // Collect all bank names
    let all_names: std::collections::HashSet<String> = views
        .iter()
        .flat_map(|(_, banks)| banks.iter().map(|(name, _)| name.clone()))
        .collect();

    for name in &all_names {
        let mut primaries_seen: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        for (stone_name, banks) in &views {
            for (bank_name, role) in banks {
                if bank_name == name && role == "primary" {
                    // Find which stone_id owns this primary
                    primaries_seen.insert(stone_name.clone());
                }
            }
        }

        // All stones should agree on the same set of primaries
        // (Each stone should see the same bank as Primary)
        if primaries_seen.len() > 1 {
            // Not necessarily an error — each stone reports what IT sees
            // But for strong consistency, all stones should agree
            // This is a soft check: just record it
        }
    }

    // Stronger check: compare stone A's view vs stone B's view
    let reference = &views[0];
    let mut all_consistent = true;

    for (stone_name, banks) in views.iter().skip(1) {
        let ref_map: std::collections::HashMap<_, _> = reference.1.iter().cloned().collect();
        let this_map: std::collections::HashMap<_, _> = banks.iter().cloned().collect();

        for name in &all_names {
            let ref_role = ref_map.get(name.as_str()).map(|s| s.as_str());
            let this_role = this_map.get(name.as_str()).map(|s| s.as_str());

            if ref_role != this_role {
                all_consistent = false;
                inconsistencies.push(format!(
                    "'{}': {} says {:?}, {} says {:?}",
                    name, reference.0, ref_role, stone_name, this_role
                ));
            }
        }
    }

    if all_consistent {
        bag.record_step(
            "consistency_check",
            format!(
                "All {} stones agree on roles for {} seed bank names",
                views.len(),
                all_names.len()
            ),
            0,
            StepResult::ok(),
        );
    } else {
        bag.record_step(
            "consistency_check",
            format!("{} inconsistencies found", inconsistencies.len()),
            0,
            StepResult::failed(inconsistencies.join("; ")),
        );
    }

    Ok(bag)
}
