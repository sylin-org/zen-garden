// Copyright (c) The Zen Garden Core Contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Nourish command - check and apply updates for offerings and firmware
//!
//! Uses execute_on_stone pattern to work across tended/discovered stones

use crate::commands::{Command, CommandResult};
use crate::context::CommandContext;
use crate::tending;
use async_trait::async_trait;
use garden_common::api_utils::ApiResponse;
use garden_common::nourishment::*;
use std::time::Duration;

/// Nourish command for update management
pub struct NourishCommand {
    pub stone_name: Option<String>,
    pub updates_only: bool,
    pub auto_confirm: bool,
}

impl NourishCommand {
    pub fn new(stone_name: Option<String>, updates_only: bool, auto_confirm: bool) -> Self {
        Self {
            stone_name,
            updates_only,
            auto_confirm,
        }
    }
}

#[async_trait]
impl Command for NourishCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        // Query garden for updates using execute_on_stone pattern
        let (nourishment_response, responding_stone) = tending::execute_on_stone(
            Duration::from_secs(3),
            Some(|stone_name: &str| {
                println!("Stone '{}' is offline, trying fallback...", stone_name);
            }),
            |candidate| {
                let client = ctx.client.clone();
                let _stone_name = candidate.stone_name.clone();
                let endpoint = candidate.endpoint.clone();
                async move {
                    use crate::tending::StoneError;
                    
                    let url = format!("{}/api/v1/garden/nourishment", endpoint.trim_end_matches('/'));
                    
                    // Make HTTP request
                    let response = client.get(&url).send().await
                        .map_err(|e| StoneError::ConnectionFailed(format!("HTTP request failed: {}", e)))?;
                    
                    let status = response.status();
                    
                    // Check response status
                    if !status.is_success() {
                        return Err(StoneError::ResponseError(
                            status.as_u16(),
                            format!("Endpoint returned {}", status)
                        ));
                    }
                    
                    // Read response body
                    let text = response.text().await
                        .map_err(|e| StoneError::ProcessingError(format!("Failed to read response: {}", e)))?;
                    
                    // Parse JSON
                    serde_json::from_str::<ApiResponse<GardenNourishmentResponse>>(&text)
                        .map_err(|e| StoneError::ProcessingError(format!("JSON parse failed: {}. Body: {}", e, &text[..text.len().min(200)])))
                }
            },
        )
        .await?;

        display_nourishment(&nourishment_response.data, &responding_stone.stone_name);

        // Check if there are any available updates
        let total_available: usize = nourishment_response
            .data
            .stones
            .iter()
            .map(|s| s.updates.available.len())
            .sum();

        if total_available == 0 {
            println!("\n✅ All stones are up to date!");
            return Ok(());
        }

        // If updates-only flag, stop here
        if self.updates_only {
            return Ok(());
        }

        // Interactive selection
        if !self.auto_confirm {
            use std::io::{self, Write};
            
            println!("\nApply updates:");
            println!("  [A] All updates");
            println!("  [O] Offerings only");
            println!("  [S] Stone-specific (TODO)");
            println!("  [ESC/Q] Cancel");
            print!("\nChoice: ");
            io::stdout().flush()?;
            
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            
            match input.trim().to_uppercase().as_str() {
                "A" => {
                    // Apply all updates
                    execute_all_updates(ctx, &nourishment_response.data, &responding_stone).await?;
                }
                "O" => {
                    // Apply offering updates only
                    execute_offering_updates(ctx, &nourishment_response.data, &responding_stone).await?;
                }
                "S" => {
                    println!("\n⚠ Stone-specific selection not yet implemented");
                    return Ok(());
                }
                "" | "Q" | "ESC" | "\x1B" => {
                    println!("Cancelled.");
                    return Ok(());
                }
                _ => {
                    println!("Invalid choice. Cancelled.");
                    return Ok(());
                }
            }
        } else {
            // Auto-confirm: apply all
            execute_all_updates(ctx, &nourishment_response.data, &responding_stone).await?;
        }

        Ok(())
    }

    fn requires_endpoint(&self) -> bool {
        false // Uses execute_on_stone for discovery
    }

    fn show_stone_header(&self) -> bool {
        false
    }

    fn name(&self) -> &'static str {
        "nourish"
    }
}

// ============================================================================
// Display Logic
// ============================================================================

