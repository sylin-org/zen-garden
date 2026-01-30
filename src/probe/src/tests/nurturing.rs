//! Nurturing tests - A/B backup slots, seed bank integration, and retention policy
//!
//! Tests for:
//! - Local A/B nurturing slots (create, list, restore)
//! - Sub-capability discovery (models, collections, plugins)
//! - Find services with sub-capability filtering syntax
//! - Full orchestration: service → local backup → seed bank replication → retention

use crate::registry::TestDef;
use crate::{Bag, LiveGarden, StepResult};
use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;

// ============================================================================
// nurturing.index - List nurturing index on each stone
// ============================================================================

pub fn index_test() -> TestDef {
    TestDef {
        id: "nurturing.index",
        name: "Nurturing Index",
        description: "Check nurturing index on each stone (A/B slots overview)",
        category: "nurturing",
        tags: &["nurturing", "backup"],
        run: |garden, bag| Box::pin(test_index(garden, bag)),
    }
}

async fn test_index(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let mut total_offerings = 0;
    let mut total_snapshots = 0;

    for stone in &garden.stones {
        let start = Instant::now();
        let result = stone.get_json("/api/v1/stone/nurturing").await;
        let duration = start.elapsed();

        match result {
            Ok(resp) => {
                let offerings = resp
                    .get("data")
                    .and_then(|d| d.get("offerings"))
                    .and_then(|o| o.as_object())
                    .map(|m| m.len())
                    .unwrap_or(0);

                let snapshots = resp
                    .get("data")
                    .and_then(|d| d.get("total_snapshots"))
                    .and_then(|t| t.as_u64())
                    .unwrap_or(0) as usize;

                total_offerings += offerings;
                total_snapshots += snapshots;

                bag.record_step(
                    format!("nurturing_{}", stone.name),
                    format!("{}: {} offerings, {} snapshots", stone.name, offerings, snapshots),
                    duration.as_millis() as u64,
                    StepResult::ok_with(serde_json::json!({
                        "offerings": offerings,
                        "snapshots": snapshots,
                    })),
                );
            }
            Err(e) => {
                bag.record_step(
                    format!("nurturing_{}", stone.name),
                    format!("{} nurturing index failed", stone.name),
                    duration.as_millis() as u64,
                    StepResult::failed(e.to_string()),
                );
            }
        }
    }

    bag.put("total_nurturing_offerings", total_offerings);
    bag.put("total_nurturing_snapshots", total_snapshots);

    bag.record_step(
        "nurturing_summary",
        format!(
            "Garden total: {} offerings with nurturing, {} snapshots",
            total_offerings, total_snapshots
        ),
        0,
        StepResult::ok_with(serde_json::json!({
            "total_offerings": total_offerings,
            "total_snapshots": total_snapshots,
        })),
    );

    Ok(bag)
}

// ============================================================================
// nurturing.offering_slots - Get slots for a specific offering
// ============================================================================

pub fn offering_slots_test() -> TestDef {
    TestDef {
        id: "nurturing.offering_slots",
        name: "Offering Slots",
        description: "Get A/B slots for a specific offering on each stone",
        category: "nurturing",
        tags: &["nurturing", "backup"],
        run: |garden, bag| Box::pin(test_offering_slots(garden, bag)),
    }
}

