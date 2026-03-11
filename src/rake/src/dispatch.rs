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
use garden_rake::cli_build::GlobalFlags;
use garden_rake::client::{resolve_target_endpoint, CachedStoneOps};
use garden_rake::commands::management::tend;
use garden_rake::commands::Command;
use garden_rake::context::{CommandContext, OutputFormat};
use garden_rake::discovery;
use garden_rake::stone_bag::StoneBag;
use garden_rake::stone_cache::STONE;
use garden_rake::tending;

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
/// `STONE` to every handler (107 occurrences of `global.quiet` alone).
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
    ///
    /// 1. Resolve endpoint optimistically (no pre-flight health check)
    /// 2. Create [`StoneBag`] seeded from tending cache when available
    /// 3. Print stone header from cached bag data (if requested)
    /// 4. Build `CommandContext` and call `cmd.execute()`
    ///
    /// If the actual command fails with a connection error the caller is
    /// responsible for retry/discovery.  We intentionally do NOT pre-check
    /// reachability — if the request succeeds, the stone is alive.
    pub async fn execute(&self, inv: CommandInvocation) -> anyhow::Result<()> {
        let cmd = inv.command;

        let output_format: OutputFormat = if self.global.field.is_some() {
            OutputFormat::Json
        } else {
            self.global.output.parse().unwrap_or_default()
        };

        if cmd.requires_endpoint() {
            let endpoint =
                resolve_endpoint(&self.client, inv.at, Some(&*STONE)).await?;

            // Build bag — seeded from tending cache when the endpoint matches,
            // so stone_name() is free.  Cold path (--at, env, discovery) does
            // a single HTTP fetch on first access.
            let bag = self.build_bag(&endpoint);

            if cmd.show_stone_header() && !output_format.is_json() {
                let stone_name = bag.stone_name().await.unwrap_or("unknown");
                println!(
                    "{}",
                    ui::stone_name_banner(stone_name, self.term.supports_color)
                );
                println!();
            }

            let stone_name = bag.stone_name().await.map(|s| s.to_string());
            let ctx = CommandContext::with_automation(
                self.client.clone(),
                Some(bag.endpoint().to_string()),
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

    /// Build a [`StoneBag`] for the given endpoint, seeding from the
    /// tending cache when the endpoints match.
    fn build_bag(&self, endpoint: &str) -> StoneBag {
        if let Ok(state) = tending::read_tending() {
            if state.endpoint == endpoint && state.capabilities.is_some() {
                tracing::debug!(stone = %state.stone_name, "StoneBag: seeded from tending cache");
                return StoneBag::from_tending(&state, self.client.clone());
            }
        }
        StoneBag::new(self.client.clone(), endpoint.to_string())
    }
}

// ============================================================================
// Endpoint resolution + helpers
// ============================================================================

/// Resolve endpoint with priority: --at > env var > cached tending > auto-discover.
///
/// Priorities 1–3 are pure string resolution (no side effects beyond name
/// lookups).  Priority 4 triggers UDP discovery, writes tending state, and
/// notifies the stone — kept here so that callers that bypass `Runtime`
/// (launch, api, refresh, offer) still get auto-discovery for free.
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
    //
    // Optimistic dispatch: return the cached endpoint WITHOUT a health check.
    // Reachability is verified later by the StoneBag's capabilities fetch in
    // Runtime::execute(), which doubles as the probe — same round-trip that
    // proves the stone is alive also populates the cached capabilities.
    if let Ok(tending) = tending::read_tending() {
        tracing::info!(
            stone = %tending.stone_name,
            endpoint = %tending.endpoint,
            age_secs = tending.age_seconds(),
            "Using cached tending state (optimistic)"
        );
        return Ok(tending.endpoint);
    }

    // Priority 4: Auto-discover via UDP broadcast + cache result
    tracing::debug!("No cached tending, attempting auto-discovery");
    println!(
        "{}{} Discovering stones...",
        " ".repeat(ui::constants::DEFAULT_INDENT),
        ui::status_indicator("info", term.supports_color)
    );

    let endpoint = discovery::discover_moss().await.map_err(|_| {
        anyhow::anyhow!(
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
        )
    })?;

    tracing::info!(endpoint = %endpoint, "Auto-discovered stone");

    // Use StoneBag to fetch stone name for tending (single capabilities call)
    let bag = StoneBag::new(client.clone(), endpoint.clone());
    if let Some(name) = bag.stone_name().await {
        let caps = bag.capabilities_owned().await;
        let _ = tending::write_tending(name.to_string(), endpoint.clone(), caps);

        println!(
            "{}{} Now tending to \"{}\"",
            " ".repeat(ui::constants::DEFAULT_INDENT),
            ui::status_indicator("success", term.supports_color),
            name
        );

        // Notify stone of tending for visual feedback (glow/pulse)
        let notify_ctx = CommandContext::without_endpoint(
            client.clone(),
            false, // quiet
            false, // fresh
            0,     // verbose
        );
        let _ = tend::notify_tending(&notify_ctx, &endpoint).await;
    }

    Ok(endpoint)
}

