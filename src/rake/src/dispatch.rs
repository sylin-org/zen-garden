//! Command dispatch with middleware
//!
//! Handles common pre/post logic for command execution:
//! - Endpoint resolution (if required by command)
//! - Stone header display (if requested)
//! - Error handling and formatting
//!
//! Since Proposal B, the `Runtime` struct encapsulates the shared infrastructure
//! (client, global flags, cache) and provides `execute()` which replaces the old
//! 7-argument dispatch calls. `CommandInvocation` pairs a Command with its
//! per-invocation target stone.

use garden_common::ui::rendering::{self as ui, TerminalInfo};
use garden_common::{GardenApiResponse, HardwareCapabilities};
use garden_rake::cli_build::GlobalFlags;
use garden_rake::client::{resolve_target_endpoint, CachedStoneOps};
use garden_rake::commands::management::tend;
use garden_rake::commands::Command;
use garden_rake::context::{CommandContext, OutputFormat};
use garden_rake::discovery;
use garden_rake::stone_cache::GLOBAL_CACHE;
use garden_rake::tending;
use std::time::Duration;

// ============================================================================
// CommandInvocation — pairs a Command with its target stone
// ============================================================================

/// A fully-constructed command ready for the Runtime to execute.
///
/// Bundles the `Command` object with the optional target stone (`--at` / `on`).
/// This eliminates the pattern of manually extracting `at` 44 times in route.rs.
pub struct CommandInvocation {
    pub command: Box<dyn Command>,
    pub at: Option<String>,
}

impl CommandInvocation {
    /// Remote command: auto-extracts `--at` from Clap matches.
    pub fn remote(cmd: impl Command + 'static, matches: &clap::ArgMatches) -> Self {
        Self {
            command: Box::new(cmd),
            at: matches.get_one::<String>("at").cloned(),
        }
    }

    /// Remote command with explicit target stone.
    pub fn remote_at(cmd: impl Command + 'static, at: Option<String>) -> Self {
        Self {
            command: Box::new(cmd),
            at,
        }
    }

    /// Local command: no endpoint needed.
    pub fn local(cmd: impl Command + 'static) -> Self {
        Self {
            command: Box::new(cmd),
            at: None,
        }
    }
}

// ============================================================================
// Runtime — shared infrastructure built once per invocation (Proposal B)
// ============================================================================

/// Shared execution infrastructure for all commands.
///
/// Built once in `main()`, replaces the 7-argument `dispatch()` calls
/// that threaded `client`, `global.quiet`, `global.fresh`, `global.verbose`,
/// `GLOBAL_CACHE` to every handler (107 occurrences of `global.quiet` alone).
pub struct Runtime {
    pub client: reqwest::Client,
    pub global: GlobalFlags,
    pub term: TerminalInfo,
}

impl Runtime {
    pub fn new(client: reqwest::Client, global: GlobalFlags, term: TerminalInfo) -> Self {
        Self {
            client,
            global,
            term,
        }
    }

    /// Execute a command invocation with full middleware:
    /// 1. Resolve endpoint (if `cmd.requires_endpoint()`)
    /// 2. Print stone header (if `cmd.show_stone_header()`)
    /// 3. Build `CommandContext` with all global flags + automation options
    /// 4. Call `cmd.execute(&ctx)`
    pub async fn execute(&self, inv: CommandInvocation) -> anyhow::Result<()> {
        let cmd = inv.command;

        let output_format: OutputFormat = if self.global.field.is_some() {
            OutputFormat::Json
        } else {
            self.global.output.parse().unwrap_or_default()
        };

        if cmd.requires_endpoint() {
            let endpoint =
                resolve_endpoint(&self.client, inv.at, Some(&*GLOBAL_CACHE)).await?;

            if cmd.show_stone_header() && !output_format.is_json() {
                print_stone_header(&self.client, &endpoint).await;
            }

            let stone_name = fetch_stone_name(&self.client, &endpoint).await;
            let ctx = CommandContext::with_automation(
                self.client.clone(),
                Some(endpoint),
                stone_name,
                self.global.quiet,
                self.global.fresh,
                self.global.verbose,
                output_format,
                self.global.field.clone(),
            );
            cmd.execute(&ctx).await
        } else {
            let ctx = CommandContext::with_automation(
                self.client.clone(),
                None,
                None,
                self.global.quiet,
                self.global.fresh,
                self.global.verbose,
                output_format,
                self.global.field.clone(),
            );
            cmd.execute(&ctx).await
        }
    }
}

