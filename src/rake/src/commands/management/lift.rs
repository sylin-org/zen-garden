//! Lift command - remove pond elements
//!
//! Zen syntax for removing pond security elements:
//! - lift keystone: Remove pond security entirely
//! - lift stone <name>: Remove a stone from the pond

use crate::commands::{Command, CommandResult};
use crate::context::Runtime;
use crate::suggestions;
use crate::ui::rendering as ui;
use garden_common::client::StoneApi;

/// Lift target type
pub enum LiftTarget {
    /// Lift the keystone (remove pond entirely)
    Keystone,
    /// Lift a stone from the pond
    Stone { name: String },
}

/// Lift command for removing pond elements
pub struct LiftCommand {
    pub target: LiftTarget,
    pub quiet: bool,
}

impl LiftCommand {
    pub fn new(target: LiftTarget, quiet: bool) -> Self {
        Self { target, quiet }
    }

    /// Create from CLI args
    pub fn from_args(
        target_type: String,
        stone_name: Option<String>,
        quiet: bool,
    ) -> anyhow::Result<Self> {
        let target = match target_type.as_str() {
            "keystone" => LiftTarget::Keystone,
            "stone" => {
                let name = stone_name
                    .ok_or_else(|| anyhow::anyhow!("Stone name required for 'lift stone'"))?;
                LiftTarget::Stone { name }
            }
            _ => anyhow::bail!(
                "Invalid target: '{}'. Use 'keystone' or 'stone'",
                target_type
            ),
        };
        Ok(Self::new(target, quiet))
    }
}

impl Command for LiftCommand {
    fn execute<'a>(&'a self, ctx: &'a Runtime) -> std::pin::Pin<Box<dyn std::future::Future<Output = CommandResult> + Send + 'a>> {
        Box::pin(async move {
            let api = ctx.stone_api()?;

            match &self.target {
                LiftTarget::Keystone => {
                    execute_lift_keystone(ctx, api).await?;
                }
                LiftTarget::Stone { name } => {
                    execute_lift_stone(ctx, api, name).await?;
                }
            }

            // Self-teaching suggestions
            suggestions::print_suggestions("lift", self.quiet);

            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        "lift"
    }
}

async fn execute_lift_keystone(ctx: &Runtime, api: &StoneApi) -> anyhow::Result<()> {
    match api.pond().drain().await {
        Ok(_) => {
            println!(
                "{}{} Keystone lifted (pond removed)",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("ok", ctx.term.supports_color)
            );
        }
        Err(
            garden_common::client::StoneApiError::HttpRaw { status, .. }
            | garden_common::client::StoneApiError::Http { status, .. },
        ) if status == reqwest::StatusCode::NOT_IMPLEMENTED => {
            println!(
                "{}{} Pond security not yet implemented (Phase 3b)",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("info", ctx.term.supports_color)
            );
            println!(
                "{}This command will remove pond security (lift the keystone).",
                " ".repeat(ui::constants::DEFAULT_INDENT)
            );
        }
        Err(e) => {
            eprintln!(
                "{}{} Failed to lift keystone: {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                e.display_message()
            );
        }
    }

    Ok(())
}

async fn execute_lift_stone(ctx: &Runtime, api: &StoneApi, stone_name: &str) -> anyhow::Result<()> {
    match api.pond().revoke(stone_name).await {
        Ok(_) => {
            println!(
                "{}{} Lifted {} from pond",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("ok", ctx.term.supports_color),
                stone_name
            );
        }
        Err(
            garden_common::client::StoneApiError::HttpRaw { status, .. }
            | garden_common::client::StoneApiError::Http { status, .. },
        ) if status == reqwest::StatusCode::NOT_IMPLEMENTED => {
            println!(
                "{}{} Pond security not yet implemented (Phase 3b)",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("info", ctx.term.supports_color)
            );
            println!(
                "{}This command will remove a stone from the pond trust network.",
                " ".repeat(ui::constants::DEFAULT_INDENT)
            );
        }
        Err(e) => {
            eprintln!(
                "{}{} Failed to lift stone: {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("error", ctx.term.supports_color),
                e.display_message()
            );
        }
    }

    Ok(())
}
