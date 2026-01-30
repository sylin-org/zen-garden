//! Nurturing tests - A/B backup slots and sub-capability discovery
//!
//! Tests for:
//! - Local A/B nurturing slots (create, list, restore)
//! - Sub-capability discovery (models, collections, plugins)
//! - Find services with sub-capability filtering syntax

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
    // Find services via garden topology (services are distributed across stones)
    let mut found_offering: Option<(String, String)> = None;

    // Query tended stone for garden-wide topology
    if let Some(tended) = garden.tended() {
        if let Ok(resp) = tended.get_json("/api/v1/garden").await {
            if let Some(stones) = resp.get("data").and_then(|d| d.get("stones")).and_then(|s| s.as_array()) {
                for stone_info in stones {
                    let stone_name = stone_info.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    if let Some(offerings) = stone_info.get("offerings").and_then(|o| o.as_array()) {
                        for offering in offerings {
                            if let Some(name) = offering.get("name").and_then(|n| n.as_str()) {
                                // Check vitality - we want running services
                                let vitality = offering.get("vitality").and_then(|v| v.as_str()).unwrap_or("");
                                if vitality == "thriving" || vitality == "healthy" {
                                    found_offering = Some((stone_name.to_string(), name.to_string()));
                                    break;
                                }
                            }
                        }
                    }
                    if found_offering.is_some() {
                        break;
                    }
                }
            }
        }
    }

    if found_offering.is_none() {
        bag.record_step(
            "offering_slots",
            "No running services found in garden topology",
            0,
            StepResult::skipped("No services in garden"),
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
    // Find a running service via garden topology
    let mut target: Option<(String, String)> = None;

    if let Some(tended) = garden.tended() {
        if let Ok(resp) = tended.get_json("/api/v1/garden").await {
            if let Some(stones) = resp.get("data").and_then(|d| d.get("stones")).and_then(|s| s.as_array()) {
                for stone_info in stones {
                    let stone_name = stone_info.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    if let Some(offerings) = stone_info.get("offerings").and_then(|o| o.as_array()) {
                        for offering in offerings {
                            if let Some(name) = offering.get("name").and_then(|n| n.as_str()) {
                                let vitality = offering.get("vitality").and_then(|v| v.as_str()).unwrap_or("");
                                if vitality == "thriving" || vitality == "healthy" {
                                    target = Some((stone_name.to_string(), name.to_string()));
                                    break;
                                }
                            }
                        }
                    }
                    if target.is_some() {
                        break;
                    }
                }
            }
        }
    }

    if target.is_none() {
        bag.record_step(
            "create_snapshot",
            "No running services found in garden topology",
            0,
            StepResult::skipped("No running services in garden"),
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
