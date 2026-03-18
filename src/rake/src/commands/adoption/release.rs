//! Release command - release an adopted service
//!
//! Releases an adopted service back to the wild (stops managing, keeps running).

use crate::command_manifest::cmd;
use crate::commands::{Command, CommandResult};
use crate::context::Runtime;
use crate::suggestions;
use crate::ui::rendering as ui;
use async_trait::async_trait;

/// Release an adopted service
pub struct ReleaseCommand {
    pub service: String,
    pub quiet: bool,
}

impl ReleaseCommand {
    pub fn new(service: String, quiet: bool) -> Self {
        Self { service, quiet }
    }
}

#[async_trait]
impl Command for ReleaseCommand {
    async fn execute(&self, ctx: &Runtime) -> CommandResult {
        let service_path = urlencoding::encode(&self.service);
        let url = ctx.api_v1_url(&format!("stone/offerings/{}/adopt", service_path))?;
        let response = ctx.client.delete(&url).send().await?;
        let status = response.status();

        match status {
            s if s.is_success() => {
                println!(
                    "{}{} Released service '{}' (container still running)",
                    " ".repeat(ui::constants::DEFAULT_INDENT),
                    ui::status_indicator("ok", ctx.term.supports_color),
                    self.service
                );
            }
            reqwest::StatusCode::NOT_FOUND => {
                eprintln!(
                    "{}{} Service '{}' not found or not adopted",
                    " ".repeat(ui::constants::DEFAULT_INDENT),
                    ui::status_indicator("error", ctx.term.supports_color),
                    self.service
                );
            }
            _ => {
                eprintln!(
                    "{}{} Failed to release: {}",
                    " ".repeat(ui::constants::DEFAULT_INDENT),
                    ui::status_indicator("error", ctx.term.supports_color),
                    status
                );
            }
        }

        // Self-teaching suggestions
        suggestions::print_suggestions(cmd::RELEASE, self.quiet);

        Ok(())
    }

    fn name(&self) -> &'static str {
        cmd::RELEASE
    }
}
