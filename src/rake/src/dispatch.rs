//! Command dispatch with middleware (RAKE-0011)
//!
//! `Runtime` builds the appropriate connection layer and context for
//! each command, then executes it. Connected commands get a `Resilient`
//! wrapper that automatically recovers from stale tending on TCP failure.

use std::sync::Arc;

use garden_rake::cli_build::GlobalFlags;
use garden_rake::commands::Command;
use garden_rake::connection::resilient::{self, Resilient};
use garden_rake::connection::resolution::{self, CachedStoneOps};
use garden_rake::context::{Context, OutputFormat};
use garden_rake::stone_cache::{STONE, STONE_ARC};
use garden_rake::ui::rendering::{self as ui, TerminalInfo};

// ============================================================================
// CommandInvocation
// ============================================================================

/// A fully-constructed command ready for the Runtime to execute.
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
// Runtime
// ============================================================================

/// Shared execution infrastructure for all commands.
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

    /// Execute a command invocation:
    ///
    /// 1. Resolve endpoint (Layer 3) + bind (Layer 2)
    /// 2. Wrap with resilience (Layer 1) for connected commands
    /// 3. On TCP failure: re-resolve, retry once (automatic via Resilient)
    pub async fn execute(&self, inv: CommandInvocation) -> anyhow::Result<()> {
        let cmd = inv.command;
        let at = inv.at;

        let output_format: OutputFormat = if self.global.field.is_some() {
            OutputFormat::Json
        } else {
            self.global.output.parse().unwrap_or_default()
        };

        if cmd.requires_endpoint() {
            let resolved = resolution::resolve(
                &self.client,
                at.as_deref(),
                Some(&*STONE as &dyn CachedStoneOps),
                None,
            )
            .await?;

            let stone = resilient::bind_stone(&self.client, resolved);

            let mut conn = Resilient::new(
                stone,
                self.client.clone(),
                at,
                Some(STONE_ARC.clone() as Arc<dyn CachedStoneOps>),
            );

            // Move shared data into Arc so the closure can clone it for each call.
            let cmd = Arc::new(cmd);
            let quiet = self.global.quiet;
            let fresh = self.global.fresh;
            let verbose = self.global.verbose;
            let supports_color = self.term.supports_color;
            let field = self.global.field.clone();

            conn.execute(|stone| {
                let cmd = Arc::clone(&cmd);
                let field = field.clone();
                Box::pin(async move {
                    if cmd.show_stone_header() && !output_format.is_json() {
                        let name = stone.name().await.unwrap_or("unknown");
                        println!("{}", ui::stone_name_banner(name, supports_color));
                        println!();
                    }

                    let stone_name = stone.name().await.map(|s| s.to_string());
                    let ctx = Context::from_stone(
                        stone,
                        stone_name,
                        stone.http().clone(),
                        quiet,
                        fresh,
                        verbose,
                        output_format,
                        field.clone(),
                    );
                    cmd.execute(&ctx).await
                })
            })
            .await
        } else {
            let ctx = Context::local(
                self.client.clone(),
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
