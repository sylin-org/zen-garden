//! Tend command - manage which stone to tend to
//!
//! The tend command manages the tending state, which determines
//! which stone commands target by default.

use crate::client::resolve_target_endpoint;
use crate::command_manifest::cmd::{self, tend_target};
use crate::commands::{Command, CommandResult};
use crate::context::Runtime;
use crate::discovery;
use crate::tending;
use garden_common::client::StoneApi;
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

impl Command for TendCommand {
    fn execute<'a>(&'a self, ctx: &'a Runtime) -> std::pin::Pin<Box<dyn std::future::Future<Output = CommandResult> + Send + 'a>> {
        Box::pin(async move {
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
                            format!("http://127.0.0.1:{}", garden_common::constants::MOSS_HTTP);

                        // Fast-skip: check if already tending to localhost
                        if let Ok(current) = tending::read_tending()
                            && current.endpoint == local_endpoint {
                                println!("Already tending to: {} (localhost)", current.stone_name);
                                return Ok(());
                            }

                        let api = StoneApi::new(ctx.client.clone(), local_endpoint.clone());

                        // Validate moss is running via health + fetch capabilities
                        match api.stone().capabilities_core().await {
                            Ok(caps) => {
                                tending::write_tending(
                                    caps.stone_name.clone(),
                                    local_endpoint.clone(),
                                    Some(caps.clone()),
                                )?;

                                // Notify stone of tending (for visual feedback in Companions)
                                let _ = notify_tending(&api).await;

                                println!("Now tending to: {} (localhost)", caps.stone_name);
                            }
                            Err(_) => {
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
                                let api = StoneApi::new(ctx.client.clone(), alternative.endpoint.clone());
                                match api.stone().capabilities_core().await {
                                    Ok(caps) => {
                                        tending::write_tending(
                                            caps.stone_name.clone(),
                                            alternative.endpoint.clone(),
                                            Some(caps.clone()),
                                        )?;

                                        // Notify stone of tending (for visual feedback in Companions)
                                        let _ = notify_tending(&api).await;

                                        println!(
                                            "Switched to {}.local ({})",
                                            caps.stone_name,
                                            alternative.endpoint.trim_start_matches("http://")
                                        );
                                    }
                                    Err(_) => {
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
                                let api = StoneApi::new(ctx.client.clone(), endpoint.clone());
                                let caps = api.stone().capabilities_core().await
                                    .map_err(|_| anyhow::anyhow!("No stones discovered on network"))?;
                                tending::write_tending(
                                    caps.stone_name.clone(),
                                    endpoint.clone(),
                                    Some(caps.clone()),
                                )?;

                                // Notify stone of tending (for visual feedback in Companions)
                                let _ = notify_tending(&api).await;

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
                        let api = StoneApi::new(ctx.client.clone(), url.to_string());
                        match api.stone().capabilities_core().await {
                            Ok(caps) => {
                                tending::write_tending(
                                    caps.stone_name.clone(),
                                    url.to_string(),
                                    Some(caps.clone()),
                                )?;

                                // Notify stone of tending (for visual feedback in Companions)
                                let _ = notify_tending(&api).await;

                                println!("Now tending to: {} ({})", caps.stone_name, url);
                            }
                            Err(_) => {
                                return Err(anyhow::anyhow!("Could not connect to endpoint: {}", url));
                            }
                        }
                    }
                    stone_name => {
                        // Resolve stone name (or simple host) to an endpoint
                        // Note: We don't use cache here since tend is a setup operation
                        let endpoint: String =
                            resolve_target_endpoint(&ctx.client, stone_name, None).await?;

                        let api = StoneApi::new(ctx.client.clone(), endpoint.clone());
                        match api.stone().capabilities_core().await {
                            Ok(caps) => {
                                tending::write_tending(
                                    caps.stone_name.clone(),
                                    endpoint.to_string(),
                                    Some(caps.clone()),
                                )?;

                                // Notify stone of tending (for visual feedback in Companions)
                                let _ = notify_tending(&api).await;

                                println!(
                                    "Now tending to: {}.local ({})",
                                    caps.stone_name,
                                    endpoint.trim_start_matches("http://")
                                );
                            }
                            Err(_) => {
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
                // Show current tending state - just read and display, no connectivity check
                match tending::read_tending() {
                    Ok(state) => {
                        if self.verbose {
                            println!(
                                "Tending to: {}.local ({})",
                                state.stone_name, state.endpoint
                            );
                            println!("Last updated: {} seconds ago", state.age_seconds());
                        } else {
                            println!(
                                "{}.local ({})",
                                state.stone_name,
                                state.endpoint.trim_start_matches("http://")
                            );
                        }
                    }
                    Err(_) => {
                        return Err(anyhow::anyhow!(
                            "Not tending to any stone.\n\n\
                            Options:\n\
                            • Auto-discover stone: garden-rake tend auto\n\
                            • Tend by name: garden-rake tend <stone-name>\n\
                            • Explicit endpoint: garden-rake tend http://<ip>:7185"
                        ));
                    }
                }
            }

            Ok(())
        })
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

/// Send tending notification to stone (for visual feedback)
///
/// POSTs to /api/v1/stone/presence/notify to trigger stone.tended event.
/// Companions (Firefly, Cricket) can react with temporary glow/pulse.
///
/// Used by:
/// - Explicit tend commands
/// - Auto-switch during dispatch when tended stone offline
pub async fn notify_tending(api: &StoneApi) -> anyhow::Result<()> {
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

    // Fire and forget - don't fail tending if notification fails
    let _ = api.stone().notify_tending(&notification).await;

    Ok(())
}
