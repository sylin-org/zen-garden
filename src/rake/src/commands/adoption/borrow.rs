//! Borrow command - register an external service
//!
//! Registers an external network service for reference and discovery.

use crate::command_manifest::cmd;
use crate::commands::{Command, CommandResult};
use crate::context::Runtime;
use crate::suggestions;
use crate::ui::rendering as ui;
use async_trait::async_trait;

/// Borrow an external service
pub struct BorrowCommand {
    pub name: String,
    pub from_url: String,
    pub quiet: bool,
}

impl BorrowCommand {
    pub fn new(name: String, from_url: String, quiet: bool) -> Self {
        Self {
            name,
            from_url,
            quiet,
        }
    }
}

#[async_trait]
impl Command for BorrowCommand {
    async fn execute(&self, ctx: &Runtime) -> CommandResult {
        let url = ctx.api_v1_url("stone/offerings/borrow")?;
        let response = ctx
            .client
            .post(&url)
            .json(&serde_json::json!({
                "name": self.name,
                "url": self.from_url
            }))
            .send()
            .await?;
        let status = response.status();

        match status {
            s if s.is_success() => {
                println!(
                    "{}{} Borrowed service '{}' from {}",
                    " ".repeat(ui::constants::DEFAULT_INDENT),
                    ui::status_indicator("ok", ctx.term.supports_color),
                    self.name,
                    self.from_url
                );
            }
            reqwest::StatusCode::CONFLICT => {
                eprintln!(
                    "{}{} Service '{}' is already borrowed",
                    " ".repeat(ui::constants::DEFAULT_INDENT),
                    ui::status_indicator("warn", ctx.term.supports_color),
                    self.name
                );
            }
            reqwest::StatusCode::BAD_REQUEST => {
                eprintln!(
                    "{}{} Invalid URL: {}",
                    " ".repeat(ui::constants::DEFAULT_INDENT),
                    ui::status_indicator("error", ctx.term.supports_color),
                    self.from_url
                );
            }
            _ => {
                eprintln!(
                    "{}{} Failed to borrow: {}",
                    " ".repeat(ui::constants::DEFAULT_INDENT),
                    ui::status_indicator("error", ctx.term.supports_color),
                    status
                );
            }
        }

        // Self-teaching suggestions
        suggestions::print_suggestions(cmd::BORROW, self.quiet);

        Ok(())
    }

    fn name(&self) -> &'static str {
        cmd::BORROW
    }
}
