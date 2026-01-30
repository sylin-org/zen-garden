//! Presence command - Stream real-time presence events from a stone
//!
//! Implements Stone Presence Protocol (PRESENCE-0001) client.
//! Connects to /api/v1/stone/presence/stream and displays events.

use anyhow::{Result, Context};
use garden_common::presence::{event_types, PresenceSnapshot, StoneState, ServiceState};

/// Stream presence events from a stone
pub async fn presence_command(
    _stone: Option<String>,
    at: Option<String>,
    client: &reqwest::Client,
    quiet_mode: bool,
    _fresh_mode: bool,
    _verbose: u8,
    cache: Option<&garden_common::client::stone_cache::StoneCache>,
) -> Result<()> {
    if quiet_mode {
        anyhow::bail!("Presence streaming requires interactive mode (cannot use --quiet)");
    }

    // Resolve endpoint using dispatch resolution logic
    let endpoint = if let Some(explicit_at) = at {
        // Explicit --at flag takes priority
        crate::client::resolve_target_endpoint(client, &explicit_at, cache.map(|c| c as &dyn crate::client::CachedStoneOps)).await?
    } else if let Ok(env_endpoint) = std::env::var("GARDEN_STONE") {
        // Environment variable
        env_endpoint
    } else {
        // Check tending state
        match crate::tending::read_tending() {
            Ok(tending_state) => tending_state.endpoint,
            Err(_) => {
                // Auto-discover
                let responses = crate::discovery::discover_moss_auto(std::time::Duration::from_secs(3)).await?;
                if responses.is_empty() {
                    anyhow::bail!("No stones discovered. Use --at to specify endpoint or run 'garden-rake tend'.");
                }
                responses[0].stone_endpoint.clone()
            }
        }
    };

    let url = format!("{}/api/v1/stone/presence/stream", endpoint.trim_end_matches('/'));

    println!("Connecting to presence stream: {}", url);
    println!("Press Ctrl+C to disconnect\n");

    // Create HTTP client for SSE with no timeout (stable connection)
    let sse_client = reqwest::Client::builder()
        .build()?;

    let response = sse_client
        .get(&url)
        .header("Accept", "text/event-stream")
        .send()
        .await?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to connect: HTTP {}", response.status());
    }

    // Process SSE events line by line
    use futures_util::StreamExt;
    let mut stream = response.bytes_stream();
    
    let mut current_event_type = String::new();
    let mut current_data = Vec::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let text = String::from_utf8_lossy(&chunk);
        
        for line in text.lines() {
            if line.starts_with("event:") {
                current_event_type = line[6..].trim().to_string();
            } else if line.starts_with("data:") {
                let data_part = line[5..].trim();
                if !current_data.is_empty() {
                    current_data.push(b'\n');
                }
                current_data.extend_from_slice(data_part.as_bytes());
            } else if line.is_empty() && !current_data.is_empty() {
                // Process complete event
                let data_str = String::from_utf8_lossy(&current_data).to_string();
                if let Err(e) = handle_presence_event(&current_event_type, &data_str) {
                    eprintln!("Error processing event: {}", e);
                }
                current_data.clear();
                current_event_type.clear();
            }
        }
    }

    Ok(())
}

/// Handle and display a presence event
fn handle_presence_event(event_type: &str, data: &str) -> Result<()> {
    match event_type {
        event_types::PRESENCE_SNAPSHOT => {
            let snapshot: PresenceSnapshot = serde_json::from_str(data)
                .context("Failed to parse snapshot")?;
            display_snapshot(&snapshot);
        }
        event_types::SERVICE_STARTED => {
            let parsed: serde_json::Value = serde_json::from_str(data)?;
            if let Some(service) = parsed.get("service").and_then(|s| s.as_str()) {
                println!("🌱 Service started: {}", service);
            }
        }
        event_types::SERVICE_STOPPED => {
            let parsed: serde_json::Value = serde_json::from_str(data)?;
            if let Some(service) = parsed.get("service").and_then(|s| s.as_str()) {
                println!("🛑 Service stopped: {}", service);
            }
        }
        event_types::SERVICE_SPROUTED => {
            let parsed: serde_json::Value = serde_json::from_str(data)?;
            if let Some(service) = parsed.get("service").and_then(|s| s.as_str()) {
                println!("✨ Service sprouted: {}", service);
            }
        }
        event_types::SERVICE_UPROOTED => {
            let parsed: serde_json::Value = serde_json::from_str(data)?;
            if let Some(service) = parsed.get("service").and_then(|s| s.as_str()) {
                println!("🗑️  Service uprooted: {}", service);
            }
        }
        event_types::STONE_LOAD_UPDATED => {
            let parsed: serde_json::Value = serde_json::from_str(data)?;
            if let Some(message) = parsed.get("message").and_then(|m| m.as_str()) {
                println!("📊 {}", message);
            }
        }
        event_types::STONE_HEALTH_CHANGED => {
            let parsed: serde_json::Value = serde_json::from_str(data)?;
            if let (Some(old), Some(new)) = (
                parsed.get("old").and_then(|o| o.as_str()),
                parsed.get("new").and_then(|n| n.as_str())
            ) {
                println!("❤️  Stone health changed: {} → {}", old, new);
            }
        }
        event_types::STONE_TENDED => {
            let parsed: serde_json::Value = serde_json::from_str(data)?;
            let by = parsed.get("by").and_then(|b| b.as_str()).unwrap_or("unknown");
            let from = parsed.get("from").and_then(|f| f.as_str()).unwrap_or("unknown");
            
            // Prominent visual feedback - Companions will show glow/pulse
            println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("👋 TENDING STARTED");
            println!("   From: {} on {}", by, from);
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
        }
        other => {
            // Unknown event, display raw
            println!("[{}] {}", other, data);
        }
    }

    Ok(())
}

/// Display initial snapshot
fn display_snapshot(snapshot: &PresenceSnapshot) {
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("📸 Presence Snapshot");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    display_stone_state(&snapshot.stone);
    
    if !snapshot.services.is_empty() {
        println!("\nServices ({}):", snapshot.services.len());
        for service in &snapshot.services {
            display_service_state(service);
        }
    } else {
        println!("\nNo services running");
    }
    
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("\nListening for events...\n");
}

/// Display stone state
fn display_stone_state(stone: &StoneState) {
    let health_icon = match stone.health.as_str() {
        "thriving" => "🌳",
        "withering" => "🥀",
        "wilting" => "💀",
        _ => "❓",
    };
    
    println!("\n{} Stone: {} ({})", health_icon, stone.name, stone.health);
    println!("  CPU:    {:.1}%", stone.cpu_percent);
    println!("  Memory: {:.1}%", stone.memory_percent);
    println!("  Disk:   {:.1}%", stone.disk_percent);
    
    let uptime_hours = stone.uptime_seconds / 3600;
    let uptime_minutes = (stone.uptime_seconds % 3600) / 60;
    println!("  Uptime: {}h {}m", uptime_hours, uptime_minutes);
}

/// Display service state
fn display_service_state(service: &ServiceState) {
    let state_icon = match service.state.as_str() {
        "running" => "✅",
        "stopped" => "⏹️ ",
        "exited" => "❌",
        _ => "❓",
    };
    
    println!("  {} {} ({})", state_icon, service.name, service.state);
}
