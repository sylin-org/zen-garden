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

        // Check if there are firmware updates
        let has_firmware = nourishment_response.data.stones.iter().any(|stone| {
            stone.updates.available.iter().any(|update| matches!(update, garden_common::nourishment::Update::Firmware { .. }))
        });

        // Interactive selection
        if !self.auto_confirm {
            use std::io::{self, Write};
            
            println!("\nApply updates:");
            println!("  [A] All updates");
            println!("  [O] Offerings only");
            if has_firmware {
                println!("  [F] Firmware only");
            }
            println!("  [ESC/Q] Cancel");
            print!("\nChoice: ");
            io::stdout().flush()?;
            
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            
            match input.trim().to_uppercase().as_str() {
                "A" => {
                    // Apply all updates
                    execute_all_updates(ctx, &nourishment_response.data).await?;
                }
                "O" => {
                    // Apply offering updates only
                    execute_offering_updates(ctx, &nourishment_response.data).await?;
                }
                "F" => {
                    // Apply firmware updates only
                    execute_firmware_updates(ctx, &nourishment_response.data).await?;
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
            execute_all_updates(ctx, &nourishment_response.data).await?;
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
    use garden_common::nourishment::FirmwareConfidence;
    
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
            // Separate offerings from firmware, and firmware by confidence
            let offerings: Vec<_> = stone.updates.available.iter()
                .filter(|u| matches!(u, Update::Offering { .. }))
                .collect();
            
            let firmware_tested: Vec<_> = stone.updates.available.iter()
                .filter_map(|u| match u {
                    Update::Firmware { confidence: FirmwareConfidence::Tested, .. } => Some(u),
                    _ => None,
                })
                .collect();
            
            let firmware_suggested: Vec<_> = stone.updates.available.iter()
                .filter_map(|u| match u {
                    Update::Firmware { confidence: FirmwareConfidence::Suggested, .. } => Some(u),
                    _ => None,
                })
                .collect();
            
            // Display offerings
            if !offerings.is_empty() {
                println!("    📦 OFFERINGS:");
                for update in offerings {
                    if let Update::Offering { name, current, available, .. } = update {
                        println!("      • {} {} → {}", name, current, available);
                    }
                }
            }
            
            // Display tested firmware (from manifests)
            if !firmware_tested.is_empty() {
                println!("    🔧 FIRMWARE (tested):");
                for update in firmware_tested {
                    if let Update::Firmware { name, current, available, requires_reboot, .. } = update {
                        let reboot = if *requires_reboot { " ⟲" } else { "" };
                        println!("      ✓ {} {} → {}{}", name, current, available, reboot);
                    }
                }
            }
            
            // Display suggested firmware (from LVFS, not in manifests)
            if !firmware_suggested.is_empty() {
                println!("    🔧 FIRMWARE (suggested):");
                for update in firmware_suggested {
                    if let Update::Firmware { name, current, available, requires_reboot, .. } = update {
                        let reboot = if *requires_reboot { " ⟲" } else { "" };
                        println!("      ○ {} {} → {}{}", name, current, available, reboot);
                    }
                }
            }
        }

        if !stone.updates.blocked.is_empty() {
            println!("    ⚠ BLOCKED:");
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
        println!("\n⟲ = reboot required");
        let has_firmware = response.stones.iter().any(|stone| {
            stone.updates.available.iter().any(|update| matches!(update, Update::Firmware { .. }))
        });
        if has_firmware {
            println!("Use [A] to apply all, [O] for offerings only, [F] for firmware only");
        } else {
            println!("Use [A] to apply all, [O] for offerings only");
        }
    }
}

// ============================================================================
// Execution Functions
// ============================================================================

/// Execute all updates (offerings + firmware)
async fn execute_all_updates(
    ctx: &CommandContext,
    _response: &GardenNourishmentResponse,
) -> anyhow::Result<()> {
    println!("\n🚀 Applying all updates...\n");
    execute_with_scope(ctx, UpdateScope::All).await
}

/// Execute offering updates only
async fn execute_offering_updates(
    ctx: &CommandContext,
    _response: &GardenNourishmentResponse,
) -> anyhow::Result<()> {
    println!("\n🚀 Applying offering updates...\n");
    execute_with_scope(ctx, UpdateScope::Offerings).await
}

/// Execute firmware updates only
async fn execute_firmware_updates(
    ctx: &CommandContext,
    _response: &GardenNourishmentResponse,
) -> anyhow::Result<()> {
    println!("\n🚀 Applying firmware updates...\n");
    execute_with_scope(ctx, UpdateScope::Firmware).await
}

/// Send scope to tended stone, which orchestrates to all affected stones
async fn execute_with_scope(
    ctx: &CommandContext,
    scope: UpdateScope,
) -> anyhow::Result<()> {
    // Build request payload
    let request = ExecuteRequest {
        scope,
        items: Vec::new(),
    };

    // Send to tended stone's garden execute endpoint
    let (response, stone) = tending::execute_on_stone(
        Duration::from_secs(30),
        Some(|stone_name: &str| {
            println!("Stone '{}' is offline, trying fallback...", stone_name);
        }),
        |candidate| {
            let client = ctx.client.clone();
            let endpoint = candidate.endpoint.clone();
            let request = request.clone();
            async move {
                use crate::tending::StoneError;
                
                let url = format!("{}/api/v1/garden/nourishment/execute", endpoint.trim_end_matches('/'));
                
                let response = client.post(&url)
                    .json(&request)
                    .send()
                    .await
                    .map_err(|e| StoneError::ConnectionFailed(format!("HTTP request failed: {}", e)))?;
                
                let status = response.status();
                
                if !status.is_success() {
                    let body = response.text().await.unwrap_or_default();
                    return Err(StoneError::ResponseError(
                        status.as_u16(),
                        format!("Execute failed: {}", body)
                    ));
                }
                
                let text = response.text().await
                    .map_err(|e| StoneError::ProcessingError(format!("Failed to read response: {}", e)))?;
                
                serde_json::from_str::<ApiResponse<GardenExecuteResponse>>(&text)
                    .map_err(|e| StoneError::ProcessingError(format!("JSON parse failed: {}", e)))
            }
        },
    ).await?;

    println!("Orchestrated by: {}", stone.stone_name);
    println!("Garden Job ID: {}\n", response.data.job_id);

    // Display results for each stone
    for job in &response.data.stone_jobs {
        let status_icon = match job.state {
            StoneJobState::Running => "🔄",
            StoneJobState::Success => "✅",
            StoneJobState::Failed => "❌",
            StoneJobState::Unreachable => "⚠️",
            StoneJobState::Pending => "⏳",
        };
        
        print!("  {} {}", status_icon, job.stone_name);
        if let Some(ref job_id) = job.job_id {
            print!(" (job: {})", job_id);
        }
        if let Some(ref msg) = job.message {
            print!(" - {}", msg);
        }
        println!();
    }

    // TODO: Stream status from each stone's job
    // For now, just show dispatch status
    
    println!("\n✅ Dispatch complete");
    Ok(())
}

