//! Make command - configure stone console mode
//!
//! Zen syntax for setting console verbosity:
//! - make stone sing: Set verbose mode
//! - make stone quiet: Set informative mode (default)
//! - make stone silent: Set silent mode
//! - make stone minimal: Set minimal mode (critical only)

use crate::command_manifest::cmd;
use crate::commands::{Command, CommandResult};
use crate::context::Runtime;
use crate::suggestions;
use crate::ui::rendering as ui;
use garden_common::client::StoneApiError;

/// Console mode action
pub enum MakeActionType {
    /// Set verbose mode (stone sings)
    Sing { forever: bool },
    /// Set informative mode (default)
    Quiet,
    /// Set silent mode
    Silent,
    /// Set minimal mode (critical only)
    Minimal,
}

/// Make command for configuring console mode
pub struct MakeCommand {
    pub action: MakeActionType,
    pub quiet: bool,
}

impl MakeCommand {
    pub fn new(action: MakeActionType, quiet: bool) -> Self {
        Self { action, quiet }
    }
}

impl Command for MakeCommand {
    fn execute<'a>(&'a self, ctx: &'a Runtime) -> std::pin::Pin<Box<dyn std::future::Future<Output = CommandResult> + Send + 'a>> {
        Box::pin(async move {
            let api = ctx.stone_api()?;

            match &self.action {
                MakeActionType::Sing { forever } => {
                    execute_make_mode(api, ctx, "verbose", *forever).await?;
                }
                MakeActionType::Quiet => {
                    execute_make_mode(api, ctx, "informative", true).await?;
                }
                MakeActionType::Silent => {
                    execute_make_mode(api, ctx, "silent", true).await?;
                }
                MakeActionType::Minimal => {
                    execute_make_mode(api, ctx, "minimal", true).await?;
                }
            }

            // Self-teaching suggestions
            suggestions::print_suggestions(cmd::MAKE, self.quiet);

            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        cmd::MAKE
    }
}

async fn execute_make_mode(
    api: &garden_common::client::StoneApi,
    ctx: &Runtime,
    mode: &str,
    persist: bool,
) -> anyhow::Result<()> {
    let timeout_minutes = if persist { 0 } else { 30 };

    let payload = serde_json::json!({
        "mode": mode,
        "persist": persist,
        "timeout_minutes": timeout_minutes
    });

    match api.stone().set_console_mode(&payload).await {
        Ok(_) => {
            let desc = match mode {
                "verbose" => {
                    if persist {
                        "Stone singing (verbose mode, permanent)"
                    } else {
                        "Stone singing (verbose mode, 30min timeout)"
                    }
                }
                "informative" => "Stone quieted (informative mode, permanent)",
                "silent" => "Stone silenced (silent mode, permanent)",
                "minimal" => "Stone set to minimal mode (critical only, permanent)",
                _ => "Console mode updated",
            };
            println!(
                "{}{} {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("ok", ctx.term.supports_color),
                desc
            );
        }
        Err(e) => {
            eprintln!(
                "{}{} Failed to set mode: {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                display_api_error(&e)
            );
        }
    }

    Ok(())
}

fn display_api_error(e: &StoneApiError) -> String {
    e.display_message()
}