// ============================================================================
// Endpoint resolution + helpers
// ============================================================================

/// Resolve endpoint with priority: --at > env var > cached tending > auto-discover
///
/// This is the authoritative endpoint resolution logic used throughout rake.
/// Includes reachability checking for cached tending and automatic fallback.
pub async fn resolve_endpoint(
    client: &reqwest::Client,
    at: Option<String>,
    cache: Option<&dyn CachedStoneOps>,
) -> anyhow::Result<String> {
    let term = TerminalInfo::detect();

    // Priority 1: --at flag (explicit override, deterministic)
    if let Some(explicit) = at {
        let endpoint = resolve_target_endpoint(client, &explicit, cache).await?;
        return Ok(endpoint);
    }

    // Priority 2: GARDEN_STONE environment variable
    if let Ok(env_endpoint) = std::env::var(garden_common::ENV_GARDEN_STONE) {
        tracing::info!(endpoint = %env_endpoint, "Using GARDEN_STONE environment variable");
        let endpoint = resolve_target_endpoint(client, &env_endpoint, cache).await?;
        return Ok(endpoint);
    }

    // Priority 3: Cached tending state (no TTL - persists until stone unreachable)
    if let Ok(tending) = tending::read_tending() {
        tracing::debug!(
            stone = %tending.stone_name,
            endpoint = %tending.endpoint,
            age_secs = tending.age_seconds(),
            "Checking cached tending state"
        );

        // Check if stone is reachable before using cached endpoint
        if is_stone_reachable(client, &tending.endpoint).await {
            tracing::info!(
                stone = %tending.stone_name,
                endpoint = %tending.endpoint,
                "Using cached tending state"
            );
            return Ok(tending.endpoint);
        } else {
            // Stone is offline - warn user and fall through to discovery
            println!(
                "{}{} Stone \"{}\" is sleeping (offline). Picking a new stone...",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("warn", term.supports_color),
                tending.stone_name
            );
            tracing::warn!(
                stone = %tending.stone_name,
                endpoint = %tending.endpoint,
                "Tended stone unreachable, falling back to discovery"
            );
            // Don't clear tending - user might want to return to this stone later
        }
    }

    // Priority 4: Auto-discover via UDP broadcast + cache result
    tracing::debug!("No cached tending, attempting auto-discovery");
    println!(
        "{}{} Discovering stones...",
        " ".repeat(ui::constants::DEFAULT_INDENT),
        ui::status_indicator("info", term.supports_color)
    );

    match discovery::discover_moss().await {
        Ok(endpoint) => {
            tracing::info!(endpoint = %endpoint, "Auto-discovered stone");

            // Fetch capabilities to get stone name for cache and display
            let caps_url = format!(
                "{}/api/v1/stone/capabilities",
                endpoint.trim_end_matches('/')
            );
            if let Ok(resp) = client
                .get(&caps_url)
                .timeout(Duration::from_secs(5))
                .send()
                .await
            {
                if let Ok(response) = resp.json::<GardenApiResponse<HardwareCapabilities>>().await {
                    let stone_name = &response.data.stone_name;
                    let _ = tending::write_tending(stone_name.clone(), endpoint.clone());

                    // Show which stone was picked
                    println!(
                        "{}{} Now tending to \"{}\"",
                        " ".repeat(ui::constants::DEFAULT_INDENT),
                        ui::status_indicator("success", term.supports_color),
                        stone_name
                    );

                    // Notify stone of tending for visual feedback (glow/pulse)
                    // Create minimal context for notification (fire-and-forget)
                    let notify_ctx = CommandContext::without_endpoint(
                        client.clone(),
                        false, // quiet_mode
                        false, // fresh_mode
                        0,     // verbose
                    );
                    let _ = tend::notify_tending(&notify_ctx, &endpoint).await;
                }
            }

            Ok(endpoint)
        }
        Err(_) => Err(anyhow::anyhow!(
            "No Zen Garden stones discovered.\n\n\
            Possible causes:\n\
              • No stones present on your network\n\
              • Firewall is blocking UDP broadcast (port {})\n\
              • Stone's garden-moss service is not running\n\n\
            To fix:\n\
              • Create a new stone: Run installer/NewStone-linux-x64.ps1\n\
              • Set tending: garden-rake tend <endpoint>\n\
              • Specify endpoint manually: garden-rake <command> --at http://<IP>:{}\n\
              • Or use a stone name: garden-rake <command> --at <stone-name>\n\
              • Check stone status: ssh stone@<ip> systemctl status garden-moss.service",
            garden_common::constants::DISCOVERY_UDP,
            garden_common::constants::MOSS_HTTP,
        )),
    }
}

