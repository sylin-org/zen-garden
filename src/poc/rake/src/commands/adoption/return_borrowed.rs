//! Return command - unregister a borrowed service
//!
//! Returns (unregisters) a borrowed external service.

use crate::command_manifest::cmd;
use crate::commands::{Command, CommandResult};
use crate::context::Context;
use crate::suggestions;
use crate::ui::rendering as ui;

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

impl Command for ReturnCommand {
    fn execute<'a>(&'a self, ctx: &'a Context) -> std::pin::Pin<Box<dyn std::future::Future<Output = CommandResult> + Send + 'a>> {
        Box::pin(async move {
            let name_path = urlencoding::encode(&self.name);
            // Signed via the local Moss oracle (Stage 4) when the target name is known.
            let response = ctx
                .api()
                .send_signed_raw(
                    reqwest::Method::DELETE,
                    &format!("/api/v1/stone/offerings/borrow/{}", name_path),
                    None,
                )
                .await?;
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
        })
    }

    fn name(&self) -> &'static str {
        cmd::RETURN
    }
}