fn display_nourishment(response: &GardenNourishmentResponse, queried_stone: &str) {
    println!("\n╭─ Stone: {} ─╮", queried_stone);
    println!("\n📦 Garden-wide Update Status\n");

    let total_available: usize = response.stones.iter().map(|s| s.updates.available.len()).sum();
    let total_blocked: usize = response.stones.iter().map(|s| s.updates.blocked.len()).sum();

    println!("Summary: {} available, {} blocked\n", total_available, total_blocked);
    println!("───────────────────────────────────────────────\n");

    for stone in &response.stones {
        let has_updates = !stone.updates.available.is_empty();
        let has_blocked = !stone.updates.blocked.is_empty();

        if !has_updates && !has_blocked {
            println!("  {} [thriving] (no updates)", stone.stone_name);
            continue;
        }

        println!("  {}", stone.stone_name);

        if !stone.updates.available.is_empty() {
            println!("    AVAILABLE:");
            for update in &stone.updates.available {
                match update {
                    Update::Offering {
                        name,
                        current,
                        available,
                        ..
                    } => {
                        println!(
                            "      • {} {} → {}",
                            name, current, available
                        );
                    }
                    Update::Firmware {
                        name,
                        current,
                        available,
                        requires_reboot,
                        ..
                    } => {
                        let reboot = if *requires_reboot {
                            " (reboot required)"
                        } else {
                            ""
                        };
                        println!(
                            "      • {} {} → {}{}",
                            name, current, available, reboot
                        );
                    }
                }
            }
        }

        if !stone.updates.blocked.is_empty() {
            println!("    BLOCKED:");
            for blocked in &stone.updates.blocked {
                let (name, current, available) = match &blocked.update {
                    Update::Offering { name, current, available, .. } => (name, current, available),
                    Update::Firmware { name, current, available, .. } => (name, current, available),
                };
                println!(
                    "      ⚠ {} {} → {}: {}",
                    name, current, available, blocked.reason
                );
            }
        }

        println!();
    }

    println!("───────────────────────────────────────────────");
    if total_available > 0 {
        println!("\nUse [A] to apply all, [O] for offerings only");
    }
}

// ============================================================================
// Execution Functions
// ============================================================================

/// Execute all updates (offerings + firmware)
async fn execute_all_updates(
    ctx: &CommandContext,
    response: &GardenNourishmentResponse,
    stone: &tending::StoneCandidate,
) -> anyhow::Result<()> {
    println!("\n🚀 Applying all updates...\n");

    // Build update selectors from all available updates
    let mut selectors = Vec::new();
    
    for stone_resp in &response.stones {
        for update in &stone_resp.updates.available {
            match update {
                Update::Offering { name, .. } => {
                    selectors.push(UpdateSelector::Offering { name: name.clone() });
                }
                Update::Firmware { device_id, .. } => {
                    selectors.push(UpdateSelector::Firmware { device_id: device_id.clone() });
                }
            }
        }
    }

    if selectors.is_empty() {
        println!("No updates to apply.");
        return Ok(());
    }

    execute_and_stream(ctx, &stone.endpoint, selectors).await
}

/// Execute offering updates only
async fn execute_offering_updates(
    ctx: &CommandContext,
    response: &GardenNourishmentResponse,
    stone: &tending::StoneCandidate,
) -> anyhow::Result<()> {
    println!("\n🚀 Applying offering updates...\n");

    // Build update selectors from offerings only
    let mut selectors = Vec::new();
    
    for stone_resp in &response.stones {
        for update in &stone_resp.updates.available {
            if let Update::Offering { name, .. } = update {
                selectors.push(UpdateSelector::Offering { name: name.clone() });
            }
        }
    }

    if selectors.is_empty() {
        println!("No offering updates to apply.");
        return Ok(());
    }

    execute_and_stream(ctx, &stone.endpoint, selectors).await
}

/// Execute updates and stream status
async fn execute_and_stream(
    ctx: &CommandContext,
    endpoint: &str,
    selectors: Vec<UpdateSelector>,
) -> anyhow::Result<()> {
    use futures_util::StreamExt;
    
    // POST to execute endpoint
    let execute_url = format!("{}/api/v1/nourishment/execute", endpoint.trim_end_matches('/'));
    
    let request = ExecuteRequest { updates: selectors };
    
    let response = ctx.client
        .post(&execute_url)
        .json(&request)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to send execute request: {}", e))?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to execute updates: {}", response.status());
    }

    let api_response: ApiResponse<ExecuteResponse> = response.json().await
        .map_err(|e| anyhow::anyhow!("Failed to parse execute response: {}", e))?;
    let job_id = api_response.data.job_id;

    println!("Job ID: {}\n", job_id);

    // Stream status via SSE
    let stream_url = format!("{}/api/v1/nourishment/stream/{}", endpoint.trim_end_matches('/'), job_id);
    
    let response = ctx.client
        .get(&stream_url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to stream: {}", e))?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to open status stream: {}", response.status());
    }

    let mut stream = response.bytes_stream();
    
    loop {
        match tokio::time::timeout(Duration::from_secs(300), stream.next()).await {
            Ok(Some(Ok(bytes))) => {
                if let Ok(text) = std::str::from_utf8(&bytes) {
                    for line in text.lines() {
                        if let Some(data) = line.strip_prefix("data: ") {
                            println!("{}", data);
                            if data.contains("complete") || data.contains("Nourishment complete") {
                                return Ok(());
                            }
                        }
                    }
                }
            }
            Ok(Some(Err(e))) => {
                anyhow::bail!("Stream error: {}", e);
            }
            Ok(None) => {
                println!("\nStream ended.");
                return Ok(());
            }
            Err(_) => {
                anyhow::bail!("Timeout waiting for updates (300s)");
            }
        }
    }
}