/// Check if a stone is reachable (quick health check)
async fn is_stone_reachable(client: &reqwest::Client, endpoint: &str) -> bool {
    let health_url = format!("{}/health", endpoint.trim_end_matches('/'));
    match client
        .get(&health_url)
        .timeout(Duration::from_secs(2))
        .send()
        .await
    {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

/// Print stone header banner
async fn print_stone_header(client: &reqwest::Client, endpoint: &str) {
    let term = TerminalInfo::detect();

    // Fetch stone capabilities to get name and health
    let caps_url = format!(
        "{}/api/v1/stone/capabilities",
        endpoint.trim_end_matches('/')
    );
    if let Ok(resp) = client
        .get(&caps_url)
        .timeout(Duration::from_secs(3))
        .send()
        .await
    {
        if let Ok(response) = resp.json::<GardenApiResponse<HardwareCapabilities>>().await {
            let stone_name = &response.data.stone_name;

            // Fetch health to get status
            let health_url = format!("{}/health", endpoint.trim_end_matches('/'));
            let health_status = if let Ok(health_resp) = client
                .get(&health_url)
                .timeout(Duration::from_secs(2))
                .send()
                .await
            {
                if let Ok(health_json) = health_resp.json::<serde_json::Value>().await {
                    // Map health to vitality language
                    if let Some(status) = health_json.get("status").and_then(|v| v.as_str()) {
                        match status {
                            garden_common::HEALTH_HEALTHY => garden_common::VITALITY_THRIVING,
                            garden_common::HEALTH_DEGRADED => {
                                garden_common::VITALITY_NEEDS_ATTENTION
                            }
                            garden_common::HEALTH_UNHEALTHY => garden_common::VITALITY_WITHERING,
                            _ => garden_common::VITALITY_DORMANT,
                        }
                    } else {
                        garden_common::VITALITY_THRIVING
                    }
                } else {
                    garden_common::VITALITY_DORMANT
                }
            } else {
                garden_common::VITALITY_DORMANT
            };

            println!(
                "{}",
                ui::stone_banner(stone_name, health_status, term.supports_color)
            );
            println!();
        }
    }
}

/// Fetch stone name from capabilities
async fn fetch_stone_name(client: &reqwest::Client, endpoint: &str) -> Option<String> {
    let caps_url = format!(
        "{}/api/v1/stone/capabilities",
        endpoint.trim_end_matches('/')
    );
    if let Ok(resp) = client
        .get(&caps_url)
        .timeout(Duration::from_secs(3))
        .send()
        .await
    {
        if let Ok(response) = resp.json::<GardenApiResponse<HardwareCapabilities>>().await {
            return Some(response.data.stone_name);
        }
    }
    None
}
