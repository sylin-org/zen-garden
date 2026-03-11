//! Return command - unregister a borrowed service
//!
//! Returns (unregisters) a borrowed external service.

use crate::command_manifest::cmd;
use crate::commands::{Command, CommandResult};
use crate::context::Runtime;
use crate::suggestions;
use async_trait::async_trait;
use garden_common::ui::rendering as ui;

/// Return (unregister) a borrowed service
pub struct ReturnCommand {
    pub name: String,
    pub quiet: bool,
}

impl ReturnCommand {
    pub fn new(name: String, quiet: bool) -> Self {
        Self { name, quiet }
    }
}

#[async_trait]
impl Command for ReturnCommand {
    async fn execute(&self, ctx: &Runtime) -> CommandResult {
        let name_path = urlencoding::encode(&self.name);
        let url = ctx.api_v1_url(&format!("stone/offerings/borrow/{}", name_path))?;
        let response = ctx.client.delete(&url).send().await?;
        let status = response.status();

        match status {
            s if s.is_success() => {
                println!(
                    "{}{} Returned borrowed service '{}'",
                    " ".repeat(ui::constants::DEFAULT_INDENT),
                    ui::status_indicator("ok", ctx.term.supports_color),
                    self.name
                );
            }
            reqwest::StatusCode::NOT_FOUND => {
                eprintln!(
                    "{}{} Service '{}' is not currently borrowed",
                    " ".repeat(ui::constants::DEFAULT_INDENT),
                    ui::status_indicator("error", ctx.term.supports_color),
                    self.name
                );
            }
            _ => {
                eprintln!(
                    "{}{} Failed to return: {}",
                    " ".repeat(ui::constants::DEFAULT_INDENT),
                    ui::status_indicator("error", ctx.term.supports_color),
                    status
                );
            }
        }

        // Self-teaching suggestions
        suggestions::print_suggestions(cmd::RETURN, self.quiet);

        Ok(())
    }

    fn name(&self) -> &'static str {
        cmd::RETURN
    }
}