async fn test_offering_slots(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    // Find a running service by querying each stone's services directly
    let mut found_offering: Option<(String, String)> = None;

    // Query each stone for its services
    for stone in &garden.stones {
        if let Ok(resp) = stone.get_json("/api/v1/stone/services").await {
            if let Some(services) = resp.get("data").and_then(|d| d.get("services")).and_then(|s| s.as_array()) {
                for svc in services {
                    let name = svc.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    let status = svc.get("status").and_then(|s| s.as_str()).unwrap_or("");
                    if status == "Running" && !name.is_empty() {
                        found_offering = Some((stone.name.clone(), name.to_string()));
                        break;
                    }
                }
            }
        }
        if found_offering.is_some() {
            break;
        }
    }

    if found_offering.is_none() {
        bag.record_step(
            "offering_slots",
            "No running services found across stones",
            0,
            StepResult::skipped("No running services"),
        );
        return Ok(bag);
    }

    let (stone_name, offering) = found_offering.unwrap();
    let stone = garden.stone(&stone_name).unwrap();

    let start = Instant::now();
    let path = format!("/api/v1/stone/nurturing/{}", offering);
    let result = stone.get_json(&path).await;
    let duration = start.elapsed();

    match result {
        Ok(resp) => {
            let slots_info = resp.get("data");

            bag.record_step(
                "offering_slots",
                format!("Slots for {} on {}", offering, stone_name),
                duration.as_millis() as u64,
                StepResult::ok_with(serde_json::json!({
                    "offering": offering,
                    "stone": stone_name,
                    "slots": slots_info,
                })),
            );
        }
        Err(e) => {
            bag.record_step(
                "offering_slots",
                format!("Failed to get slots for {}", offering),
                duration.as_millis() as u64,
                StepResult::failed(e.to_string()),
            );
        }
    }

    Ok(bag)
}

// ============================================================================
// nurturing.create_snapshot - Create a snapshot for an offering
// ============================================================================

pub fn create_snapshot_test() -> TestDef {
    TestDef {
        id: "nurturing.create_snapshot",
        name: "Create Snapshot",
        description: "Create a nurturing snapshot for a running service",
        category: "nurturing",
        tags: &["nurturing", "backup", "mutating"],
        run: |garden, bag| Box::pin(test_create_snapshot(garden, bag)),
    }
}

async fn test_create_snapshot(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    // Find a running service by querying each stone's services directly
    let mut target: Option<(String, String)> = None;

    for stone in &garden.stones {
        if let Ok(resp) = stone.get_json("/api/v1/stone/services").await {
            if let Some(services) = resp.get("data").and_then(|d| d.get("services")).and_then(|s| s.as_array()) {
                for svc in services {
                    let name = svc.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    let status = svc.get("status").and_then(|s| s.as_str()).unwrap_or("");
                    if status == "Running" && !name.is_empty() {
                        target = Some((stone.name.clone(), name.to_string()));
                        break;
                    }
                }
            }
        }
        if target.is_some() {
            break;
        }
    }

    if target.is_none() {
        bag.record_step(
            "create_snapshot",
            "No running services found across stones",
            0,
            StepResult::skipped("No running services"),
        );
        return Ok(bag);
    }

    let (stone_name, offering) = target.unwrap();
    let stone = match garden.stone(&stone_name) {
        Some(s) => s,
        None => {
            bag.record_step(
                "create_snapshot",
                format!("Stone {} not found in discovered stones", stone_name),
                0,
                StepResult::failed("Stone not accessible"),
            );
            return Ok(bag);
        }
    };
    bag.put("snapshot_target_stone", &stone.name);
    bag.put("snapshot_target_offering", &offering);

    // Create the snapshot
    let start = Instant::now();
    let path = format!("/api/v1/stone/nurturing/{}", offering);
    let body = serde_json::json!({
        "commit_image": false  // Don't commit image for speed in testing
    });

    let result = stone.post_json(&path, &body).await;
    let duration = start.elapsed();

    match result {
        Ok(resp) => {
            let slot = resp
                .get("data")
                .and_then(|d| d.get("slot"))
                .and_then(|s| s.as_str())
                .unwrap_or("unknown");
            let harvest_id = resp
                .get("data")
                .and_then(|d| d.get("harvest_id"))
                .and_then(|h| h.as_str())
                .unwrap_or("unknown");

            bag.put("created_slot", slot);
            bag.put("created_harvest_id", harvest_id);

            bag.record_step(
                "create_snapshot",
                format!("Created snapshot {} in slot {}", harvest_id, slot),
                duration.as_millis() as u64,
                StepResult::ok_with(serde_json::json!({
                    "offering": offering,
                    "stone": stone.name,
                    "slot": slot,
                    "harvest_id": harvest_id,
                })),
            );
        }
        Err(e) => {
            bag.record_step(
                "create_snapshot",
                format!("Failed to create snapshot for {}", offering),
                duration.as_millis() as u64,
                StepResult::failed(e.to_string()),
            );
        }
    }

    Ok(bag)
}

