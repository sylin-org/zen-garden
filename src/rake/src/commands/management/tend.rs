//! Tend command - manage which stone to tend to
//!
//! The tend command manages the tending state, which determines
//! which stone commands target by default.

use crate::client::resolve_target_endpoint;
use crate::command_manifest::cmd::{self, tend_target};
use crate::commands::{Command, CommandResult};
use crate::context::CommandContext;
use crate::discovery;
use crate::tending::{self, TendingState};
use async_trait::async_trait;
use garden_common::{GardenApiResponse, HardwareCapabilities};
use std::time::Duration;

/// Tend command - manage which stone to tend to
pub struct TendCommand {
    pub target: Option<String>,
    pub clear: bool,
    pub verbose: bool,
}

impl TendCommand {
    pub fn new(target: Option<String>, clear: bool, verbose: bool) -> Self {
        Self {
            target,
            clear,
            verbose,
        }
    }
}

#[async_trait]
impl Command for TendCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        if self.clear {
            tending::clear_tending()?;
            println!("Tending state cleared.");
            return Ok(());
        }

        if let Some(target_value) = &self.target {
            match target_value.as_str() {
                tend_target::THIS | tend_target::LOCAL => {
                    // Tend to localhost - validate moss is running
                    let local_endpoint =
                        format!("http://127.0.0.1:{}", garden_common::ports::MOSS_HTTP);
                    let health_url = format!("{}/health", local_endpoint);

                    match ctx
                        .client
                        .get(&health_url)
                        .timeout(Duration::from_millis(200))
                        .send()
                        .await
                    {
                        Ok(resp) if resp.status().is_success() => {
                            // Get stone name from capabilities
                            let caps_url = format!("{}/api/v1/stone/capabilities", local_endpoint);
                            let response: GardenApiResponse<HardwareCapabilities> = ctx
                                .client
                                .get(&caps_url)
                                .timeout(Duration::from_secs(5))
                                .send()
                                .await?
                                .json()
                                .await?;
                            let caps = response.data;
                            tending::write_tending(caps.stone_name.clone(), local_endpoint.clone())?;
                            
                            // Notify stone of tending (for visual feedback in adapters)
                            let _ = notify_tending(ctx, &local_endpoint).await;
                            
                            println!("Now tending to: {} (localhost)", caps.stone_name);
                        }
                        _ => {
                            return Err(anyhow::anyhow!(
                                "No local moss detected.\n\n\
                                Options:\n\
                                • Auto-discover stone: garden-rake tend auto\n\
                                • Explicit endpoint: garden-rake tend http://<ip>:7185"
                            ));
                        }
                    }
                }
                tend_target::ANOTHER => {
                    // Switch to another available stone
                    println!("Looking for alternative stones...");
                    match tending::discover_alternative_stone(Duration::from_secs(3)).await? {
                        Some(alternative) => {
                            // Validate and get stone name
                            let health_url = format!("{}/health", alternative.endpoint.trim_end_matches('/'));
                            match ctx
                                .client
                                .get(&health_url)
                                .timeout(Duration::from_secs(3))
                                .send()
                                .await
                            {
                                Ok(resp) if resp.status().is_success() => {
                                    let caps_url = format!("{}/api/v1/stone/capabilities", alternative.endpoint.trim_end_matches('/'));
                                    let response: GardenApiResponse<HardwareCapabilities> = ctx
                                        .client
                                        .get(&caps_url)
                                        .timeout(Duration::from_secs(5))
                                        .send()
                                        .await?
                                        .json()
                                        .await?;
                                    let caps = response.data;
                                    tending::write_tending(caps.stone_name.clone(), alternative.endpoint.clone())?;
                                    
                                    // Notify stone of tending (for visual feedback in adapters)
                                    let _ = notify_tending(ctx, &alternative.endpoint).await;
                                    
                                    println!(
                                        "Switched to {}.local ({})",
                                        caps.stone_name,
                                        alternative.endpoint.trim_start_matches("http://")
                                    );
                                }
                                _ => {
                                    return Err(anyhow::anyhow!(
                                        "Found alternative stone but it's not responding: {}",
                                        alternative.endpoint
                                    ));
                                }
                            }
                        }
                        None => {
                            // Check if we have a current stone
                            if let Ok(state) = tending::read_tending() {
                                println!(
                                    "No other stones found. Still tending to {}.local",
                                    state.stone_name
                                );
                            } else {
                                return Err(anyhow::anyhow!(
                                    "No stones found on network.\n\n\
                                    Options:\n\
                                    • Auto-discover stone: garden-rake tend auto\n\
                                    • Explicit endpoint: garden-rake tend http://<ip>:7185"
                                ));
                            }
                        }
                    }
                }
                tend_target::AUTO => {
                    // Force fresh discovery
                    tending::clear_tending()?;
                    println!("Discovering stones...");
                    match discovery::discover_moss().await {
                        Ok(endpoint) => {
                            // Get capabilities for stone name
                            let caps_url = format!("{}/api/v1/stone/capabilities", endpoint.trim_end_matches('/'));
                            let response: GardenApiResponse<HardwareCapabilities> = ctx
                                .client
                                .get(&caps_url)
                                .timeout(Duration::from_secs(5))
                                .send()
                                .await?
                                .json()
                                .await?;
                            let caps = response.data;
                            tending::write_tending(caps.stone_name.clone(), endpoint.clone())?;
                            
                            // Notify stone of tending (for visual feedback in adapters)
                            let _ = notify_tending(ctx, &endpoint).await;
                            
                            println!(
                                "  Found {}.local ({})",
                                caps.stone_name,
                                endpoint.trim_start_matches("http://")
                            );
                            println!("  Now tending to {}.local", caps.stone_name);
                        }
                        Err(_) => {
                            return Err(anyhow::anyhow!("No stones discovered on network"));
                        }
                    }
                }
                url if url.starts_with("http://") || url.starts_with("https://") => {
                    // Explicit endpoint - validate it
                    let health_url = format!("{}/health", url.trim_end_matches('/'));
                    match ctx
                        .client
                        .get(&health_url)
                        .timeout(Duration::from_secs(3))
                        .send()
                        .await
                    {
                        Ok(resp) if resp.status().is_success() => {
                            let caps_url = format!("{}/api/v1/stone/capabilities", url.trim_end_matches('/'));
                            let response: GardenApiResponse<HardwareCapabilities> = ctx
                                .client
                                .get(&caps_url)
                                .timeout(Duration::from_secs(5))
                                .send()
                                .await?
                                .json()
                                .await?;
                            let caps = response.data;
                            tending::write_tending(caps.stone_name.clone(), url.to_string())?;
                            
                            // Notify stone of tending (for visual feedback in adapters)
                            let _ = notify_tending(ctx, url).await;
                            
                            println!("Now tending to: {} ({})", caps.stone_name, url);
                        }
                        _ => {
                            return Err(anyhow::anyhow!(
                                "Could not connect to endpoint: {}",
                                url
                            ));
                        }
                    }
                }
                stone_name => {
                    // Resolve stone name (or simple host) to an endpoint
                    // Note: We don't use cache here since tend is a setup operation
                    let endpoint: String = resolve_target_endpoint(&ctx.client, stone_name, None).await?;

                    // Validate it and store tending state
                    let health_url = format!("{}/health", endpoint.trim_end_matches('/'));
                    match ctx
                        .client
                        .get(&health_url)
                        .timeout(Duration::from_secs(3))
                        .send()
                        .await
                    {
                        Ok(resp) if resp.status().is_success() => {
                            let caps_url = format!("{}/api/v1/stone/capabilities", endpoint.trim_end_matches('/'));
                            let caps: HardwareCapabilities = ctx
                                .client
                                .get(&caps_url)
                                .timeout(Duration::from_secs(5))
                                .send()
                                .await?
                                .json::<GardenApiResponse<HardwareCapabilities>>()
                                .await?
                                .data;
                            tending::write_tending(caps.stone_name.clone(), endpoint.to_string())?;
                            
                            // Notify stone of tending (for visual feedback in adapters)
                            let _ = notify_tending(ctx, &endpoint).await;
                            
                            println!(
                                "Now tending to: {}.local ({})",
                                caps.stone_name,
                                endpoint.trim_start_matches("http://")
                            );
                        }
                        _ => {
                            return Err(anyhow::anyhow!(
                                "Could not connect to stone '{}' ({})",
                                stone_name,
                                endpoint
                            ));
                        }
                    }
                }
            }
        } else {
            // Show current tending state - auto-discover if needed
            let tending_result: anyhow::Result<TendingState> = tending::read_tending();
            match tending_result {
                Ok(state) => {
                    // Tending persists until cleared or stone unreachable - verify stone is reachable
                    let health_url = format!("{}/health", state.endpoint.trim_end_matches('/'));
                    match ctx
                        .client
                        .get(&health_url)
                        .timeout(Duration::from_millis(500))
                        .send()
                        .await
                    {
                        Ok(resp) if resp.status().is_success() => {
                            if self.verbose {
                                println!(
                                    "Tending to: {}.local ({})",
                                    state.stone_name, state.endpoint
                                );
                                println!("Last activity: {} seconds ago", state.age_seconds());
                                println!("Status: Active (persistent, no TTL)");
                            } else {
                                println!(
                                    "{}.local ({})",
                                    state.stone_name,
                                    state.endpoint.trim_start_matches("http://")
                                );
                            }
                        }
                        _ => {
                            // Stone no longer reachable - try to rediscover
                            tracing::debug!(
                                "Tended stone {} no longer reachable, attempting rediscovery",
                                state.stone_name
                            );
                            if let Err(e) = auto_discover_and_tend(&ctx.client).await {
                                return Err(anyhow::anyhow!(
                                    "Previously tended stone '{}' is no longer reachable.\n\n{}\n\n\
                                    Options:\n\
                                    • Auto-discover stone: garden-rake tend auto\n\
                                    • Explicit endpoint: garden-rake tend http://<ip>:7185",
                                    state.stone_name,
                                    e
                                ));
                            }
                        }
                    }
                }
                Err(_) => {
                    // No cached state - auto-discover
                    tracing::debug!("No tending state found, attempting auto-discovery");
                    if let Err(e) = auto_discover_and_tend(&ctx.client).await {
                        return Err(anyhow::anyhow!(
                            "Not tending to any stone.\n\n{}\n\n\
                            Options:\n\
                            • Auto-discover stone: garden-rake tend auto\n\
                            • Explicit endpoint: garden-rake tend http://<ip>:7185",
                            e
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    fn requires_endpoint(&self) -> bool {
        false
    }

    fn show_stone_header(&self) -> bool {
        false
    }

    fn name(&self) -> &'static str {
        cmd::TEND
    }
}

/// Helper function to auto-discover and tend to a stone
async fn auto_discover_and_tend(client: &reqwest::Client) -> anyhow::Result<()> {
    // Try localhost first
    let local_endpoint = format!("http://127.0.0.1:{}", garden_common::ports::MOSS_HTTP);
    let health_url = format!("{}/health", local_endpoint);

    if let Ok(resp) = client
        .get(&health_url)
        .timeout(Duration::from_millis(200))
        .send()
        .await
    {
        if resp.status().is_success() {
            // Get stone name from capabilities
            let caps_url = format!("{}/api/v1/stone/capabilities", local_endpoint);
            if let Ok(response) = client
                .get(&caps_url)
                .timeout(Duration::from_secs(5))
                .send()
                .await
            {
                if let Ok(response) = response
                    .json::<GardenApiResponse<HardwareCapabilities>>()
                    .await
                {
                    let caps = response.data;
                    tending::write_tending(caps.stone_name.clone(), local_endpoint.clone())?;
                    println!("{}.local (localhost)", caps.stone_name);
                    return Ok(());
                }
            }
        }
    }

    // Try network discovery
    match discovery::discover_moss().await {
        Ok(endpoint) => {
            // Get capabilities for stone name
            let caps_url = format!("{}/api/v1/stone/capabilities", endpoint.trim_end_matches('/'));
            let response: GardenApiResponse<HardwareCapabilities> = client
                .get(&caps_url)
                .timeout(Duration::from_secs(5))
                .send()
                .await?
                .json()
                .await?;
            let caps = response.data;
            tending::write_tending(caps.stone_name.clone(), endpoint.clone())?;
            println!(
                "{}.local ({})",
                caps.stone_name,
                endpoint.trim_start_matches("http://")
            );
            Ok(())
        }
        Err(e) => Err(anyhow::anyhow!(
            "No stones found on network or localhost: {}",
            e
        )),
    }
}

/// Send tending notification to stone (for visual feedback)
/// 
/// POSTs to /api/v1/stone/presence/notify to trigger stone.tended event.
/// Adapters (Firefly, Cricket) can react with temporary glow/pulse.
/// 
/// Used by:
/// - Explicit tend commands
/// - Auto-switch during dispatch when tended stone offline
pub async fn notify_tending(ctx: &CommandContext, endpoint: &str) -> anyhow::Result<()> {
    use garden_common::presence::ClientNotification;
    
    // Get hostname for "from" field
    let hostname = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string());
    
    let notification = ClientNotification {
        event_type: "tended".to_string(),
        client: "rake".to_string(),
        from_host: Some(hostname.clone()),
        message: Some(format!("Tending from {}", hostname)),
    };
    
    let notify_url = format!("{}/api/v1/stone/presence/notify", endpoint.trim_end_matches('/'));
    
    // Fire and forget - don't fail tending if notification fails
    let _ = ctx.client
        .post(&notify_url)
        .json(&notification)
        .timeout(Duration::from_millis(500))
        .send()
        .await;
    
    Ok(())
}
