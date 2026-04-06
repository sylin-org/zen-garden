//! Reconcile command - sync offerings with actual state
//!
//! Reconciles the offerings state with actual container state.

use crate::command_manifest::cmd;
use crate::commands::{Command, CommandResult};
use crate::context::Context;
use crate::suggestions;
use crate::ui::rendering as ui;

/// Reconcile offerings with actual container state
pub struct ReconcileCommand {
    pub drop_invalid: bool,
    pub quiet: bool,
}

impl ReconcileCommand {
    pub fn new(drop_invalid: bool, quiet: bool) -> Self {
        Self {
            drop_invalid,
            quiet,
        }
    }
}

impl Command for ReconcileCommand {
    fn execute<'a>(&'a self, ctx: &'a Context) -> std::pin::Pin<Box<dyn std::future::Future<Output = CommandResult> + Send + 'a>> {
        Box::pin(async move {
            let api = ctx.api();
            let payload = serde_json::json!({ "drop_invalid": self.drop_invalid });

            let body: serde_json::Value = api
                .stone()
                .reconcile(&payload)
                .await
                .map_err(|e| anyhow::anyhow!("Reconcile failed: {}", e.display_message()))?;

            let adopted = body
                .get("adopted")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let dropped = body
                .get("dropped_invalid")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let left = body
                .get("left_unregistered")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);

            println!(
                "{}{} Reconcile complete",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                ui::status_indicator("ok", ctx.term.supports_color)
            );
            println!(
                "{}  Adopted: {}",
                " ".repeat(ui::constants::DEFAULT_INDENT),
                adopted
            );
            if self.drop_invalid {
                println!("  Dropped invalid: {}", dropped);
            }
            if left > 0 {
                println!("  Left unregistered: {}", left);
            }

            // Self-teaching suggestions
            suggestions::print_suggestions(cmd::RECONCILE, self.quiet);

            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        cmd::RECONCILE
    }
}
