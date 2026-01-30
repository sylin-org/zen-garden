//! Companion tests - registry, command forwarding, health

use crate::registry::TestDef;
use crate::{Bag, LiveGarden, StepResult};
use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;

// ============================================================================
// Companions.registry - Verify Companion registry on each stone
// ============================================================================

pub fn registry_test() -> TestDef {
    TestDef {
        id: "Companions.registry",
        name: "Companion Registry",
        description: "Check registered Companions on each stone",
        category: "companions",
        tags: &["companions", "registry"],
        run: |garden, bag| Box::pin(test_registry(garden, bag)),
    }
}

async fn test_registry(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let mut total_companions = 0;
    let mut running_companions = 0;

    for stone in &garden.stones {
        let start = Instant::now();
        let result = stone.get_json("/api/v1/stone/companions").await;
        let duration = start.elapsed();

        match result {
            Ok(resp) => {
                // API returns { data: { Companions: [...] } }
                let companions = resp
                    .get("data")
                    .and_then(|d| d.get("companions"))
                    .and_then(|d| d.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|a| {
                                let name = a.get("id").and_then(|n| n.as_str())?;
                                let running = a
                                    .get("running")
                                    .and_then(|s| s.as_bool())
                                    .unwrap_or(false);
                                let status = if running { "running" } else { "stopped" };
                                Some((name.to_string(), status.to_string()))
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();

                let count = companions.len();
                let running = companions.iter().filter(|(_, s)| s == "running").count();

                total_companions += count;
                running_companions += running;

                bag.record_step(
                    format!("companions_{}", stone.name),
                    format!("{}: {} companions ({} running)", stone.name, count, running),
                    duration.as_millis() as u64,
                    StepResult::ok_with(serde_json::json!({
                        "companions": companions,
                        "count": count,
                        "running": running,
                    })),
                );
            }
            Err(e) => {
                bag.record_step(
                    format!("companions_{}", stone.name),
                    format!("{} Companion registry check failed", stone.name),
                    duration.as_millis() as u64,
                    StepResult::failed(e.to_string()),
                );
            }
        }
    }

    bag.put("total_companions", total_companions);
    bag.put("running_companions", running_companions);

    Ok(bag)
}

// ============================================================================
// Companions.cricket - Check Cricket audio Companion specifically
// ============================================================================

pub fn cricket_test() -> TestDef {
    TestDef {
        id: "Companions.cricket",
        name: "Cricket Companion",
        description: "Verify Cricket audio Companion is running and responsive",
        category: "companions",
        tags: &["companions", "cricket", "audio"],
        run: |garden, bag| Box::pin(test_cricket(garden, bag)),
    }
}

async fn test_cricket(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let mut found_cricket = false;

    for stone in &garden.stones {
        let start = Instant::now();
        let result = stone.get_json("/api/v1/stone/companions/cricket").await;
        let duration = start.elapsed();

        match result {
            Ok(resp) => {
                found_cricket = true;

                // API returns { data: { running: bool, port: u16, ... } }
                let running = resp
                    .get("data")
                    .and_then(|d| d.get("running"))
                    .and_then(|s| s.as_bool())
                    .unwrap_or(false);

                let port = resp
                    .get("data")
                    .and_then(|d| d.get("port"))
                    .and_then(|p| p.as_u64());

                let status = if running { "running" } else { "stopped" };

                if running {
                    // Play a sound to verify Cricket is working
                    let play_result = stone
                        .post_json(
                            "/api/v1/stone/companions/cricket/command",
                            &serde_json::json!({
                                "companion": "cricket",
                                "raw_args": ["play", "stone-online"]
                            }),
                        )
                        .await;

                    let play_ok = play_result
                        .as_ref()
                        .ok()
                        .and_then(|r| r.get("status"))
                        .and_then(|s| s.as_str())
                        .map(|s| s == "success")
                        .unwrap_or(false);

                    bag.record_step(
                        format!("cricket_{}", stone.name),
                        format!("{}: Cricket {} (port {:?}) - played sound: {}", 
                            stone.name, status, port, if play_ok { "?" } else { "?" }),
                        duration.as_millis() as u64,
                        if play_ok {
                            StepResult::ok_with(serde_json::json!({
                                "status": status,
                                "port": port,
                                "sound_played": true,
                            }))
                        } else {
                            StepResult::failed("Cricket running but failed to play sound")
                        },
                    );
                } else {
                    bag.record_step(
                        format!("cricket_{}", stone.name),
                        format!("{}: Cricket {} (port {:?})", stone.name, status, port),
                        duration.as_millis() as u64,
                        StepResult::skipped(format!("Cricket registered but {}", status)),
                    );
                }
            }
            Err(e) => {
                // Cricket may not be installed on all stones
                let is_not_found = e.to_string().contains("404")
                    || e.to_string().contains("not found")
                    || e.to_string().contains("NOT_FOUND");

                if is_not_found {
                    bag.record_step(
                        format!("cricket_{}", stone.name),
                        format!("{}: Cricket not installed", stone.name),
                        duration.as_millis() as u64,
                        StepResult::skipped("Cricket Companion not installed"),
                    );
                } else {
                    bag.record_step(
                        format!("cricket_{}", stone.name),
                        format!("{}: Cricket check failed", stone.name),
                        duration.as_millis() as u64,
                        StepResult::failed(e.to_string()),
                    );
                }
            }
        }
    }

    if !found_cricket {
        bag.record_step(
            "cricket_summary",
            "No Cricket Companions found in garden",
            0,
            StepResult::skipped("Cricket not installed on any stone"),
        );
    }

    Ok(bag)
}

// ============================================================================
// Companions.command_forwarding - Test command forwarding to Companions
// ============================================================================

pub fn command_forwarding_test() -> TestDef {
    TestDef {
        id: "Companions.command_forwarding",
        name: "Command Forwarding",
        description: "Test command forwarding to running Companions",
        category: "companions",
        tags: &["companions", "commands"],
        run: |garden, bag| Box::pin(test_command_forwarding(garden, bag)),
    }
}

async fn test_command_forwarding(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    // Find a stone with Cricket running
    for stone in &garden.stones {
        let companion_result = stone.get_json("/api/v1/stone/companions/cricket").await;

        if let Ok(resp) = companion_result {
            // API returns { data: { running: bool, ... } }
            let running = resp
                .get("data")
                .and_then(|d| d.get("running"))
                .and_then(|s| s.as_bool())
                .unwrap_or(false);

            if !running {
                continue;
            }

            // Try sending a safe command (list tunes)
            let start = Instant::now();
            let cmd_result = stone
                .post_json(
                    "/api/v1/stone/companions/cricket/command",
                    &serde_json::json!({
                        "companion": "cricket",
                        "raw_args": ["list"]
                    }),
                )
                .await;
            let duration = start.elapsed();

            match cmd_result {
                Ok(resp) => {
                    // CommandResponse has { status: "success"|"error", output, message }
                    let status = resp
                        .get("status")
                        .and_then(|s| s.as_str())
                        .unwrap_or("unknown");
                    let success = status == "success";

                    bag.record_step(
                        format!("command_{}", stone.name),
                        format!("{}: command forwarding {}", stone.name, if success { "works" } else { "failed" }),
                        duration.as_millis() as u64,
                        if success {
                            StepResult::ok_with(serde_json::json!({
                                "stone": stone.name,
                                "companion": "cricket",
                                "command": "list",
                            }))
                        } else {
                            StepResult::failed(format!("Command returned status={}", status))
                        },
                    );

                    // Found and tested one, that's enough
                    return Ok(bag);
                }
                Err(e) => {
                    bag.record_step(
                        format!("command_{}", stone.name),
                        format!("{}: command forwarding failed", stone.name),
                        duration.as_millis() as u64,
                        StepResult::failed(e.to_string()),
                    );
                }
            }
        }
    }

    bag.record_step(
        "command_skipped",
        "No running Companions found",
        0,
        StepResult::skipped("No running Companions to test command forwarding"),
    );

    Ok(bag)
}