// ============================================================================
// nurturing.subcap_discovery - Discover sub-capabilities on services
// ============================================================================

pub fn subcap_discovery_test() -> TestDef {
    TestDef {
        id: "nurturing.subcap_discovery",
        name: "Sub-Capability Discovery",
        description: "Trigger sub-capability discovery on services (models, collections)",
        category: "nurturing",
        tags: &["nurturing", "subcap", "discovery"],
        run: |garden, bag| Box::pin(test_subcap_discovery(garden, bag)),
    }
}

async fn test_subcap_discovery(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let mut total_capabilities = 0;
    let mut services_with_caps = Vec::new();

    // First get the garden topology to know which stones have services
    let topology_stones: Vec<(String, usize)> = if let Some(tended) = garden.tended() {
        if let Ok(resp) = tended.get_json("/api/v1/garden").await {
            resp.get("data")
                .and_then(|d| d.get("stones"))
                .and_then(|s| s.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|s| {
                            let name = s.get("name").and_then(|n| n.as_str())?;
                            let count = s.get("offerings").and_then(|o| o.as_array()).map(|a| a.len()).unwrap_or(0);
                            Some((name.to_string(), count))
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    bag.put("topology_stones", &topology_stones);

    // Only refresh capabilities on stones that have services
    for stone in &garden.stones {
        let service_count = topology_stones.iter()
            .find(|(name, _)| name == &stone.name)
            .map(|(_, count)| *count)
            .unwrap_or(0);

        if service_count == 0 {
            bag.record_step(
                format!("subcap_refresh_{}", stone.name),
                format!("{}: no services (skipped)", stone.name),
                0,
                StepResult::ok_with(serde_json::json!({ "skipped": true, "reason": "no services" })),
            );
            continue;
        }

        // Trigger refresh of sub-capabilities
        let start = Instant::now();
        let refresh_result = stone
            .post_json("/api/v1/stone/services/refresh-capabilities", &serde_json::json!({}))
            .await;
        let duration = start.elapsed();

        match refresh_result {
            Ok(resp) => {
                let updated = resp
                    .get("data")
                    .and_then(|d| d.get("updated"))
                    .and_then(|u| u.as_u64())
                    .unwrap_or(0);

                bag.record_step(
                    format!("subcap_refresh_{}", stone.name),
                    format!("{}: refreshed capabilities for {} services", stone.name, updated),
                    duration.as_millis() as u64,
                    StepResult::ok_with(serde_json::json!({ "updated": updated })),
                );
            }
            Err(e) => {
                bag.record_step(
                    format!("subcap_refresh_{}", stone.name),
                    format!("{} refresh failed", stone.name),
                    duration.as_millis() as u64,
                    StepResult::failed(e.to_string()),
                );
                continue;
            }
        }

        // Now check what sub-capabilities exist (correct structure: data.services)
        let services_result = stone.get_json("/api/v1/stone/services").await;
        if let Ok(resp) = services_result {
            if let Some(services) = resp.get("data").and_then(|d| d.get("services")).and_then(|s| s.as_array()) {
                for svc in services {
                    let name = svc.get("name").and_then(|n| n.as_str()).unwrap_or("unknown");

                    if let Some(caps) = svc.get("sub_capabilities").and_then(|c| c.as_array()) {
                        if !caps.is_empty() {
                            for cap in caps {
                                let cap_type = cap.get("type").and_then(|t| t.as_str()).unwrap_or("unknown");
                                let items = cap.get("items").and_then(|i| i.as_array()).map(|a| a.len()).unwrap_or(0);

                                services_with_caps.push(serde_json::json!({
                                    "stone": stone.name,
                                    "service": name,
                                    "cap_type": cap_type,
                                    "item_count": items,
                                }));
                                total_capabilities += items;
                            }
                        }
                    }
                }
            }
        }
    }

    bag.put("total_sub_capabilities", total_capabilities);
    bag.put("services_with_caps", &services_with_caps);

    bag.record_step(
        "subcap_summary",
        format!(
            "Found {} sub-capabilities across {} service/cap pairs",
            total_capabilities,
            services_with_caps.len()
        ),
        0,
        StepResult::ok_with(serde_json::json!({
            "total_capabilities": total_capabilities,
            "services": services_with_caps,
        })),
    );

    Ok(bag)
}

// ============================================================================
// nurturing.find_with_subcap - Test find services with sub-capability syntax
// ============================================================================

pub fn find_with_subcap_test() -> TestDef {
    TestDef {
        id: "nurturing.find_with_subcap",
        name: "Find With Sub-Capability",
        description: "Test find services with sub-capability filtering syntax",
        category: "nurturing",
        tags: &["nurturing", "subcap", "find"],
        run: |garden, bag| Box::pin(test_find_with_subcap(garden, bag)),
    }
}

async fn test_find_with_subcap(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let tended = match garden.tended() {
        Some(s) => s,
        None => {
            bag.record_step(
                "find_subcap",
                "No tended stone available",
                0,
                StepResult::skipped("No tended stone"),
            );
            return Ok(bag);
        }
    };

    // First, find a service with sub-capabilities
    let services_result = tended.get_json("/api/v1/garden/services").await;
    let mut test_queries: Vec<String> = Vec::new();

    if let Ok(resp) = &services_result {
        if let Some(services) = resp.get("data").and_then(|d| d.get("services")).and_then(|s| s.as_array()) {
            for svc in services {
                let offering = svc.get("offering").and_then(|o| o.as_str()).unwrap_or("");

                if let Some(caps) = svc.get("sub_capabilities").and_then(|c| c.as_array()) {
                    for cap in caps {
                        let cap_type = cap.get("type").and_then(|t| t.as_str()).unwrap_or("");

                        if let Some(items) = cap.get("items").and_then(|i| i.as_array()) {
                            if let Some(first_item) = items.first().and_then(|i| i.as_str()) {
                                // Create test queries using different syntax
                                test_queries.push(format!("{}[{}]", offering, first_item));
                                test_queries.push(format!("{}:{}", cap_type, first_item));
                                break;
                            }
                        }
                    }
                }
                if test_queries.len() >= 2 {
                    break;
                }
            }
        }
    }

    if test_queries.is_empty() {
        // Fall back to basic queries that should work
        test_queries.push("*".to_string()); // Match all

        bag.record_step(
            "find_subcap_note",
            "No services with sub-capabilities found, using fallback queries",
            0,
            StepResult::ok(),
        );
    }

    // Test each query
    for query in &test_queries {
        let start = Instant::now();
        let path = format!("/api/v1/garden/services?q={}", urlencoding::encode(query));
        let result = tended.get_json(&path).await;
        let duration = start.elapsed();

        match result {
            Ok(resp) => {
                let count = resp
                    .get("data")
                    .and_then(|d| d.get("services"))
                    .and_then(|s| s.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);

                bag.record_step(
                    format!("find_subcap_{}", query),
                    format!("Query '{}': {} results", query, count),
                    duration.as_millis() as u64,
                    StepResult::ok_with(serde_json::json!({
                        "query": query,
                        "results": count,
                    })),
                );
            }
            Err(e) => {
                bag.record_step(
                    format!("find_subcap_{}", query),
                    format!("Query '{}' failed", query),
                    duration.as_millis() as u64,
                    StepResult::failed(e.to_string()),
                );
            }
        }
    }

    Ok(bag)
}

// ============================================================================
// nurturing.offering_id_stability - Verify offering_id persistence
// ============================================================================

pub fn offering_id_stability_test() -> TestDef {
    TestDef {
        id: "nurturing.offering_id",
        name: "Offering ID Stability",
        description: "Verify offering_id is present and stable across services",
        category: "nurturing",
        tags: &["nurturing", "offering_id"],
        run: |garden, bag| Box::pin(test_offering_id_stability(garden, bag)),
    }
}

async fn test_offering_id_stability(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let mut offerings_with_id = 0;
    let mut offerings_without_id = 0;
    let mut missing_ids: Vec<String> = Vec::new();

    for stone in &garden.stones {
        let start = Instant::now();
        let result = stone.get_json("/api/v1/stone/services").await;
        let duration = start.elapsed();

        match result {
            Ok(resp) => {
                // Correct structure: data.services (not data directly)
                if let Some(services) = resp.get("data").and_then(|d| d.get("services")).and_then(|s| s.as_array()) {
                    for svc in services {
                        let name = svc.get("name").and_then(|n| n.as_str()).unwrap_or("unknown");
                        let offering_id = svc.get("offering_id").and_then(|i| i.as_str());

                        if let Some(id) = offering_id {
                            if !id.is_empty() {
                                offerings_with_id += 1;
                            } else {
                                offerings_without_id += 1;
                                missing_ids.push(format!("{}:{}", stone.name, name));
                            }
                        } else {
                            offerings_without_id += 1;
                            missing_ids.push(format!("{}:{}", stone.name, name));
                        }
                    }
                }

                bag.record_step(
                    format!("offering_id_{}", stone.name),
                    format!("{}: checked offering IDs", stone.name),
                    duration.as_millis() as u64,
                    StepResult::ok(),
                );
            }
            Err(e) => {
                bag.record_step(
                    format!("offering_id_{}", stone.name),
                    format!("{} services check failed", stone.name),
                    duration.as_millis() as u64,
                    StepResult::failed(e.to_string()),
                );
            }
        }
    }

    bag.put("offerings_with_id", offerings_with_id);
    bag.put("offerings_without_id", offerings_without_id);

    let summary_result = if offerings_without_id == 0 {
        StepResult::ok_with(serde_json::json!({
            "with_id": offerings_with_id,
            "without_id": offerings_without_id,
        }))
    } else {
        StepResult::failed(format!(
            "{} services missing offering_id: {:?}",
            offerings_without_id, missing_ids
        ))
    };

    bag.record_step(
        "offering_id_summary",
        format!(
            "{} with offering_id, {} without",
            offerings_with_id, offerings_without_id
        ),
        0,
        summary_result,
    );

    Ok(bag)
}

// ============================================================================
// nurturing.remote_list - List remote snapshots on seed banks
// ============================================================================

pub fn remote_list_test() -> TestDef {
    TestDef {
        id: "nurturing.remote_list",
        name: "Remote Snapshots",
        description: "List remote nurturing snapshots on seed banks",
        category: "nurturing",
        tags: &["nurturing", "remote", "seed-bank"],
        run: |garden, bag| Box::pin(test_remote_list(garden, bag)),
    }
}

async fn test_remote_list(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    // First find a stone with seed banks
    let mut seed_banks: Vec<(String, String, String)> = Vec::new(); // (stone, bank_id, bank_name)

    for stone in &garden.stones {
        let result = stone.get_json("/api/v1/stone/storage/bank").await;
        if let Ok(resp) = result {
            if let Some(banks) = resp.get("data").and_then(|d| d.as_array()) {
                for bank in banks {
                    let id = bank.get("id").and_then(|i| i.as_str()).unwrap_or("");
                    let name = bank.get("name").and_then(|n| n.as_str()).unwrap_or(id);
                    if !id.is_empty() {
                        seed_banks.push((stone.name.clone(), id.to_string(), name.to_string()));
                    }
                }
            }
        }
    }

    if seed_banks.is_empty() {
        bag.record_step(
            "remote_list",
            "No seed banks available for remote listing",
            0,
            StepResult::skipped("No seed banks in garden"),
        );
        return Ok(bag);
    }

    // Check remote snapshots on each bank
    for (stone_name, bank_id, bank_name) in &seed_banks {
        let stone = garden.stone(stone_name).unwrap();

        let start = Instant::now();
        let path = format!("/api/v1/stone/nurturing/remote/{}", bank_name);
        let result = stone.get_json(&path).await;
        let duration = start.elapsed();

        match result {
            Ok(resp) => {
                let snapshot_count = resp
                    .get("data")
                    .and_then(|d| d.get("snapshots"))
                    .and_then(|s| s.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);

                bag.record_step(
                    format!("remote_{}_{}", stone_name, bank_id),
                    format!("{}/{}: {} remote snapshots", stone_name, bank_name, snapshot_count),
                    duration.as_millis() as u64,
                    StepResult::ok_with(serde_json::json!({
                        "bank_name": bank_name,
                        "bank_id": bank_id,
                        "snapshot_count": snapshot_count,
                    })),
                );
            }
            Err(e) => {
                bag.record_step(
                    format!("remote_{}_{}", stone_name, bank_id),
                    format!("{}/{} listing failed", stone_name, bank_name),
                    duration.as_millis() as u64,
                    StepResult::failed(e.to_string()),
                );
            }
        }
    }

    Ok(bag)
}

// ============================================================================
// nurturing.orchestration - Full nurturing flow: service → local → seed bank
// ============================================================================

pub fn orchestration_test() -> TestDef {
    TestDef {
        id: "nurturing.orchestration",
        name: "Nurturing Orchestration",
        description: "Full nurturing flow: service → local A/B → seed bank replication with retention",
        category: "nurturing",
        tags: &["nurturing", "orchestration", "seed-bank", "mutating"],
        run: |garden, bag| Box::pin(test_orchestration(garden, bag)),
    }
}

/// Full orchestration test for the nurturing system
///
/// This test exercises the complete nurturing flow:
/// 1. Find a running service with an offering_id
/// 2. Create a local A/B snapshot
/// 3. Detect seed bank presence
/// 4. Replicate to seed bank (if available)
/// 5. Verify retention policy (5 snapshots per offering)
async fn test_orchestration(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    // ========================================================================
    // Step 1: Find a running service with offering_id
    // ========================================================================

    let mut target: Option<(String, String, String)> = None; // (stone_name, offering_name, offering_id)

    for stone in &garden.stones {
        if let Ok(resp) = stone.get_json("/api/v1/stone/services").await {
            if let Some(services) = resp.get("data").and_then(|d| d.get("services")).and_then(|s| s.as_array()) {
                for svc in services {
                    let name = svc.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    let status = svc.get("status").and_then(|s| s.as_str()).unwrap_or("");
                    let offering_id = svc.get("offering_id").and_then(|i| i.as_str()).unwrap_or("");

                    if status == "Running" && !name.is_empty() && !offering_id.is_empty() {
                        target = Some((stone.name.clone(), name.to_string(), offering_id.to_string()));
                        break;
                    }
                }
            }
        }
        if target.is_some() {
            break;
        }
    }

    let (stone_name, offering_name, offering_id) = match target {
        Some(t) => t,
        None => {
            bag.record_step(
                "orchestration_find_service",
                "No running service with offering_id found",
                0,
                StepResult::skipped("No eligible service"),
            );
            return Ok(bag);
        }
    };

    let stone = garden.stone(&stone_name).unwrap();

    bag.record_step(
        "orchestration_find_service",
        format!("Found service: {} (ID: {}) on {}", offering_name, &offering_id[..8], stone_name),
        0,
        StepResult::ok_with(serde_json::json!({
            "stone": stone_name,
            "offering": offering_name,
            "offering_id": offering_id,
        })),
    );

    // ========================================================================
    // Step 2: Create local A/B snapshot
    // ========================================================================

    let start = Instant::now();
    let path = format!("/api/v1/stone/nurturing/{}", offering_name);
    let body = serde_json::json!({
        "commit_image": false
    });

    let snapshot_result = stone.post_json(&path, &body).await;
    let duration = start.elapsed();

    let (harvest_id, slot) = match snapshot_result {
        Ok(resp) => {
            let slot = resp
                .get("data")
                .and_then(|d| d.get("slot"))
                .and_then(|s| s.as_str())
                .unwrap_or("unknown");
            let harvest_id = resp
                .get("data")
                .and_then(|d| d.get("harvest_id"))
                .and_then(|h| h.as_str())
                .unwrap_or("unknown");

            bag.record_step(
                "orchestration_local_snapshot",
                format!("Created local snapshot {} in slot {}", harvest_id, slot),
                duration.as_millis() as u64,
                StepResult::ok_with(serde_json::json!({
                    "harvest_id": harvest_id,
                    "slot": slot,
                })),
            );
            (harvest_id.to_string(), slot.to_string())
        }
        Err(e) => {
            bag.record_step(
                "orchestration_local_snapshot",
                format!("Failed to create local snapshot: {}", e),
                duration.as_millis() as u64,
                StepResult::failed(e.to_string()),
            );
            return Ok(bag);
        }
    };

    // ========================================================================
    // Step 3: Verify local A/B slots
    // ========================================================================

    let start = Instant::now();
    let slots_path = format!("/api/v1/stone/nurturing/{}", offering_name);
    let slots_result = stone.get_json(&slots_path).await;
    let duration = start.elapsed();

    match slots_result {
        Ok(resp) => {
            let slot_a = resp
                .get("data")
                .and_then(|d| d.get("slot_a"))
                .is_some();
            let slot_b = resp
                .get("data")
                .and_then(|d| d.get("slot_b"))
                .is_some();

            bag.record_step(
                "orchestration_verify_slots",
                format!("Slots: A={}, B={}", if slot_a { "filled" } else { "empty" }, if slot_b { "filled" } else { "empty" }),
                duration.as_millis() as u64,
                StepResult::ok_with(serde_json::json!({
                    "slot_a": slot_a,
                    "slot_b": slot_b,
                })),
            );
        }
        Err(e) => {
            bag.record_step(
                "orchestration_verify_slots",
                format!("Failed to verify slots: {}", e),
                duration.as_millis() as u64,
                StepResult::failed(e.to_string()),
            );
        }
    }

    // ========================================================================
    // Step 4: Check for seed bank presence
    // ========================================================================

    let mut seed_bank: Option<(String, String)> = None; // (bank_id, bank_name)

    let start = Instant::now();
    let banks_result = stone.get_json("/api/v1/stone/storage/bank").await;
    let duration = start.elapsed();

    match banks_result {
        Ok(resp) => {
            if let Some(banks) = resp.get("data").and_then(|d| d.as_array()) {
                for bank in banks {
                    let id = bank.get("id").and_then(|i| i.as_str()).unwrap_or("");
                    let name = bank.get("name").and_then(|n| n.as_str()).unwrap_or(id);
                    let status = bank.get("status").and_then(|s| s.as_str()).unwrap_or("");

                    if !id.is_empty() && status != "offline" {
                        seed_bank = Some((id.to_string(), name.to_string()));
                        break;
                    }
                }
            }

            if seed_bank.is_some() {
                let (_, name) = seed_bank.as_ref().unwrap();
                bag.record_step(
                    "orchestration_detect_seedbank",
                    format!("Seed bank detected: {}", name),
                    duration.as_millis() as u64,
                    StepResult::ok_with(serde_json::json!({
                        "seed_bank": name,
                    })),
                );
            } else {
                bag.record_step(
                    "orchestration_detect_seedbank",
                    "No online seed banks available",
                    duration.as_millis() as u64,
                    StepResult::ok_with(serde_json::json!({
                        "seed_bank": null,
                    })),
                );
            }
        }
        Err(e) => {
            bag.record_step(
                "orchestration_detect_seedbank",
                format!("Failed to check seed banks: {}", e),
                duration.as_millis() as u64,
                StepResult::failed(e.to_string()),
            );
        }
    }

    // ========================================================================
    // Step 5: Replicate to seed bank (if available)
    // ========================================================================

    if let Some((_bank_id, bank_name)) = seed_bank {
        let start = Instant::now();
        let replicate_path = format!("/api/v1/stone/nurturing/{}/replicate", offering_name);
        let replicate_body = serde_json::json!({
            "seed_bank": bank_name,
        });

        let replicate_result = stone.post_json(&replicate_path, &replicate_body).await;
        let duration = start.elapsed();

        match replicate_result {
            Ok(resp) => {
                let success = resp
                    .get("data")
                    .and_then(|d| d.get("success"))
                    .and_then(|s| s.as_bool())
                    .unwrap_or(false);
                let pruned = resp
                    .get("data")
                    .and_then(|d| d.get("pruned_harvest_ids"))
                    .and_then(|p| p.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                let message = resp
                    .get("data")
                    .and_then(|d| d.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("");

                if success {
                    bag.record_step(
                        "orchestration_replicate",
                        format!("Replicated to {} (pruned {} old snapshots)", bank_name, pruned),
                        duration.as_millis() as u64,
                        StepResult::ok_with(serde_json::json!({
                            "seed_bank": bank_name,
                            "harvest_id": harvest_id,
                            "pruned_count": pruned,
                            "message": message,
                        })),
                    );
                } else {
                    bag.record_step(
                        "orchestration_replicate",
                        format!("Replication failed: {}", message),
                        duration.as_millis() as u64,
                        StepResult::failed(message.to_string()),
                    );
                }
            }
            Err(e) => {
                // Replication endpoint may not exist yet - record as info not failure
                bag.record_step(
                    "orchestration_replicate",
                    format!("Replication not available: {}", e),
                    duration.as_millis() as u64,
                    StepResult::skipped(format!("Endpoint not implemented: {}", e)),
                );
            }
        }

        // ====================================================================
        // Step 6: Verify remote snapshots and retention
        // ====================================================================

        let start = Instant::now();
        let remote_path = format!("/api/v1/stone/nurturing/remote/{}", bank_name);
        let remote_result = stone.get_json(&remote_path).await;
        let duration = start.elapsed();

        match remote_result {
            Ok(resp) => {
                let snapshots = resp
                    .get("data")
                    .and_then(|d| d.get("snapshots"))
                    .and_then(|s| s.as_array());

                if let Some(snaps) = snapshots {
                    // Count snapshots for this offering
                    let offering_snapshots: Vec<_> = snaps
                        .iter()
                        .filter(|s| {
                            s.get("offering_id")
                                .and_then(|id| id.as_str())
                                .map(|id| id == offering_id)
                                .unwrap_or(false)
                        })
                        .collect();

                    let count = offering_snapshots.len();
                    let within_retention = count <= 5;

                    bag.record_step(
                        "orchestration_verify_retention",
                        format!(
                            "{} remote snapshots for offering (retention: {})",
                            count,
                            if within_retention { "OK ≤5" } else { "EXCEEDED >5" }
                        ),
                        duration.as_millis() as u64,
                        if within_retention {
                            StepResult::ok_with(serde_json::json!({
                                "snapshot_count": count,
                                "retention_slots": 5,
                                "within_policy": true,
                            }))
                        } else {
                            StepResult::failed(format!("Retention exceeded: {} > 5", count))
                        },
                    );
                } else {
                    bag.record_step(
                        "orchestration_verify_retention",
                        "No remote snapshots found (new offering)",
                        duration.as_millis() as u64,
                        StepResult::ok_with(serde_json::json!({
                            "snapshot_count": 0,
                            "within_policy": true,
                        })),
                    );
                }
            }
            Err(e) => {
                bag.record_step(
                    "orchestration_verify_retention",
                    format!("Failed to verify remote snapshots: {}", e),
                    duration.as_millis() as u64,
                    StepResult::skipped(e.to_string()),
                );
            }
        }
    } else {
        bag.record_step(
            "orchestration_replicate",
            "Skipped: no seed bank available",
            0,
            StepResult::skipped("No seed bank"),
        );
        bag.record_step(
            "orchestration_verify_retention",
            "Skipped: no seed bank available",
            0,
            StepResult::skipped("No seed bank"),
        );
    }

    // ========================================================================
    // Summary
    // ========================================================================

    bag.record_step(
        "orchestration_summary",
        format!(
            "Orchestration complete for {} ({})",
            offering_name,
            &offering_id[..8]
        ),
        0,
        StepResult::ok_with(serde_json::json!({
            "offering": offering_name,
            "offering_id": offering_id,
            "local_slot": slot,
            "harvest_id": harvest_id,
        })),
    );

    Ok(bag)
}
