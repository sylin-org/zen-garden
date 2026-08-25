// Copyright (c) The Zen Garden Core Contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Nourish command - check and apply updates for offerings and firmware
//!
//! Uses execute_on_stone pattern to work across tended/discovered stones

use crate::commands::{Command, CommandResult};
use crate::context::Context;
use crate::tending;
use futures_util::StreamExt;
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

impl Command for NourishCommand {
    fn execute<'a>(&'a self, ctx: &'a Context) -> std::pin::Pin<Box<dyn std::future::Future<Output = CommandResult> + Send + 'a>> {
        Box::pin(async move {
            // Query garden for updates using execute_on_stone pattern
            let (nourishment_response, responding_stone) = tending::execute_on_stone(
                Duration::from_secs(3),
                Some(|stone_name: &str| {
                    println!("Stone '{}' is offline, trying fallback...", stone_name);
                }),
                |candidate| {
                    let client = ctx.client.clone();
                    let endpoint = candidate.endpoint.clone();
                    async move {
                        use crate::tending::StoneError;
                        use garden_common::client::StoneApi;

                        let api = StoneApi::new(client, endpoint);

                        let data: serde_json::Value = api.garden().updates().await.map_err(|e| {
                            match e {
                                garden_common::client::StoneApiError::Connection(ce) => {
                                    StoneError::ConnectionFailed(format!("HTTP request failed: {}", ce))
                                }
                                garden_common::client::StoneApiError::Http { status, message, .. } => {
                                    StoneError::ResponseError(status.as_u16(), message)
                                }
                                garden_common::client::StoneApiError::HttpRaw { status, body } => {
                                    StoneError::ResponseError(status.as_u16(), body)
                                }
                                other => StoneError::ProcessingError(format!("{}", other)),
                            }
                        })?;

                        let response: GardenNourishmentResponse = serde_json::from_value(data)
                            .map_err(|e| {
                                StoneError::ProcessingError(format!("JSON parse failed: {}", e))
                            })?;

                        Ok(ApiResponse { data: response, suggestions: None })
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
            let has_firmware =
                nourishment_response.data.stones.iter().any(|stone| {
                    stone.updates.available.iter().any(|update| {
                        matches!(update, garden_common::nourishment::Update::Firmware { .. })
                    })
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
        })
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

    let total_available: usize = response
        .stones
        .iter()
        .map(|s| s.updates.available.len())
        .sum();
    let total_blocked: usize = response
        .stones
        .iter()
        .map(|s| s.updates.blocked.len())
        .sum();

    println!(
        "Summary: {} available, {} blocked\n",
        total_available, total_blocked
    );
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
            // Separate by update type
            let moss_updates: Vec<_> = stone
                .updates
                .available
                .iter()
                .filter(|u: &&Update| matches!(*u, Update::Moss { .. }))
                .collect();

            let offerings: Vec<_> = stone
                .updates
                .available
                .iter()
                .filter(|u: &&Update| matches!(*u, Update::Offering { .. }))
                .collect();

            let firmware_tested: Vec<_> = stone
                .updates
                .available
                .iter()
                .filter(|u: &&Update| {
                    matches!(
                        *u,
                        Update::Firmware {
                            confidence: FirmwareConfidence::Tested,
                            ..
                        }
                    )
                })
                .collect();

            let firmware_suggested: Vec<_> = stone
                .updates
                .available
                .iter()
                .filter(|u: &&Update| {
                    matches!(
                        *u,
                        Update::Firmware {
                            confidence: FirmwareConfidence::Suggested,
                            ..
                        }
                    )
                })
                .collect();

            // Display Moss self-updates
            if !moss_updates.is_empty() {
                for update in moss_updates {
                    if let Update::Moss {
                        current, available, ..
                    } = update
                    {
                        println!("    Moss       {} → {}", current, available);
                    }
                }
            }

            // Display offerings
            if !offerings.is_empty() {
                println!("    📦 OFFERINGS:");
                for update in offerings {
                    if let Update::Offering {
                        name,
                        current,
                        available,
                        ..
                    } = update
                    {
                        println!("      • {} {} → {}", name, current, available);
                    }
                }
            }

            // Display tested firmware (from manifests)
            if !firmware_tested.is_empty() {
                println!("    🔧 FIRMWARE (tested):");
                for update in firmware_tested {
                    if let Update::Firmware {
                        name,
                        current,
                        available,
                        requires_reboot,
                        ..
                    } = update
                    {
                        let reboot = if *requires_reboot { " ⟲" } else { "" };
                        println!("      ✓ {} {} → {}{}", name, current, available, reboot);
                    }
                }
            }

            // Display suggested firmware (from LVFS, not in manifests)
            if !firmware_suggested.is_empty() {
                println!("    🔧 FIRMWARE (suggested):");
                for update in firmware_suggested {
                    if let Update::Firmware {
                        name,
                        current,
                        available,
                        requires_reboot,
                        ..
                    } = update
                    {
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
                    Update::Offering {
                        name,
                        current,
                        available,
                        ..
                    } => (name.as_str(), current.as_str(), available.as_str()),
                    Update::Firmware {
                        name,
                        current,
                        available,
                        ..
                    } => (name.as_str(), current.as_str(), available.as_str()),
                    Update::Moss {
                        current, available, ..
                    } => ("moss", current.as_str(), available.as_str()),
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
            stone
                .updates
                .available
                .iter()
                .any(|update: &Update| matches!(*update, Update::Firmware { .. }))
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
    ctx: &Context,
    _response: &GardenNourishmentResponse,
) -> anyhow::Result<()> {
    println!("\n🚀 Applying all updates...\n");
    execute_with_scope(ctx, UpdateScope::All).await
}

/// Execute offering updates only
async fn execute_offering_updates(
    ctx: &Context,
    _response: &GardenNourishmentResponse,
) -> anyhow::Result<()> {
    println!("\n🚀 Applying offering updates...\n");
    execute_with_scope(ctx, UpdateScope::Offerings).await
}

/// Execute firmware updates only
async fn execute_firmware_updates(
    ctx: &Context,
    _response: &GardenNourishmentResponse,
) -> anyhow::Result<()> {
    println!("\n🚀 Applying firmware updates...\n");
    execute_with_scope(ctx, UpdateScope::Firmware).await
}

/// Send scope to tended stone, which orchestrates to all affected stones
async fn execute_with_scope(ctx: &Context, scope: UpdateScope) -> anyhow::Result<()> {
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
            let endpoint = candidate.endpoint.trim_end_matches('/').to_string();
            let stone_name = candidate.stone_name.clone();
            let request = request.clone();
            async move {
                use crate::tending::StoneError;

                // Sign via the local Moss oracle (Stage 4), audience = the tended
                // stone's name; garden updates execute is enforced on /garden/.
                let api = garden_common::client::StoneApi::with_signing(
                    client,
                    endpoint,
                    garden_common::client::PondSigning {
                        sign_url: format!(
                            "http://127.0.0.1:{}/api/v1/pond/sign",
                            garden_common::constants::MOSS_SIGN_LOOPBACK
                        ),
                        audience: stone_name,
                    },
                );
                let body = serde_json::to_vec(&request).map_err(|e| {
                    StoneError::ProcessingError(format!("Failed to encode request: {}", e))
                })?;

                let response = api
                    .send_signed_raw(
                        reqwest::Method::POST,
                        "/api/v1/garden/updates/execute",
                        Some(body),
                    )
                    .await
                    .map_err(|e| {
                        StoneError::ConnectionFailed(format!("HTTP request failed: {}", e))
                    })?;

                let status = response.status();

                if !status.is_success() {
                    let body = response.text().await.unwrap_or_default();
                    return Err(StoneError::ResponseError(
                        status.as_u16(),
                        format!("Execute failed: {}", body),
                    ));
                }

                let text = response.text().await.map_err(|e| {
                    StoneError::ProcessingError(format!("Failed to read response: {}", e))
                })?;

                serde_json::from_str::<ApiResponse<GardenExecuteResponse>>(&text)
                    .map_err(|e| StoneError::ProcessingError(format!("JSON parse failed: {}", e)))
            }
        },
    )
    .await?;

    println!("Orchestrated by: {}", stone.stone_name);
    println!("Garden Job ID: {}\n", response.data.job_id);

    // Display results for each stone and collect streaming targets
    let mut stream_targets: Vec<(String, String, String)> = Vec::new(); // (stone_name, endpoint, job_id)

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

        // Collect running jobs for streaming
        if matches!(job.state, StoneJobState::Running | StoneJobState::Pending) {
            if let Some(ref job_id) = job.job_id {
                if let Some(ref endpoint) = job.endpoint {
                    stream_targets.push((
                        job.stone_name.clone(),
                        endpoint.clone(),
                        job_id.clone(),
                    ));
                }
            }
        }
    }

    // Stream real-time progress from each stone's nourishment job
    if !stream_targets.is_empty() {
        println!("\n─── Live progress ───────────────────────────────\n");
        stream_nourishment_jobs(&ctx.client, &stream_targets).await;
    }

    println!("\n✅ Nourishment complete");
    Ok(())
}

/// Stream real-time SSE progress from one or more stone nourishment jobs.
///
/// Connects to each stone's `/api/v1/stone/updates/stream/:job_id`
/// endpoint and prints status messages as they arrive. For multi-stone
/// gardens, messages are prefixed with the stone name.
async fn stream_nourishment_jobs(
    client: &reqwest::Client,
    targets: &[(String, String, String)], // (stone_name, endpoint, job_id)
) {
    let multi_stone = targets.len() > 1;

    // Stream sequentially (stones execute in parallel on the server side;
    // we display as events arrive from each stone in turn).
    for (stone_name, endpoint, job_id) in targets {
        let url = format!(
            "{}/api/v1/stone/updates/stream/{}",
            endpoint.trim_end_matches('/'),
            job_id
        );

        let prefix = if multi_stone {
            format!("  [{}] ", stone_name)
        } else {
            "  ".to_string()
        };

        match client
            .get(&url)
            .header("Accept", "text/event-stream")
            .timeout(Duration::from_secs(600)) // 10 min max for long image pulls
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                let mut stream = response.bytes_stream();
                let mut sse_buffer = String::new();

                while let Some(chunk) = stream.next().await {
                    match chunk {
                        Ok(bytes) => {
                            sse_buffer.push_str(&String::from_utf8_lossy(&bytes));

                            // Process complete SSE messages (delimited by \n\n)
                            while let Some(pos) = sse_buffer.find("\n\n") {
                                let message = sse_buffer[..pos].to_string();
                                sse_buffer.drain(..pos + 2);

                                // Extract data: lines from SSE message
                                for line in message.lines() {
                                    if let Some(data) = line.strip_prefix("data:") {
                                        let data = data.trim();
                                        if !data.is_empty() {
                                            println!("{}{}", prefix, data);
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("{}Stream error: {}", prefix, e);
                            break;
                        }
                    }
                }
            }
            Ok(response) => {
                // Job may have already completed (404) — not an error
                if response.status().as_u16() != 404 {
                    eprintln!(
                        "{}Failed to stream from {}: HTTP {}",
                        prefix,
                        stone_name,
                        response.status()
                    );
                }
            }
            Err(e) => {
                eprintln!("{}Failed to connect to {}: {}", prefix, stone_name, e);
            }
        }
    }
}
